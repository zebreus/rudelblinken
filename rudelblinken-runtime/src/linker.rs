mod glue;
mod linker;

use crate::host::Host;
use linker::{link_base, link_hardware};
use wasmi::{Config, Engine, Instance, Linker, Module, Store};

const MAJOR: u8 = 0;
const MINOR: u8 = 0;
const PATCH: u8 = 1;

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
    store: &mut Store<T>,
) -> Result<(), wasmi::Error> {
    link_base(linker, store)?;
    link_hardware(linker, store)?;

    return Ok(());
}
