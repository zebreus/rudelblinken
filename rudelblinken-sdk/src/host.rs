use std::cell::OnceCell;

use rkyv::Archive;
use tracing::{trace, warn};
use wasmi::{
    AsContext, AsContextMut, Caller, Extern, Instance, Memory, StoreContext, StoreContextMut,
    TypedFunc, WasmParams, WasmResults,
};

use crate::common;

pub trait HasExports {
    fn get_export(&self, name: &str) -> Option<Extern>;
}

impl<S> HasExports for Caller<'_, S> {
    fn get_export(&self, name: &str) -> Option<Extern> {
        self.get_export(name)
    }
}

pub struct InstanceWithContext<C: AsContext> {
    pub context: C,
    instance: Instance,
}

impl<C: AsContext> InstanceWithContext<C> {
    pub fn new(context: C, instance: Instance) -> Self {
        Self { context, instance }
    }
}

impl<C: AsContext> HasExports for InstanceWithContext<C> {
    fn get_export(&self, name: &str) -> Option<Extern> {
        self.instance.get_export(&self.context, name)
    }
}

impl<C: AsContext> AsContext for InstanceWithContext<C> {
    type Data = C::Data;

    fn as_context(&self) -> StoreContext<Self::Data> {
        self.context.as_context()
    }
}

impl<C: AsContextMut> AsContextMut for InstanceWithContext<C> {
    fn as_context_mut(&mut self) -> StoreContextMut<Self::Data> {
        self.context.as_context_mut()
    }
}

pub struct Host<I> {
    runtime_info: I,
    memory: OnceCell<Result<Memory, Error>>,
    alloc_fn: OnceCell<Result<TypedFunc<u32, u32>, Error>>,
    dealloc_fn: OnceCell<Result<TypedFunc<u32, ()>, Error>>,
    main_fn: OnceCell<Result<TypedFunc<(), ()>, Error>>,
    on_ble_adv_recv_fn: OnceCell<Result<TypedFunc<u32, ()>, Error>>,
}

const RESET_FUEL: u64 = 1 << 14;

impl<I: HasExports + AsContextMut> From<I> for Host<I> {
    fn from(runtime_info: I) -> Self {
        Self {
            runtime_info,
            memory: OnceCell::new(),
            alloc_fn: OnceCell::new(),
            dealloc_fn: OnceCell::new(),
            main_fn: OnceCell::new(),
            on_ble_adv_recv_fn: OnceCell::new(),
        }
    }
}

impl<HostState, I: HasExports + AsContextMut<Data = HostState>> Host<I> {
    // SAFETY: The actual lifetime of the pointer is until the given pointer is
    // deallocated
    unsafe fn recover_region(
        &mut self,
        reg_off: usize,
    ) -> Result<&'static mut common::Region, Error> {
        let mem = self.memory()?;
        let mem_len = mem.data(&self.runtime_info).len();

        if mem_len <= reg_off + size_of::<common::Region>() {
            // should be a pointer to a region, but its not pointing correctly into the wasm-controlled memory
            warn!(
                reg_ptr = reg_off,
                reg_len = size_of::<common::Region>(),
                mem_len,
                "region pointer leads out of bounds"
            );
            return Err(Error::BadRegionBox);
        }

        let base_ptr = mem.data_ptr(&self.runtime_info) as usize;
        let reg_ptr = base_ptr + reg_off;
        // I /think/ this cannot fail with the bounds check above, and as we
        // never drop the box (but instead leak it), there are also no issues
        // with deallocation of wasm-managed memory
        let reg = unsafe { Box::from_raw(reg_ptr as *mut common::Region) };
        Ok(Box::leak(reg))
    }

    // FIXME(lmv): make sure even a malicious guest (i.e. the pointer passed as
    // arg and dealloc are attacker controlled) cannot crash the host
    pub fn read_guest_value<V>(&mut self, reg_off: usize) -> Result<V, Error>
    where
        V: rkyv::Archive,
        <V as Archive>::Archived:
            rkyv::Deserialize<V, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
        <V as Archive>::Archived: for<'b> rkyv::bytecheck::CheckBytes<
            rkyv::api::high::HighValidator<'b, rkyv::rancor::Error>,
        >,
    {
        let reg = unsafe { self.recover_region(reg_off) }?;

        let mem = self.memory()?;
        let mem_len = mem.data(&self.runtime_info).len();

        let data = mem.data_mut(&mut self.runtime_info);

        let len = reg.len as usize;
        let ptr = reg.ptr as usize;
        if mem_len <= ptr + len {
            // should be a pointer to a buffer of length len, but its not
            // pointing correctly into the wasm-controlled memory
            warn!(
                reg_ptr = ptr,
                reg_len = len,
                mem_len,
                "region for reading a guest value is out of bounds"
            );
            return Err(Error::ReadFailure);
        }
        // FIXME(lmv): handle error
        let r = rkyv::from_bytes::<V, rkyv::rancor::Error>(&data[ptr..ptr + len])
            .expect("failed to deserialize value from guest");

        // FIXME(lmv): can we do the de-allocation on the host side? i.e. just
        // do a Vec::from_raw_parts and then drop the region and vector objects
        // instead of calling back to wasm?
        self.dealloc(reg_off)?;
        Ok(r)
    }

    // TODO(lmv): figure out if this is now save even with a malicious guest
    // (i.e. alloc is attacker controlled)
    pub fn write_value_to_guest_memory<V>(&mut self, value: &V) -> Result<usize, Error>
    where
        V: for<'b> rkyv::Serialize<
            rkyv::rancor::Strategy<
                rkyv::ser::Serializer<
                    rkyv::util::AlignedVec,
                    rkyv::ser::allocator::ArenaHandle<'b>,
                    rkyv::ser::sharing::Share,
                >,
                rkyv::rancor::Error,
            >,
        >,
    {
        // FIXME(lmv): handle error
        let enc = rkyv::to_bytes::<rkyv::rancor::Error>(value)
            .expect("failed to serialize value for guest");

        let mem = self.memory()?;

        let reg_off = self.alloc(enc.len())?;
        let reg = unsafe { self.recover_region(reg_off) }?;

        let data = mem.data_mut(&mut self.runtime_info);

        let len = reg.len as usize;
        if len != enc.len() {
            // alloc didn't create a region of the correct size
            warn!(
                reg_len = len,
                rkyv_len = enc.len(),
                "allocated region has wrong size"
            );
            return Err(Error::AllocFailure);
        }
        let ptr = reg.ptr as usize;
        if data.len() <= ptr + len {
            // should be a pointer to a buffer of length len, but its not
            // pointing correctly into the wasm-controlled memory
            warn!(
                reg_ptr = ptr,
                reg_len = len,
                mem_len = data.len(),
                "region allocated for writing to the guest leads out of bounds"
            );
            return Err(Error::AllocFailure);
        }
        data[ptr..(ptr + len)].clone_from_slice(&enc);

        Ok(reg_off)
    }

    fn memory(&self) -> Result<Memory, Error> {
        self.memory
            .get_or_init(|| {
                self.runtime_info
                    .get_export("memory")
                    .and_then(Extern::into_memory)
                    .ok_or(Error::MemoryNotFound)
            })
            .clone()
    }

    fn get_typed_func<P: WasmParams, R: WasmResults>(
        &self,
        name: &str,
    ) -> Result<TypedFunc<P, R>, Error> {
        self.runtime_info
            .get_export(name)
            .and_then(Extern::into_func)
            .ok_or_else(|| Error::FunctionNotFound(name.to_owned()))?
            .typed(&self.runtime_info)
            .map_err(|_| Error::FunctionTypeMissmatch(name.to_owned()))
    }

    // FIXME(lmv): Again, macros for generating this?
    pub fn on_ble_adv_recv(&mut self, arg: &common::BLEAdvNotification) -> Result<(), Error> {
        let arg = self
            .write_value_to_guest_memory(arg)
            .expect("failed to write arg value") as u32;
        self.reset_fuel();
        self.on_ble_adv_recv_fn()?
            .call(&mut self.runtime_info, arg)
            .map_err(|err| Error::FunctionCallFailure(format!("{:?}", err)))?;
        Ok(())
    }

    fn on_ble_adv_recv_fn(&self) -> Result<TypedFunc<u32, ()>, Error> {
        self.on_ble_adv_recv_fn
            .get_or_init(|| self.get_typed_func("on_ble_adv_recv"))
            .clone()
    }

    fn alloc(&mut self, len: usize) -> Result<usize, Error> {
        self.reset_fuel();
        let ptr = self
            .alloc_fn()?
            .call(&mut self.runtime_info, len as u32)
            .map_err(|_| Error::AllocFailure)?;
        Ok(ptr as usize)
    }

    fn alloc_fn(&self) -> Result<TypedFunc<u32, u32>, Error> {
        self.alloc_fn
            .get_or_init(|| self.get_typed_func("alloc_mem"))
            .clone()
    }

    fn dealloc(&mut self, ptr: usize) -> Result<(), Error> {
        self.reset_fuel();
        self.dealloc_fn()?
            .call(&mut self.runtime_info, ptr as u32)
            .map_err(|err| Error::DeallocFailure(format!("{:?}", err)))?;
        Ok(())
    }

    fn dealloc_fn(&self) -> Result<TypedFunc<u32, ()>, Error> {
        self.dealloc_fn
            .get_or_init(|| self.get_typed_func("dealloc_mem"))
            .clone()
    }

    pub fn main(&mut self) -> Result<(), Error> {
        self.reset_fuel();
        self.main_fn()?
            .call(&mut self.runtime_info, ())
            .map_err(|err| Error::MainFailure(format!("{:?}", err)))?;
        Ok(())
    }

    fn main_fn(&self) -> Result<TypedFunc<(), ()>, Error> {
        self.main_fn
            .get_or_init(|| self.get_typed_func("main"))
            .clone()
    }

    pub fn reset_fuel(&mut self) {
        // FIXME(lmv): error handling
        let mut ctx = self.runtime_info.as_context_mut();
        trace!(old_fuel = ?ctx.get_fuel(), "resetting fuel");
        ctx.set_fuel(RESET_FUEL).expect("failed to reset fuel");
    }

    pub fn get_runtime_info(self) -> I {
        self.runtime_info
    }
}

impl<HostState> Host<Caller<'_, HostState>> {
    pub fn state(&self) -> &HostState {
        self.runtime_info.data()
    }

    pub fn state_mut(&mut self) -> &mut HostState {
        self.runtime_info.data_mut()
    }
}

pub trait HostBase: Sized {
    // FIXME(lmv): improve this logging interface
    fn host_log(&mut self, log: common::Log);
    fn get_name(&mut self) -> String;

    fn get_time_millis(&mut self) -> u32;

    // Timeout (in microseconds) can be passed by the guest when explicitly
    // yielding.  It is 0 for implicit yields.
    //
    // host is terminated if the function returns false
    fn on_yield(host: &mut Host<Caller<'_, Self>>, timeout: u32) -> bool {
        true
    }

    fn has_host_base() -> bool {
        true
    }
}

pub trait LEDBrightness: HostBase {
    fn set_led_brightness(&mut self, settings: common::LEDBrightnessSettings);

    fn has_led_brightness() -> bool {
        true
    }
}

pub trait BLEAdv: HostBase {
    fn configure_ble_adv(&mut self, settings: common::BLEAdvSettings);
    fn configure_ble_data(&mut self, data: common::BLEAdvData);

    fn has_ble_adv() -> bool {
        true
    }
}

pub mod helper {
    use rkyv::Archive;
    use wasmi::{AsContextMut, Caller, Func, IntoFunc, Linker};

    use super::{BLEAdv, Host, HostBase, LEDBrightness};

    #[allow(dead_code)]
    fn wrap_fn_arg_ret<'b, HostState, F, A, R>(
        ifn: F,
    ) -> impl IntoFunc<HostState, (Caller<'b, HostState>, u32), Result<u32, wasmi::Error>>
    where
        HostState: HostBase,
        F: Fn(&mut HostState, A) -> R + Send + Sync + 'static,
        A: rkyv::Archive,
        <A as Archive>::Archived:
            rkyv::Deserialize<A, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
        <A as Archive>::Archived: for<'c> rkyv::bytecheck::CheckBytes<
            rkyv::api::high::HighValidator<'c, rkyv::rancor::Error>,
        >,
        R: for<'c> rkyv::Serialize<
            rkyv::rancor::Strategy<
                rkyv::ser::Serializer<
                    rkyv::util::AlignedVec,
                    rkyv::ser::allocator::ArenaHandle<'c>,
                    rkyv::ser::sharing::Share,
                >,
                rkyv::rancor::Error,
            >,
        >,
    {
        move |caller: Caller<'_, HostState>, arg_ptr: u32| {
            let mut host = Host::from(caller);
            // FIXME(lmv): handle error
            let arg = host
                .read_guest_value::<A>(arg_ptr as usize)
                .expect("failed to read argument value");
            let ret = ifn(host.runtime_info.data_mut(), arg);

            if !HostState::on_yield(&mut host, 0) {
                return Err(wasmi::Error::host(super::Error::YieldTermination));
            }
            host.reset_fuel();

            // FIXME(lmv): handle error
            Ok(host
                .write_value_to_guest_memory(&ret)
                .expect("failed to write return value") as u32)
        }
    }

    #[inline]
    fn wrap_fn_arg<'b, HostState, F, A>(
        ifn: F,
    ) -> impl IntoFunc<HostState, (Caller<'b, HostState>, u32), Result<(), wasmi::Error>>
    where
        HostState: HostBase,
        F: Fn(&mut HostState, A) + Send + Sync + 'static,
        A: rkyv::Archive,
        <A as Archive>::Archived:
            rkyv::Deserialize<A, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
        <A as Archive>::Archived: for<'c> rkyv::bytecheck::CheckBytes<
            rkyv::api::high::HighValidator<'c, rkyv::rancor::Error>,
        >,
    {
        move |caller: Caller<'_, HostState>, arg_ptr: u32| {
            let mut host = Host::from(caller);
            // FIXME(lmv): handle error
            let arg = host
                .read_guest_value::<A>(arg_ptr as usize)
                .expect("failed to read argument value");
            ifn(host.runtime_info.data_mut(), arg);

            if !HostState::on_yield(&mut host, 0) {
                return Err(wasmi::Error::host(super::Error::YieldTermination));
            }
            host.reset_fuel();
            Ok(())
        }
    }

    #[inline]
    fn wrap_fn_ret<'b, HostState, F, R>(
        ifn: F,
    ) -> impl IntoFunc<HostState, (Caller<'b, HostState>,), Result<u32, wasmi::Error>>
    where
        HostState: HostBase,
        F: Fn(&mut HostState) -> R + Send + Sync + 'static,
        R: for<'c> rkyv::Serialize<
            rkyv::rancor::Strategy<
                rkyv::ser::Serializer<
                    rkyv::util::AlignedVec,
                    rkyv::ser::allocator::ArenaHandle<'c>,
                    rkyv::ser::sharing::Share,
                >,
                rkyv::rancor::Error,
            >,
        >,
    {
        move |caller: Caller<'_, HostState>| {
            let mut host = Host::from(caller);
            let ret = ifn(host.runtime_info.data_mut());
            // FIXME(lmv): handle error

            if !HostState::on_yield(&mut host, 0) {
                return Err(wasmi::Error::host(super::Error::YieldTermination));
            }
            host.reset_fuel();
            Ok(host
                .write_value_to_guest_memory(&ret)
                .expect("failed to write return value") as u32)
        }
    }

    #[allow(dead_code)]
    #[inline]
    fn wrap_fn<'b, HostState, F>(
        ifn: F,
    ) -> impl IntoFunc<HostState, (Caller<'b, HostState>,), Result<(), wasmi::Error>>
    where
        HostState: HostBase,
        F: Fn(&mut HostState) + Send + Sync + 'static,
    {
        move |caller: Caller<'_, HostState>| {
            let mut host = Host::from(caller);
            ifn(host.runtime_info.data_mut());

            if !HostState::on_yield(&mut host, 0) {
                return Err(wasmi::Error::host(super::Error::YieldTermination));
            }
            host.reset_fuel();
            Ok(())
        }
    }

    #[inline]
    fn rt_yield<'b, HostState>(
    ) -> impl IntoFunc<HostState, (Caller<'b, HostState>, u32), Result<(), wasmi::Error>>
    where
        HostState: HostBase,
    {
        move |caller: Caller<'_, HostState>, timeout: u32| {
            let mut host = Host::from(caller);
            if !HostState::on_yield(&mut host, timeout) {
                return Err(wasmi::Error::host(super::Error::YieldTermination));
            }
            host.reset_fuel();
            Ok(())
        }
    }

    #[inline]
    fn wrap_fn_ret_plain<'b, HostState, F>(
        ifn: F,
    ) -> impl IntoFunc<HostState, (Caller<'b, HostState>,), Result<u32, wasmi::Error>>
    where
        HostState: HostBase,
        F: Fn(&mut HostState) -> u32 + Send + Sync + 'static,
    {
        move |caller: Caller<'_, HostState>| {
            let mut host = Host::from(caller);
            let ret = ifn(host.runtime_info.data_mut());

            if !HostState::on_yield(&mut host, 0) {
                return Err(wasmi::Error::host(super::Error::YieldTermination));
            }
            host.reset_fuel();
            Ok(ret)
        }
    }

    // TODO(lmv): maybe auto-generate these impl using a proc macro on the traits or
    // similar?

    pub fn prepare_link_host_base<HostState: HostBase + 'static>(
        mut ctx: impl AsContextMut<Data = HostState>,
        linker: &mut Linker<HostState>,
    ) -> Result<(), wasmi::errors::LinkerError> {
        linker.define(
            "env",
            "rt_yield",
            Func::wrap(&mut ctx, rt_yield::<HostState>()),
        )?;
        linker.define(
            "env",
            "get_time_millis",
            Func::wrap(&mut ctx, wrap_fn_ret_plain(HostState::get_time_millis)),
        )?;
        linker.define(
            "env",
            "has_host_base",
            Func::wrap(
                &mut ctx,
                wrap_fn_ret_plain(|_| HostState::has_host_base() as u32),
            ),
        )?;
        linker.define(
            "env",
            "host_log",
            Func::wrap(&mut ctx, wrap_fn_arg(HostState::host_log)),
        )?;
        linker.define(
            "env",
            "get_name",
            Func::wrap(&mut ctx, wrap_fn_ret(HostState::get_name)),
        )?;
        Ok(())
    }

    pub fn prepare_link_led_brightness<HostState: LEDBrightness + 'static>(
        mut ctx: impl AsContextMut<Data = HostState>,
        linker: &mut Linker<HostState>,
    ) -> Result<(), wasmi::errors::LinkerError> {
        linker.define(
            "env",
            "has_led_brightness",
            Func::wrap(
                &mut ctx,
                wrap_fn_ret_plain(|_| HostState::has_led_brightness() as u32),
            ),
        )?;
        linker.define(
            "env",
            "set_led_brightness",
            Func::wrap(&mut ctx, wrap_fn_arg(HostState::set_led_brightness)),
        )?;
        Ok(())
    }

    pub fn prepare_link_ble_adv<HostState: BLEAdv + 'static>(
        mut ctx: impl AsContextMut<Data = HostState>,
        linker: &mut Linker<HostState>,
    ) -> Result<(), wasmi::errors::LinkerError> {
        linker.define(
            "env",
            "has_ble_adv",
            Func::wrap(&mut ctx, || HostState::has_ble_adv() as u32),
        )?;
        linker.define(
            "env",
            "configure_ble_adv",
            Func::wrap(&mut ctx, wrap_fn_arg(HostState::configure_ble_adv)),
        )?;
        linker.define(
            "env",
            "configure_ble_data",
            Func::wrap(&mut ctx, wrap_fn_arg(HostState::configure_ble_data)),
        )?;
        Ok(())
    }

    pub fn prepare_link_stubs<HostState>(
        mut ctx: impl AsContextMut<Data = HostState>,
        linker: &mut Linker<HostState>,
        imports: wasmi::ModuleImportsIter,
    ) -> Result<(), wasmi::errors::LinkerError> {
        for import in imports {
            let module = import.module();
            let name = import.name();
            let ty = import.ty();
            if linker.get(&mut ctx, module, name).is_some() {
                continue;
            }

            match ty {
                wasmi::ExternType::Func(ref ty) => {
                    tracing::info!(
                        module,
                        name,
                        ?ty,
                        "providing stub implementation for unsupported function import"
                    );

                    let ty = ty.clone();
                    let func = Func::new(&mut ctx, ty.clone(), move |_caller, _args, ret| {
                        for (i, ty) in ty.results().iter().enumerate() {
                            ret[i] = wasmi::Val::default(*ty)
                        }
                        Ok(())
                    });
                    linker.define(module, name, func)?;
                }
                _ => {
                    tracing::info!(module, name, ?ty, "ignoring unkown non-function import");
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to locate wasm memory")]
    MemoryNotFound,
    #[error("Failed to locate function with name '{0}'")]
    FunctionNotFound(String),
    #[error("Function with name '{0}' has incorrect types")]
    FunctionTypeMissmatch(String),
    #[error("Failed to construct a box from a region pointer")]
    BadRegionBox,
    // FIXME(lmv): include more info about the error?
    #[error("Read from an invalid pointer")]
    ReadFailure,
    // FIXME(lmv): include more info about the error?
    #[error("Failed to call a function: {0}")]
    FunctionCallFailure(String),
    // FIXME(lmv): include more info about the error?
    #[error("Failed to allocate memory")]
    AllocFailure,
    #[error("Failed to deallocate memory: {0}")]
    DeallocFailure(String),
    #[error("Guest main failed: {0}")]
    MainFailure(String),
    #[error("Guest terminated because on_yield returned false")]
    YieldTermination,
}

impl wasmi::core::HostError for Error {}
