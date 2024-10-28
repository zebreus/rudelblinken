use anyhow::Result;
use rudelblinken_sdk::{
    common::{self, BLEAdvNotification},
    host::{
        helper::{
            prepare_link_ble_adv, prepare_link_host_base, prepare_link_led_brightness,
            prepare_link_stubs,
        },
        BLEAdv, Host, HostBase, InstanceWithContext, LEDBrightness,
    },
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use wasmi::{Engine, Linker, Module, Store};

const WASM_MOD: &[u8] = include_bytes!(
    "../../rudelblinken-wasm/target/wasm32-unknown-unknown/release/rudelblinken_wasm.wasm"
);

struct HostState {
    name: String,
}

impl HostBase for HostState {
    fn host_log(&self, log: common::Log) {
        match log.level {
            common::LogLevel::Error => tracing::error!(msg = &log.message, "guest logged"),
            common::LogLevel::Warn => tracing::warn!(msg = &log.message, "guest logged"),
            common::LogLevel::Info => tracing::info!(msg = &log.message, "guest logged"),
            common::LogLevel::Debug => tracing::debug!(msg = &log.message, "guest logged"),
            common::LogLevel::Trace => tracing::trace!(msg = &log.message, "guest logged"),
        }
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}

impl LEDBrightness for HostState {
    fn set_led_brightness(&self, settings: common::LEDBrightnessSettings) {
        tracing::info!(?settings, "guest set led bightness")
    }
}

impl BLEAdv for HostState {
    fn configure_ble_adv(&self, settings: common::BLEAdvSettings) {
        tracing::info!(?settings, "guest configured ble_adv")
    }

    fn configure_ble_data(&self, data: common::BLEAdvData) {
        tracing::info!(?data, "guest set ble_adv data")
    }
}

fn main() -> Result<()> {
    // TODO (next steps):
    // - add a cli interface for specifying (multiple) wasm binaries to run
    // - build a sync-able simulation;
    //  - impl ble advertisment propergation, triggering sends at a random time
    //    respecting the configured delays (initially always deliver instantly
    //    to everyone, package delay and loss can be implemented later)
    //  - implement led brightness info (log when passing thresholds or actually
    //    show brightness with dots visually)
    // - make the simulation fancy
    //   - allow positioning nodes in space, do package loss based on distance
    //   - implement (noisy) propagation delay
    //   - cool visualization
    //   - ...
    // along the way, maybe make the sdk a bit nicer

    let env_filter = EnvFilter::try_from_default_env();

    let stdout_env_filter = env_filter.unwrap_or_else(|_| EnvFilter::new("info"));
    let stdout_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(std::io::stdout)
        .with_filter(stdout_env_filter);

    tracing_subscriber::registry().with(stdout_layer).init();

    let engine = Engine::default();
    let module = Module::new(&engine, WASM_MOD)?;

    let mut store = Store::new(
        &engine,
        HostState {
            name: "lgcl".to_owned(),
        },
    );

    let linker = <Linker<HostState>>::new(&engine);
    let mut linker = linker;

    // FIXME(lmv): can we somehow call all prepare functions the host supports
    // with the given host state?
    prepare_link_host_base(&mut store, &mut linker).expect("failed to link hos base");
    prepare_link_led_brightness(&mut store, &mut linker).expect("failed to link led brightness");
    prepare_link_ble_adv(&mut store, &mut linker).expect("failed to link ble adv");
    prepare_link_stubs(&mut store, &mut linker, module.imports()).expect("failed to link stubs");

    let pre_instance = linker.instantiate(&mut store, &module)?;
    let instance = pre_instance.start(&mut store)?;
    let guest_main = instance.get_typed_func::<(), ()>(&store, "main")?;

    // And finally we can call the wasm!
    guest_main.call(&mut store, ())?;
    println!("wasm exited");

    let mut host: Host<_> = InstanceWithContext::new(&mut store, instance).into();
    host.on_ble_adv_recv(&BLEAdvNotification {
        mac: [0x4d, 0x61, 0x72, 0x63, 0x79, 0x00],
        data: vec![
            0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64, 0x21,
        ],
    })
    .expect("failed to trigger callback");
    host.on_ble_adv_recv(&BLEAdvNotification {
        mac: [0x4d, 0x61, 0x72, 0x63, 0x79, 0x00],
        data: vec![
            0x21, 0x64, 0x6c, 0x72, 0x6f, 0x57, 0x20, 0x6f, 0x6c, 0x6c, 0x65, 0x48,
        ],
    })
    .expect("failed to trigger callback");

    Ok(())
}
