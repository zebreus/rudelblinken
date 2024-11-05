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
use wasmi::{Caller, Config, Engine, Linker, Module, Store};

const WASM_MOD: &[u8] = include_bytes!(
    "../../rudelblinken-wasm/target/wasm32-unknown-unknown/release/rudelblinken_wasm.wasm"
);

struct HostState {
    name: String,
    pending_callbacks: Vec<Box<dyn Fn(&mut Host<Caller<'_, Self>>)>>,
}

impl HostBase for HostState {
    fn host_log(&mut self, log: common::Log) {
        match log.level {
            common::LogLevel::Error => tracing::error!(msg = &log.message, "guest logged"),
            common::LogLevel::Warn => tracing::warn!(msg = &log.message, "guest logged"),
            common::LogLevel::Info => tracing::info!(msg = &log.message, "guest logged"),
            common::LogLevel::Debug => tracing::debug!(msg = &log.message, "guest logged"),
            common::LogLevel::Trace => tracing::trace!(msg = &log.message, "guest logged"),
        }
    }

    fn get_name(&mut self) -> String {
        self.name.clone()
    }

    fn on_yield(host: &mut Host<Caller<'_, Self>>, timeout: u32) -> bool {
        if timeout != 0 {
            let s = host.state_mut();
            if s.pending_callbacks.is_empty() {
                return true;
            }
            let mut cbs = vec![];
            std::mem::swap(&mut s.pending_callbacks, &mut cbs);

            for cb in cbs {
                cb(host)
            }
        }
        true
    }

    fn get_time_millis(&mut self) -> u32 {
        todo!()
    }
}

impl LEDBrightness for HostState {
    fn set_led_brightness(&mut self, settings: common::LEDBrightnessSettings) {
        tracing::info!(?settings, "guest set led bightness")
    }
}

impl BLEAdv for HostState {
    fn configure_ble_adv(&mut self, settings: common::BLEAdvSettings) {
        tracing::info!(?settings, "guest configured ble_adv")
    }

    fn configure_ble_data(&mut self, data: common::BLEAdvData) {
        tracing::info!(?data, "guest set ble_adv data")
    }
}

fn main() -> Result<()> {
    // TODO (next steps):
    // - manual mutli-threading to handle callbacks etc
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
    // along the way do some sdk improvements:
    // - do not panic in the sdk, get rid of except/unwrap call that can
    //   conceivably fail
    // - make tracing-based logging inside the guest work, and log correctly on
    //   the host
    // - maybe use macros to generate all/some of the repetitive stuff
    // - just generally go through the TODOs and FIXMEs in the code

    let env_filter = EnvFilter::try_from_default_env();

    let stdout_env_filter = env_filter.unwrap_or_else(|_| EnvFilter::new("info"));
    let stdout_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(std::io::stdout)
        .with_filter(stdout_env_filter);

    tracing_subscriber::registry().with(stdout_layer).init();

    let engine = Engine::new(
        Config::default()
            .consume_fuel(true)
            .ignore_custom_sections(true),
    );
    let module = Module::new(&engine, WASM_MOD)?;

    let mut store = Store::new(
        &engine,
        HostState {
            name: "lgcl".to_owned(),
            pending_callbacks: vec![
                Box::new(|host| {
                    host.on_ble_adv_recv(&BLEAdvNotification {
                        mac: [0x4d, 0x61, 0x72, 0x63, 0x79, 0x00],
                        data: vec![
                            0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64, 0x21,
                        ],
                    })
                    .expect("failed to trigger callback");
                }),
                Box::new(|host| {
                    host.on_ble_adv_recv(&BLEAdvNotification {
                        mac: [0x4d, 0x61, 0x72, 0x63, 0x79, 0x00],
                        data: vec![
                            0x21, 0x64, 0x6c, 0x72, 0x6f, 0x57, 0x20, 0x6f, 0x6c, 0x6c, 0x65, 0x48,
                        ],
                    })
                    .expect("failed to trigger callback");
                }),
            ],
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

    let mut host: Host<_> = InstanceWithContext::new(&mut store, instance).into();

    host.main().expect("guest main failed");
    println!("wasm exited");

    Ok(())
}
