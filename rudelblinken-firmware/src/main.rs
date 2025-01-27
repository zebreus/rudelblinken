#![feature(round_char_boundary)]
#![feature(once_cell_try)]

use cat_management_service::CatManagementService;
use esp32_nimble::{
    enums::{ConnMode, DiscMode, PowerLevel, PowerType},
    utilities::mutex::Mutex,
    BLEAdvertisementData, BLEDevice, BLEServer,
};
use esp_idf_hal::gpio::{self, PinDriver};
use esp_idf_sys::{self as _, heap_caps_print_heap_info, MALLOC_CAP_DEFAULT};
use file_upload_service::FileUploadService;
use nrf_logging_service::SerialLoggingService;
use std::{sync::LazyLock, time::Duration};
use storage::get_filesystem;

mod cat_management_service;
mod config;
mod file_upload_service;
mod nrf_logging_service;
pub mod service_helpers;
pub mod storage;
mod wasm_service;
// mod telid_logging_service;

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
    // Set PHY to 2M for all connections
    unsafe {
        esp_idf_sys::ble_gap_set_prefered_default_le_phy(
            esp_idf_sys::BLE_GAP_LE_PHY_2M_MASK as u8,
            esp_idf_sys::BLE_GAP_LE_PHY_2M_MASK as u8,
        );
        esp_idf_sys::ble_att_set_preferred_mtu(esp_idf_sys::BLE_ATT_MTU_MAX as u16);
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
        ::tracing::info!("Client connected: {:?}", desc);

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
        ::tracing::info!("Client disconnected: {:?}", desc);
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

pub static BLE_DEVICE: LazyLock<&'static mut BLEDevice> = LazyLock::new(|| BLEDevice::take());
pub static LED_PIN: LazyLock<Mutex<PinDriver<'static, gpio::Gpio8, gpio::Output>>> =
    LazyLock::new(|| {
        Mutex::new(PinDriver::output(unsafe { gpio::Gpio8::new() }).expect("pin init failed"))
    });

fn main() {
    // // Sleep a bit to allow the debugger to attach
    // unsafe {
    //     esp_idf_sys::sleep(4);
    // }

    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    fix_mac_address();

    let server = setup_ble_server();

    let _serial_logging_service = SerialLoggingService::new(server);

    get_filesystem().unwrap();
    print_memory_info();

    let _led_pin =
        Mutex::new(PinDriver::output(unsafe { gpio::Gpio8::new() }).expect("pin init failed"));

    let _file_upload_service = FileUploadService::new(server);
    LazyLock::force(&LED_PIN);

    let _cat_management_service = CatManagementService::new(server);

    // Starting advertising also starts the ble server. We cant add or change the services/attributes after the ble server started.
    {
        let ble_advertising = BLE_DEVICE.get_advertising();
        let mut data = BLEAdvertisementData::new();
        data.name("Rudelblinken")
            .add_service_uuid(FileUploadService::uuid())
            .manufacturer_data(&[0, 0]);
        ble_advertising.lock().set_data(&mut data).unwrap();
        ble_advertising
            .lock()
            .advertisement_type(ConnMode::Und)
            .disc_mode(DiscMode::Gen)
            .scan_response(false)
            .min_interval(100)
            .max_interval(250);
        ble_advertising.lock().start().unwrap();
    }

    loop {
        std::thread::sleep(Duration::from_secs(1));
    }

    // ble_device.get_server().on_connect(|_server, connection| {
    //     tracing::info!("Client connected, {:?}", connection);
    //     let ble_device = &BLE_DEVICE;
    //     let ble_advertising = ble_device.get_advertising();
    //     ble_advertising.lock().start().unwrap();
    // });
    // ble_device.get_server().on_disconnect(|connection, result| {
    //     tracing::info!("Client disconnected, {:?}", connection);
    //     tracing::info!("with result {:?}", result);
    //     let ble_device = &BLE_DEVICE;
    //     let ble_advertising = ble_device.get_advertising();
    //     ble_advertising.lock().start().unwrap();
    // });
}
