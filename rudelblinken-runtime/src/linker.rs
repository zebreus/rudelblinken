use wasmi::{Caller, Config, Engine, Func, Instance, Linker, Memory, Module, Store};

use crate::host::{Host, LogLevel};

fn get_memory<'a, T: Host>(caller: &Caller<'a, T>) -> Result<Memory, wasmi::Error> {
    match caller.get_export("memory") {
        Some(wasmi::Extern::Memory(mem)) => Ok(mem),
        _ => Err(wasmi::Error::new(
            "memory not found. Does the guest export 'memory'?",
        )),
    }
}

fn get_slice<'a, T: Host>(
    memory: &Memory,
    caller: &'a Caller<'_, T>,
    offset: i32,
    length: i32,
) -> Result<&'a [u8], wasmi::Error> {
    let data = memory
        .data(caller)
        .get(offset as u32 as usize..)
        .ok_or(wasmi::Error::new("pointer out of bounds"))?
        .get(..length as u32 as usize)
        .ok_or(wasmi::Error::new("length out of bounds"))?;

    return Ok(data);
}

fn get_mut_slice<'a, T: Host>(
    memory: &Memory,
    caller: &'a mut Caller<'_, T>,
    offset: i32,
    length: i32,
) -> Result<&'a mut [u8], wasmi::Error> {
    let data = memory
        .data_mut(caller)
        .get_mut(offset as u32 as usize..)
        .ok_or(wasmi::Error::new("pointer out of bounds"))?
        .get_mut(..length as u32 as usize)
        .ok_or(wasmi::Error::new("length out of bounds"))?;

    return Ok(data);
}

pub struct LinkedHost<T: Host> {
    instance: Instance,
    store: Store<T>,
}

impl<T: Host> LinkedHost<T> {
    fn new(instance: Instance, store: Store<T>) -> Self {
        return LinkedHost { instance, store };
    }
    pub fn run(&mut self) -> Result<(), wasmi::Error> {
        let run = self
            .instance
            .get_typed_func::<(), ()>(&self.store, "rudel:base/run@0.0.1#run")?;
        run.call(&mut self.store, ())?;
        return Ok(());
    }
}

pub fn setup<T: Host>(wasm: &[u8], host: T) -> Result<LinkedHost<T>, wasmi::Error> {
    let engine = Engine::new(
        Config::default()
            .consume_fuel(true)
            .ignore_custom_sections(true),
    );
    let module = Module::new(&engine, wasm)?;

    let mut store = Store::new(&engine, host);
    store.set_fuel(99999).unwrap();

    let mut linker = <Linker<T>>::new(&engine);

    setup_linker(&mut linker, &mut store)?;

    let instance = linker.instantiate(&mut store, &module)?.start(&mut store)?;

    let linked_instance = LinkedHost::new(instance, store);
    return Ok(linked_instance);
}

/// Link the host functions provided by T.
///
/// This functions will provide the rudel-host functions to the linker by generating glue code for the functionality provided by the host implementation T
pub fn setup_linker<T: Host>(
    linker: &mut Linker<T>,
    mut store: &mut Store<T>,
) -> Result<(), wasmi::Error> {
    linker.define(
        "rudel:base/base@0.0.1",
        "log",
        Func::wrap(
            &mut store,
            |mut caller: Caller<'_, T>,
             level: i32,
             message_offset: i32,
             message_length: i32|
             -> Result<(), wasmi::Error> {
                let log_level = LogLevel::from_i32(level)?;

                let memory = get_memory(&caller)?;
                let data = get_slice(&memory, &caller, message_offset, message_length)?;
                let message: String = match std::str::from_utf8(data) {
                    Ok(s) => s.to_owned(),
                    Err(_) => return Err(wasmi::Error::new("invalid utf-8")),
                };

                caller.data_mut().log(log_level, message);

                return Ok(());
            },
        ),
    )?;

    linker.define(
        "rudel:base/base@0.0.1",
        "time",
        Func::wrap(
            &mut store,
            |mut caller: Caller<'_, T>| -> Result<u64, wasmi::Error> {
                let time = caller.data_mut().time();

                return Ok(time);
            },
        ),
    )?;

    linker.define(
        "rudel:base/base@0.0.1",
        "sleep",
        Func::wrap(
            &mut store,
            |mut caller: Caller<'_, T>, micros: u64| -> Result<(), wasmi::Error> {
                caller.data_mut().sleep(micros);
                return Ok(());
            },
        ),
    )?;

    linker.define(
        "rudel:base/base@0.0.1",
        "has-host-base",
        Func::wrap(
            &mut store,
            |mut caller: Caller<'_, T>, semantic_version_offset: i32| -> Result<(), wasmi::Error> {
                let base_version = caller.data_mut().get_base_version();
                let memory = get_memory(&caller)?;

                let major = get_mut_slice(&memory, &mut caller, semantic_version_offset + 0, 1)?;
                major[0] = base_version.major;
                let minor = get_mut_slice(&memory, &mut caller, semantic_version_offset + 1, 1)?;
                minor[0] = base_version.major;
                let patch = get_mut_slice(&memory, &mut caller, semantic_version_offset + 2, 1)?;
                patch[0] = base_version.major;

                return Ok(());
            },
        ),
    )?;

    linker.define(
        "rudel:base/base@0.0.1",
        "get-name",
        Func::wrap(
            &mut store,
            |mut caller: Caller<'_, T>, offset: i32| -> Result<(), wasmi::Error> {
                let name = caller.data().get_name();
                let memory = get_memory(&caller)?;
                let data = get_mut_slice(&memory, &mut caller, offset, 16)?;
                let name_bytes = name.as_bytes();
                data[..name_bytes.len()].copy_from_slice(name_bytes);
                data[name_bytes.len()..].fill(0);

                return Ok(());
            },
        ),
    )?;

    return Ok(());
}
