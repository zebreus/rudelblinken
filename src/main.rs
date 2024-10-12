use esp32_nimble::{
    enums::{ConnMode, DiscMode},
    BLEAdvertisementData, BLEDevice,
};
use esp_idf_hal::delay::FreeRtos;
use esp_idf_sys as _;

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let ble_device = BLEDevice::take();
    let ble_advertiser = ble_device.get_advertising();

    let mut ad_data = BLEAdvertisementData::new();
    ad_data.name("ESP32-C3 Peripheral");

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
