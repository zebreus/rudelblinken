use anyhow::Result;
use esp32_nimble::{
    enums::{ConnMode, DiscMode},
    utilities::BleUuid,
    uuid128, BLEAdvertisementData, BLEDevice, NimbleProperties,
};
use esp_idf_hal::delay::FreeRtos;
use esp_idf_sys as _;
use wasmi::{Caller, Engine, Func, Linker, Module, Store};

const UPDATE_SERVICE_UUID: BleUuid = BleUuid::from_uuid16(29342);
const UPDATE_SERVICE_RECEIVE_DATA_UUID: BleUuid = BleUuid::from_uuid16(13443);

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let ble_device = BLEDevice::take();
    let ble_advertising = ble_device.get_advertising();
    ble_advertising.lock().reset();

    let server = ble_device.get_server();
    server.on_connect(|server, desc| {
        ::log::info!("Client connected: {:?}", desc);

        server
            .update_conn_params(desc.conn_handle(), 24, 48, 0, 60)
            .unwrap();

        if server.connected_count() < (esp_idf_svc::sys::CONFIG_BT_NIMBLE_MAX_CONNECTIONS as _) {
            ::log::info!("Multi-connect support: start advertising");
            ble_advertising.lock().start().unwrap();
        }
    });

    server.on_disconnect(|desc, idk| {
        ::log::info!("Client disconnected: {:?}", desc);
    });

    let update_service = server.create_service(UPDATE_SERVICE_UUID);
    let receive_data_characteristic = update_service.lock().create_characteristic(
        UPDATE_SERVICE_RECEIVE_DATA_UUID,
        NimbleProperties::READ | NimbleProperties::WRITE,
    );
    receive_data_characteristic
        .lock()
        .on_read(move |_, _| {
            ::log::info!("Read from writable characteristic.");
        })
        .on_write(|args| {
            ::log::info!(
                "Wrote to writable characteristic: {:?} -> {:?}",
                args.current_data(),
                args.recv_data()
            );
        });

    // Configure Advertiser with Specified Data
    ble_advertising
        .lock()
        .set_data(
            BLEAdvertisementData::new()
                .name("ESP32-GATT-Server")
                // .add_service_uuid(uuid128!("fafafafa-fafa-fafa-fafa-fafafafafafa"))
                .add_service_uuid(UPDATE_SERVICE_UUID),
        )
        .advertisement_type(ConnMode::Und)
        .disc_mode(DiscMode::Gen)
        .scan_response(true)
        .start()
        .unwrap();

    server.ble_gatts_show_local();

    loop {
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000);
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
