use cat_management_service::{CatManagementService, WasmHostMessage};
use esp32_nimble::{
    enums::{ConnMode, DiscMode, PowerLevel, PowerType},
    utilities::mutex::Mutex,
    BLEAdvertisementData, BLEDevice, BLEScan, BLEServer,
};
use esp_idf_hal::{
    ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver},
    prelude::Peripherals,
    task,
    units::FromValueType,
};
use esp_idf_sys as _;
use file_upload_service::FileUploadService;
use rudelblinken_sdk::common::BLEAdvNotification;

mod cat_management_service;
mod file_upload_service;

/// Changes the OUI of the base mac address to 24:ec:4b which is not assigned
///
/// We can find our devices based on this OUI
fn fix_mac_address() {
    unsafe {
        let mut mac = [0u8; 6];
        esp_idf_sys::esp_base_mac_addr_get(mac.as_mut_ptr());
        if matches!(mac, [0x24, 0xec, 0x4b, ..]) {
            return;
        }
        let new_mac = [0x24, 0xec, 0x4b, mac[3], mac[4], mac[5]];
        esp_idf_sys::esp_iface_mac_addr_set(
            new_mac.as_ptr(),
            esp_idf_sys::esp_mac_type_t_ESP_MAC_BASE,
        );
    };
}

fn setup_ble_server() -> &'static mut BLEServer {
    let ble_device = BLEDevice::take();
    BLEDevice::take();
    // Set PHY to 2M for all connections
    unsafe {
        esp_idf_sys::ble_gap_set_prefered_default_le_phy(
            esp_idf_sys::BLE_GAP_LE_PHY_2M_MASK as u8,
            esp_idf_sys::BLE_GAP_LE_PHY_2M_MASK as u8,
        );
    }
    ble_device
        .set_preferred_mtu(esp_idf_sys::BLE_ATT_MTU_MAX as u16)
        .unwrap();
    ble_device
        .set_power(PowerType::Default, PowerLevel::P9)
        .unwrap();
    ble_device
        .set_power(PowerType::Advertising, PowerLevel::P9)
        .unwrap();
    ble_device
        .set_power(PowerType::Scan, PowerLevel::P9)
        .unwrap();

    let server = ble_device.get_server();
    server.on_connect(|server, desc| {
        ::log::info!("Client connected: {:?}", desc);

        // Black magic
        //
        // https://github.com/espressif/esp-idf/issues/12789
        server
            .update_conn_params(desc.conn_handle(), 6, 6, 0, 10)
            .unwrap();
        unsafe {
            esp_idf_sys::ble_gap_set_data_len(desc.conn_handle(), 0x00FB, 0x0148);
            // Set PHY to 2M for this connection
            esp_idf_sys::ble_gap_set_prefered_le_phy(
                desc.conn_handle(),
                esp_idf_sys::BLE_GAP_LE_PHY_2M_MASK as u8,
                esp_idf_sys::BLE_GAP_LE_PHY_2M_MASK as u8,
                esp_idf_sys::BLE_GAP_LE_PHY_CODED_ANY as u16, // We are not coding, so this does not matter,
            );
        }
        // if server.connected_count() < (esp_idf_svc::sys::CONFIG_BT_NIMBLE_MAX_CONNECTIONS as _) {
        //     ::log::info!("Multi-connect support: start advertising");
        //     ble_advertising.lock().start().unwrap();
        // }
    });

    server.on_disconnect(|desc, _| {
        ::log::info!("Client disconnected: {:?}", desc);
    });

    server.ble_gatts_show_local();

    server
}

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    fix_mac_address();

    setup_ble_server();

    let peripherals = Peripherals::take().unwrap();
    let timer_driver = LedcTimerDriver::new(
        peripherals.ledc.timer0,
        &TimerConfig::default().frequency(25.kHz().into()),
    )
    .unwrap();
    let led_driver = Mutex::new(
        LedcDriver::new(
            peripherals.ledc.channel0,
            timer_driver,
            peripherals.pins.gpio8,
        )
        .unwrap(),
    );

    let ble_device = BLEDevice::take();
    let ble_server = ble_device.get_server();
    let ble_advertising = ble_device.get_advertising();

    let file_upload_service = FileUploadService::new(ble_server);
    let cat_management_service = CatManagementService::new(
        ble_server,
        file_upload_service.clone(),
        ble_advertising,
        led_driver,
    );

    ble_advertising
        .lock()
        .set_data(
            BLEAdvertisementData::new()
                .name("Rudelblinken")
                // .add_service_uuid(uuid128!("fafafafa-fafa-fafa-fafa-fafafafafafa"))
                .add_service_uuid(FileUploadService::uuid())
                .manufacturer_data(&[0, 0]),
        )
        .unwrap();
    // Configure Advertiser with Specified Data
    ble_advertising
        .lock()
        .advertisement_type(ConnMode::Und)
        .disc_mode(DiscMode::Gen)
        .scan_response(true)
        .min_interval(100)
        .max_interval(250)
        .start()
        .unwrap();

    let mut ble_scan = BLEScan::new();
    ble_scan.active_scan(false).interval(100).window(99);

    /* let mut pin = PinDriver::output(unsafe { gpio::Gpio8::new() }).expect("pin init failed");

    let) mut c = 0i32;

    let sync_data = |c: u8| {
        ble_advertising
            .lock()
            .set_data(
                BLEAdvertisementData::new()
                    .name("Rudelblinken")
                    .add_service_uuid(FileUploadService::uuid())
                    .manufacturer_data(&[0x00, 0x00, 0xca, 0x7e, 0xa2, c]),
            )
            .expect("failed to update adv data");
    };
    let rem = 0.0; */
    loop {
        task::block_on(async {
            ble_scan
                .start(ble_device, 1000, |dev, data| {
                    if let Some(md) = data.manufacture_data() {
                        cat_management_service
                            .lock()
                            .wasm_runner
                            .send(WasmHostMessage::BLEAdvRecv(BLEAdvNotification {
                                mac: dev.addr().as_be_bytes(),
                                data: md.payload.into(),
                            }))
                            .expect("failed to send ble adv callback");
                    }
                    /* if let Some(md) = data.manufacture_data() {
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
                    } */
                    None::<()>
                })
                .await
                .expect("scan failed");
        });
        /* sync_data(c as u8);

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
        } */
    }
}
