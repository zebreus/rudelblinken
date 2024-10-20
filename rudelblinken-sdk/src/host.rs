use std::cell::OnceCell;

use rkyv::Archive;
use wasmi::{Caller, Extern, Memory, TypedFunc, WasmParams, WasmResults, WasmTy};

use crate::common::Region;

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

impl<'a, HostState> Host<'a, HostState> {
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
            unsafe { Box::from_raw(reg_ptr as *mut Region) }
        };

        let data = self.memory()?.data_mut(&mut self.caller);

        let ptr = reg.ptr as usize;
        let len = reg.len as usize;
        let r = rkyv::from_bytes::<V, rkyv::rancor::Error>(&data[ptr..ptr + len])
            .expect("TODO: error handling");

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
        let enc = rkyv::to_bytes::<rkyv::rancor::Error>(value).expect("TODO: error handling");

        let mem = self.memory()?;

        let reg_off = self.alloc(enc.len())?;
        let reg = {
            let base_ptr = mem.data_ptr(&self.caller) as usize;
            let reg_ptr = base_ptr + reg_off;
            unsafe { Box::from_raw(reg_ptr as *mut Region) }
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
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to locate wasm memory")]
    MemoryNotFound,
    #[error("Failed to locate function with name '{0}'")]
    FunctionNotFound(String),
    #[error("Function with name '{0}' has incorrect types")]
    FunctionTypeMissmatch(String),
    #[error("Failed to allocate memory")]
    AllocFailure,
    #[error("Failed to deallocate memory")]
    DeallocFailure,
}
