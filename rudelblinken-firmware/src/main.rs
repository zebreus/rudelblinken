use std::sync::Arc;

use cat_management_service::{CatManagementService, WasmHostMessage};
use esp32_nimble::{
    enums::{ConnMode, DiscMode, PowerLevel, PowerType},
    utilities::mutex::Mutex,
    BLEAdvertisementData, BLEDevice, BLEScan, BLEServer,
};
use esp_idf_hal::{
    gpio::{self, PinDriver},
    ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver},
    prelude::Peripherals,
    task,
    units::FromValueType,
};
use esp_idf_sys::{self as _, heap_caps_print_heap_info, MALLOC_CAP_DEFAULT};
use file_upload_service::FileUploadService;
use rudelblinken_sdk::common::BLEAdvNotification;
use storage::setup_storage;

mod cat_management_service;
mod file_upload_service;
pub mod storage;

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

/// You need to set the following options in sdkconfig to use this function
///
/// CONFIG_FREERTOS_USE_TRACE_FACILITY=y
/// CONFIG_FREERTOS_USE_STATS_FORMATTING_FUNCTIONS=y
pub fn print_memory_info() {
    unsafe {
        // let mut stats_buffer = [0u8; 1024];
        // vTaskList(stats_buffer.as_mut_ptr() as *mut i8);
        // let slice = String::from_utf8_lossy(&stats_buffer);
        // println!("Tasks:\n{}", slice);

        println!("");
        heap_caps_print_heap_info(MALLOC_CAP_DEFAULT);

        println!("");
        println!(
            "Free {} of {} ({}%)",
            esp_idf_sys::heap_caps_get_free_size(esp_idf_sys::MALLOC_CAP_DEFAULT),
            esp_idf_sys::heap_caps_get_total_size(esp_idf_sys::MALLOC_CAP_DEFAULT),
            esp_idf_sys::heap_caps_get_free_size(esp_idf_sys::MALLOC_CAP_DEFAULT) as f32
                / esp_idf_sys::heap_caps_get_total_size(esp_idf_sys::MALLOC_CAP_DEFAULT) as f32,
        );
        println!(
            "Largest free block: {}",
            esp_idf_sys::heap_caps_get_largest_free_block(esp_idf_sys::MALLOC_CAP_DEFAULT),
        );
    }
}

fn main() {
    unsafe {
        esp_idf_sys::sleep(2);
    }
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    unsafe {
        esp_idf_sys::sleep(2);
    }

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    fix_mac_address();

    setup_ble_server();

    setup_storage();

    print_memory_info();
    // print_partitions();

    // let mut storage = FlashStorage::new().unwrap();
    // let mut storage2 = FlashStorage::new().unwrap();

    // let test_string = String::from("Hallo Wolt!");
    // let test_vec: Vec<u8> = test_string.clone().into();

    // // storage.write(&test_vec).unwrap_or_default();
    // // let new_vec = storage.read(test_vec.len());
    // // let new_vec2 = storage2.read(test_vec.len());

    // // let new_string = String::from_utf8_lossy(&new_vec);
    // // let new_string2 = String::from_utf8_lossy(&new_vec2);

    // // ::log::error!(target: "test-fs", "Print {} Pront {} Prant {}", test_string, new_string, new_string2);

    let peripherals = Peripherals::take().unwrap();
    let timer_driver = LedcTimerDriver::new(
        peripherals.ledc.timer0,
        &TimerConfig::default().frequency(25.kHz().into()),
    )
    .unwrap();
    /* let led_driver = Mutex::new(
        LedcDriver::new(
            peripherals.ledc.channel0,
            timer_driver,
            peripherals.pins.gpio8,
        )
        .unwrap(),
    );
    led_driver.lock().set_duty(0x1000).unwrap(); */
    let led_pin =
        Mutex::new(PinDriver::output(unsafe { gpio::Gpio8::new() }).expect("pin init failed"));

    let ble_device = BLEDevice::take();

    let file_upload_service = FileUploadService::new(ble_device.get_server());
    let cat_management_service =
        CatManagementService::new(ble_device, file_upload_service.clone(), led_pin);

    {
        let ble_advertising = ble_device.get_advertising();
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
    }

    let mut ble_scan = BLEScan::new();
    ble_scan.active_scan(false).interval(100).window(99);

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
                    None::<()>
                })
                .await
                .expect("scan failed");
        });
    }
}
