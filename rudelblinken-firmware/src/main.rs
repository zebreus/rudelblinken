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
use name::initialize_name;
use nrf_logging_service::SerialLoggingService;
use std::{sync::LazyLock, time::Duration};
use storage::get_filesystem;

mod cat_management_service;
mod config;
mod file_upload_service;
mod name;
mod nrf_logging_service;
pub mod service_helpers;
pub mod storage;
mod wasm_service;
// mod telid_logging_service;

fn get_bluetooth_mac_address() -> [u8; 6] {
    let mac = config::mac_address::get();
    if let Some(mac) = mac {
        return mac;
    }

    let mut bluetooth_mac = [0u8; 6];
    unsafe {
        esp_idf_sys::bootloader_random_enable();
        esp_idf_sys::esp_fill_random(bluetooth_mac.as_mut_ptr() as *mut core::ffi::c_void, 6);
        esp_idf_sys::bootloader_random_disable();

        // Mark as a BLE static address
        bluetooth_mac[0] = bluetooth_mac[0] | 0b11000000;
        // Mark as unicast
        bluetooth_mac[0] = bluetooth_mac[0] & 0b11111110;
        // Mark as locally generated
        bluetooth_mac[0] = bluetooth_mac[0] | 0b00000010;
    }
    config::mac_address::set(&Some(bluetooth_mac));
    bluetooth_mac
}

/// Make sure that we are using a random mac address
fn fix_mac_address() {
    let bluetooth_mac = get_bluetooth_mac_address();

    // The base mac is the bluetooth mac - 2
    // (at least i think so) for this reason we also set the bluetooth mac explicitly. We are not going to use the wifi mac anyways
    let mut base_mac = bluetooth_mac.clone();
    base_mac[5] = base_mac[5].wrapping_sub(2);

    unsafe {
        esp_idf_sys::esp_iface_mac_addr_set(
            base_mac.as_ptr(),
            esp_idf_sys::esp_mac_type_t_ESP_MAC_BASE,
        );
        esp_idf_sys::esp_iface_mac_addr_set(
            bluetooth_mac.as_ptr(),
            esp_idf_sys::esp_mac_type_t_ESP_MAC_BT,
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
    let device_name = initialize_name();

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
        let advertisment_name = "[rb]".to_string() + &device_name;
        let ble_advertising = BLE_DEVICE.get_advertising();
        let mut data = BLEAdvertisementData::new();
        data.name(advertisment_name.as_ref());
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
