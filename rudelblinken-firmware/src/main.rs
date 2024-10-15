use anyhow::Result;
use esp32_nimble::{
    enums::{ConnMode, DiscMode},
    utilities::BleUuid,
    uuid128, BLEAdvertisementData, BLEDevice, BLEScan, NimbleProperties,
};
use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{self, PinDriver},
    task,
};
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

    ble_advertising
        .lock()
        .set_data(
            BLEAdvertisementData::new()
                .name("ESP32-Pulse-Server")
                // .add_service_uuid(uuid128!("fafafafa-fafa-fafa-fafa-fafafafafafa"))
                // .add_service_uuid(UPDATE_SERVICE_UUID)
                .manufacturer_data(&[0, 0]),
        )
        .unwrap();
    // Configure Advertiser with Specified Data
    ble_advertising
        .lock()
        .advertisement_type(ConnMode::Und)
        .disc_mode(DiscMode::Gen)
        .scan_response(true)
        .min_interval(1000)
        .max_interval(2500)
        .start()
        .unwrap();

    server.ble_gatts_show_local();

    let mut pin = PinDriver::output(unsafe { gpio::Gpio8::new() }).expect("pin init failed");

    let mut ble_scan = BLEScan::new();
    ble_scan.active_scan(false).interval(100).window(99);

    let mut c = 0i32;

    let sync_data = |c: u8| {
        ble_advertising
            .lock()
            .set_data(
                BLEAdvertisementData::new()
                    .name("ESP32-Pulse-Server")
                    .manufacturer_data(&[0x00, 0x00, 0xca, 0x7e, 0xa2, c]),
            )
            .expect("failed to update adv data");
    };
    let mut rem = 0.0;
    loop {
        let mut offset_sum = 0i32;
        let mut offset_num = 0u32;
        task::block_on(async {
            ble_scan
                .start(ble_device, 10, |device, data| {
                    if let Some(md) = data.manufacture_data() {
                        let md = md.payload;
                        if md.len() == 4 && md[0] == 0x0ca && md[1] == 0x7e && md[2] == 0xa2 {
                            offset_num += 1;
                            let mut delta = md[3] as i32 - c as i32;
                            if delta < -128 {
                                delta += 0x100;
                            } else if 128 < delta {
                                delta -= 0x100;
                            }
                            offset_sum += delta;
                            /* let mut delta = md[3] as f32 - c as f32;
                            delta *= 0.05;
                            delta += nudge;
                            nudge = delta - delta.floor();
                            let delta = delta.floor() as u32;
                            ::log::info!("nudging with delta = {}", delta);
                            c = (c + delta as u32) & 0xff; */
                        }
                    }
                    None::<()>
                })
                .await
                .expect("scan failed");
        });
        sync_data(c as u8);

        let nudge = if 0 < offset_num {
            let err = (offset_sum as f32 * 0.05 / (offset_num as f32)) + rem;
            let rem = err - err.floor();
            let v = err.floor() as i32;
            ::log::info!("nudging with nudge = {} ({}, {})", v, offset_num, rem);
            v
        } else {
            0
        };
        c += 2 + nudge;
        while c < 0 {
            c += 256;
        }
        c &= 0xff;
        if 192 < c {
            pin.set_high().expect("set_high failed");
        } else {
            pin.set_low().expect("set_low failed");
        }
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
