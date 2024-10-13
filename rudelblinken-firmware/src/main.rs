use anyhow::Result;
use esp32_nimble::{
    enums::{ConnMode, DiscMode},
    BLEAdvertisementData, BLEDevice,
};
use esp_idf_hal::delay::FreeRtos;
use esp_idf_sys as _;
use wasmi::{Caller, Engine, Func, Linker, Module, Store};

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let ble_device = BLEDevice::take();
    let ble_advertiser = ble_device.get_advertising();

    let mut ad_data = BLEAdvertisementData::new();
    let val = match wasm_poc() {
        Ok(v) => v,
        Err(err) => {
            println!("wasm_poc err={:?}", err);
            0
        }
    };
    let name = format!("ESP32-C3 {}", val);
    println!("Using name {}", name);
    ad_data.name(&name);

    // Configure Advertiser with Specified Data
    ble_advertiser.lock().set_data(&mut ad_data).unwrap();

    ble_advertiser
        .lock()
        .advertisement_type(ConnMode::Non)
        .disc_mode(DiscMode::Gen)
        .scan_response(false);

    ble_advertiser.lock().start().unwrap();
    println!("Advertisement Started");
    loop {
        // Keep Advertising
        // Add delay to prevent watchdog from triggering
        FreeRtos::delay_ms(10);
    }
}

const WASM_MOD: &[u8] =
    include_bytes!("../../target/wasm32-unknown-unknown/release/rudelblinken_wasm.wasm");

fn wasm_poc() -> Result<u64> {
    let engine = Engine::default();

    let module = Module::new(&engine, WASM_MOD)?;

    type HostState = ();
    let mut store = Store::new(&engine, ());
    let host_ping = Func::wrap(&mut store, |_caller: Caller<'_, HostState>, param: i32| {
        println!("Got {param} from WebAssembly");
    });

    let mut linker = <Linker<HostState>>::new(&engine);

    linker.define("env", "ping", host_ping)?;
    let pre_instance = linker.instantiate(&mut store, &module)?;
    let instance = pre_instance.start(&mut store)?;
    let add = instance.get_typed_func::<(u64, u64), u64>(&store, "add")?;

    // And finally we can call the wasm!
    Ok(add.call(&mut store, (1, 3))?)
}
