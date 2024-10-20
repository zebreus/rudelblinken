use std::cell::OnceCell;

use rkyv::Archive;
use wasmi::{
    AsContextMut, Caller, Extern, Func, IntoFunc, Linker, Memory, TypedFunc, WasmParams,
    WasmResults,
};

use crate::common;

pub struct Host<'a, HostState> {
    caller: Caller<'a, HostState>,
    memory: OnceCell<Result<Memory, Error>>,
    alloc_fn: OnceCell<Result<TypedFunc<u32, u32>, Error>>,
    dealloc_fn: OnceCell<Result<TypedFunc<u32, ()>, Error>>,
}

impl<'a, HostState> From<Caller<'a, HostState>> for Host<'a, HostState> {
    fn from(caller: Caller<'a, HostState>) -> Self {
        Self {
            caller,
            memory: OnceCell::new(),
            alloc_fn: OnceCell::new(),
            dealloc_fn: OnceCell::new(),
        }
    }
}

impl<HostState> Host<'_, HostState> {
    pub fn read_guest_value<V>(&mut self, reg_off: usize) -> Result<V, Error>
    where
        V: rkyv::Archive,
        <V as Archive>::Archived:
            rkyv::Deserialize<V, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
        <V as Archive>::Archived: for<'b> rkyv::bytecheck::CheckBytes<
            rkyv::api::high::HighValidator<'b, rkyv::rancor::Error>,
        >,
    {
        let mem = self.memory()?;
        let reg = {
            let base_ptr = mem.data_ptr(&self.caller) as usize;
            let reg_ptr = base_ptr + reg_off;
            unsafe { Box::from_raw(reg_ptr as *mut common::Region) }
        };

        let data = self.memory()?.data_mut(&mut self.caller);

        let ptr = reg.ptr as usize;
        let len = reg.len as usize;
        // FIXME(lmv): handle error
        let r = rkyv::from_bytes::<V, rkyv::rancor::Error>(&data[ptr..ptr + len])
            .expect("failed to deserialize value from guest");

        // forget the region, as we deallocate it in wasm
        std::mem::forget(reg);
        // FIXME(lmv): can we do the de-allocation on the host side? i.e. just
        // do a Vec::from_raw_parts and then drop the region and vector objects
        // instead of calling back to wasm?
        self.dealloc(reg_off)?;
        Ok(r)
    }

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
        let reg = {
            let base_ptr = mem.data_ptr(&self.caller) as usize;
            let reg_ptr = base_ptr + reg_off;
            unsafe { Box::from_raw(reg_ptr as *mut common::Region) }
        };

        let data = mem.data_mut(&mut self.caller);

        let ptr = reg.ptr as usize;
        let len = reg.len as usize;
        data[ptr..(ptr + len)].clone_from_slice(&enc);

        // forget the region, as we deallocate it in wasm
        std::mem::forget(reg);

        Ok(reg_off)
    }

    fn memory(&self) -> Result<Memory, Error> {
        self.memory
            .get_or_init(|| {
                self.caller
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
        self.caller
            .get_export(name)
            .and_then(Extern::into_func)
            .ok_or_else(|| Error::FunctionNotFound(name.to_owned()))?
            .typed(&self.caller)
            .map_err(|_| Error::FunctionTypeMissmatch(name.to_owned()))
    }

    fn alloc(&mut self, len: usize) -> Result<usize, Error> {
        let ptr = self
            .alloc_fn()?
            .call(&mut self.caller, len as u32)
            .map_err(|_| Error::AllocFailure)?;
        Ok(ptr as usize)
    }

    fn alloc_fn(&self) -> Result<TypedFunc<u32, u32>, Error> {
        self.alloc_fn
            .get_or_init(|| self.get_typed_func("alloc_mem"))
            .clone()
    }

    fn dealloc(&mut self, ptr: usize) -> Result<(), Error> {
        self.dealloc_fn()?
            .call(&mut self.caller, ptr as u32)
            .map_err(|_| Error::DeallocFailure)?;
        Ok(())
    }

    fn dealloc_fn(&self) -> Result<TypedFunc<u32, ()>, Error> {
        self.dealloc_fn
            .get_or_init(|| self.get_typed_func("dealloc_mem"))
            .clone()
    }

    fn wrap_fn_arg_ret<'b, F, A, R>(
        ifn: F,
    ) -> impl IntoFunc<HostState, (Caller<'b, HostState>, u32), u32>
    where
        F: Fn(&HostState, A) -> R + Send + Sync + 'static,
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
            let ret = ifn(host.caller.data(), arg);
            // FIXME(lmv): handle error
            host.write_value_to_guest_memory(&ret)
                .expect("failed to write return value") as u32
        }
    }

    fn wrap_fn_arg<'b, F, A>(ifn: F) -> impl IntoFunc<HostState, (Caller<'b, HostState>, u32), ()>
    where
        F: Fn(&HostState, A) + Send + Sync + 'static,
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
            ifn(host.caller.data(), arg);
        }
    }

    fn wrap_fn_ret<'b, F, R>(ifn: F) -> impl IntoFunc<HostState, (Caller<'b, HostState>,), u32>
    where
        F: Fn(&HostState) -> R + Send + Sync + 'static,
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
            let ret = ifn(host.caller.data());
            // FIXME(lmv): handle error
            host.write_value_to_guest_memory(&ret)
                .expect("failed to write return value") as u32
        }
    }
}

pub trait HostBase {
    // FIXME(lmv): improve this logging interface
    fn host_log(&self, log: common::Log);
    fn get_name(&self) -> String;

    fn has_host_base() -> bool {
        true
    }
}

pub trait LEDBrightness {
    fn set_led_brightness(&self, settings: common::LEDBrightnessSettings);

    fn has_led_brightness() -> bool {
        true
    }
}

pub trait BLEAdv {
    fn configure_ble_adv(&self, settings: common::BLEAdvSettings);
    fn configure_ble_data(&self, data: common::BLEAdvData);

    fn has_ble_adv() -> bool {
        true
    }
}

// TODO(lmv): maybe auto-generate these impl using a proc macro on the traits or
// similar?

impl<HostState> Host<'_, HostState>
where
    HostState: HostBase + 'static,
{
    pub fn prepare_link_host_base(
        mut ctx: impl AsContextMut<Data = HostState>,
        linker: &mut Linker<HostState>,
    ) -> Result<(), wasmi::errors::LinkerError> {
        linker.define(
            "env",
            "has_host_base",
            Func::wrap(&mut ctx, || HostState::has_host_base() as u32),
        )?;
        linker.define(
            "env",
            "host_log",
            Func::wrap(&mut ctx, Self::wrap_fn_arg(HostState::host_log)),
        )?;
        linker.define(
            "env",
            "get_name",
            Func::wrap(&mut ctx, Self::wrap_fn_ret(HostState::get_name)),
        )?;
        Ok(())
    }
}

impl<HostState> Host<'_, HostState>
where
    HostState: LEDBrightness + 'static,
{
    pub fn prepare_link_led_brightness(
        mut ctx: impl AsContextMut<Data = HostState>,
        linker: &mut Linker<HostState>,
    ) -> Result<(), wasmi::errors::LinkerError> {
        linker.define(
            "env",
            "has_led_brightness",
            Func::wrap(&mut ctx, || HostState::has_led_brightness() as u32),
        )?;
        linker.define(
            "env",
            "set_led_brightness",
            Func::wrap(&mut ctx, Self::wrap_fn_arg(HostState::set_led_brightness)),
        )?;
        Ok(())
    }
}

impl<HostState> Host<'_, HostState>
where
    HostState: BLEAdv + 'static,
{
    pub fn prepare_link_ble_adv(
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
            Func::wrap(&mut ctx, Self::wrap_fn_arg(HostState::configure_ble_adv)),
        )?;
        linker.define(
            "env",
            "configure_ble_data",
            Func::wrap(&mut ctx, Self::wrap_fn_arg(HostState::configure_ble_data)),
        )?;
        Ok(())
    }
}

impl<HostState> Host<'_, HostState> {
    pub fn prepare_link_stubs(
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
    // FIXME(lmv): include more info about the error?
    #[error("Failed to allocate memory")]
    AllocFailure,
    // FIXME(lmv): include more info about the error?
    #[error("Failed to deallocate memory")]
    DeallocFailure,
}
