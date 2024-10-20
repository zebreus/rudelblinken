use anyhow::Result;
use rkyv::{Archive, Deserialize, Serialize};
use rudelblinken_sdk::host::Host;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use wasmi::{Caller, Engine, ExternType, Func, Linker, Memory, Module, Store, Val};

const WASM_MOD: &[u8] = include_bytes!(
    "../../rudelblinken-wasm/target/wasm32-unknown-unknown/release/rudelblinken_wasm.wasm"
);

fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env();

    let stdout_env_filter = env_filter.unwrap_or_else(|_| EnvFilter::new("info"));
    let stdout_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(std::io::stdout)
        .with_filter(stdout_env_filter);

    tracing_subscriber::registry().with(stdout_layer).init();

    let engine = Engine::default();
    let module = Module::new(&engine, WASM_MOD)?;

    type HostState = ();
    let mut store = Store::new(&engine, ());

    let linker = <Linker<HostState>>::new(&engine);
    let mut linker = linker;

    let name = "A NAME";

    for import in module.imports() {
        println!(
            "import: mod={}, name={}, ty={:?}",
            import.module(),
            import.name(),
            import.ty()
        );
        if import.module() != "env" {
            tracing::warn!(
                module = import.module(),
                name = import.name(),
                ty = ?import.ty(),
                "module has import from non-env module, ignoring",
            );
            continue;
        }
        // TODO(lmv): clean this up
        match (import.module(), import.name(), import.ty()) {
            ("env", "test", &ExternType::Func(ref ty)) => {
                let func = Func::wrap(
                    &mut store,
                    |mut caller: Caller<'_, HostState>, arg_ptr: u32| -> u32 {
                        tracing::info!("test");
                        let mut host = Host::from(caller);
                        let arg = host
                            .read_guest_value::<rudelblinken_sdk::common::TestArgument>(
                                arg_ptr as usize,
                            )
                            .expect("panic");
                        tracing::info!(?arg, "test");
                        let resp = rudelblinken_sdk::common::TestResult {
                            min_interval: 4,
                            max_interval: 5,
                            test_string: "magic".to_owned(),
                        };

                        host.write_value_to_guest_memory(&resp).expect("panic") as u32
                    },
                );
                linker.define("env", "test", func)?;
            }
            ("env", "get_name", &ExternType::Func(ref ty)) => {
                let func = Func::wrap(
                    &mut store,
                    |mut caller: Caller<'_, HostState>| -> (u32, u32) {
                        tracing::info!("get_name");
                        let name_bin = name.bytes().collect::<Vec<_>>();
                        let sptr = caller
                            .get_export("alloc")
                            .unwrap()
                            .into_func()
                            .unwrap()
                            .typed::<(u32,), u32>(&caller)
                            .expect("typed failed")
                            .call(&mut caller, (name_bin.len() as u32,))
                            .expect("alloc failed");
                        let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
                        /* mem.write(&mut caller, sptr as usize, &(sptr + 12).to_le_bytes())
                            .expect("write failed");
                        mem.write(
                            &mut caller,
                            sptr as usize + 4,
                            &name_bin.len().to_le_bytes(),
                        )
                        .expect("write failed");
                        mem.write(
                            &mut caller,
                            sptr as usize + 8,
                            &name_bin.len().to_le_bytes(),
                        )
                        .expect("write failed"); */
                        mem.write(&mut caller, sptr as usize, &name_bin)
                            .expect("write failed");
                        tracing::info!(?sptr, "get_name");
                        (sptr, name_bin.len() as u32)
                    },
                );
                linker.define("env", "get_name", func)?;
            }
            ("env", "set_led_brightness", &ExternType::Func(ref ty)) => {
                todo!()
            }
            ("env", "configure_ble_adv", &ExternType::Func(ref ty)) => {
                let func = Func::wrap(&mut store, |caller: Caller<'_, HostState>, arg_ptr: u32| {
                    tracing::info!("configure_ble_adv");
                    let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
                    let mem_ref = mem.data(&caller);
                    let off = arg_ptr as usize;
                    let ptr =
                        u32::from_le_bytes(mem_ref[off..off + 4].try_into().unwrap()) as usize;
                    let len =
                        u32::from_le_bytes(mem_ref[off + 8..off + 12].try_into().unwrap()) as usize;
                    let arg = &mem_ref[ptr..ptr + len];

                    let arg_val = rkyv::from_bytes::<BLEAdvSettings, rkyv::rancor::Error>(&arg)
                        .expect("from_bytes failed");
                    tracing::info!(?arg_val, "configure_ble_adv");
                });
                linker.define("env", "configure_ble_adv", func)?;
            }
            ("env", "configure_ble_data", &ExternType::Func(ref ty)) => {
                todo!()
            }
            ("env", "configure_ble_recv_callback", &ExternType::Func(ref ty)) => {
                todo!()
            }
            ("env", "host_log", &ExternType::Func(ref ty)) => {
                let func = Func::wrap(&mut store, |caller: Caller<'_, HostState>, arg_ptr: u32| {
                    tracing::info!("log");
                    let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
                    let mem_ref = mem.data(&caller);
                    let off = arg_ptr as usize;
                    let ptr =
                        u32::from_le_bytes(mem_ref[off..off + 4].try_into().unwrap()) as usize;
                    let len =
                        u32::from_le_bytes(mem_ref[off + 8..off + 12].try_into().unwrap()) as usize;
                    let arg = &mem_ref[ptr..ptr + len];

                    let arg_val = rkyv::from_bytes::<LogArgs, rkyv::rancor::Error>(&arg)
                        .expect("from_bytes failed");
                    tracing::info!(?arg_val, "log");
                });
                linker.define("env", "host_log", func)?;
            }
            ("env", name, &ExternType::Func(ref ty)) => {
                tracing::info!(
                    module = "env",
                    name,
                    ?ty,
                    "providing stub implementation for unkown function import"
                );
                let ty = ty.clone();

                let func = Func::new(&mut store, ty.clone(), move |_caller, _args, ret| {
                    for (i, ty) in ty.results().iter().enumerate() {
                        ret[i] = Val::default(ty.clone())
                    }
                    Ok(())
                });
                linker.define("env", name, func)?;
            }
            (module, name, ty) => {
                tracing::warn!(module, name, ?ty, "ignoring unkown import");
            }
        };
    }

    let pre_instance = linker.instantiate(&mut store, &module)?;
    let instance = pre_instance.start(&mut store)?;
    let add = instance.get_typed_func::<(), ()>(&store, "main")?;

    // And finally we can call the wasm!
    add.call(&mut store, ())?;
    println!("Hello, world!");
    Ok(())
}

#[derive(Archive, Deserialize, Serialize, Debug)]
struct LogArgs {
    msg: String,
}

#[derive(Archive, Deserialize, Serialize, Debug)]
struct BLEAdvSettings {
    min_interval: u32,
    max_interval: u32,
}
