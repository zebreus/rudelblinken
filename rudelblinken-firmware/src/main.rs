use std::sync::Arc;

use esp32_nimble::{
    enums::{ConnMode, DiscMode, PowerLevel, PowerType},
    utilities::mutex::Mutex,
    BLEAdvertisementData, BLEDevice, BLEScan, BLEServer,
};
// use esp_idf_hal::timer::TimerConfig;
use boot_config_service::BootConfigService;
use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{self, PinDriver},
    ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver},
    peripheral::Peripheral,
    prelude::Peripherals,
    task,
    units::FromValueType,
};
use esp_idf_sys::{self as _, heap_caps_print_heap_info, MALLOC_CAP_DEFAULT};
use file_upload_service::FileUploadService;
use serial_logging_service::SerialLoggingService;
use storage::setup_storage;

mod boot_config_service;
mod file_upload_service;
mod serial_logging_service;
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

fn main() {
    // unsafe {
    //     esp_idf_sys::sleep(2);
    // }
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();
    // Bind the log crate to the ESP Logging facilities
    // esp_idf_svc::log::EspLogger::initialize_default();
    // tracing_subscriber::fmt()
    //     .with_max_level(tracing::Level::INFO)
    //     .with_writer(std::io::stdout)
    //     .init();

    fix_mac_address();

    setup_ble_server();

    let ble_device = BLEDevice::take();
    let serial_logging_service = SerialLoggingService::new(ble_device.get_server());
    let boot_config_service = BootConfigService::new(ble_device.get_server());

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

    /* let peripherals = Peripherals::take().unwrap();
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
    led_driver.lock().set_duty(0x1000).unwrap(); */
    // let red_pin =
    //     Mutex::new(PinDriver::output(unsafe { gpio::Gpio8::new() }).expect("pin init failed"));
    // let blue_pin =
    //     Mutex::new(PinDriver::output(unsafe { gpio::Gpio6::new() }).expect("pin init failed"));

    let file_upload_service = FileUploadService::new(ble_device.get_server());

    {
        let ble_advertising = ble_device.get_advertising();
        ble_advertising
            .lock()
            .set_data(
                BLEAdvertisementData::new()
                    .name("Blinkenboots")
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

    // red_pin.lock().set_high().unwrap();
    // blue_pin.lock().set_high().unwrap();

    let mut peripherals = Peripherals::take().unwrap();
    let BootConfigService {
        red_pin,
        blue_pin,
        switch_colors,
        ..
    } = boot_config_service.lock().clone();

    let mut channel_a = LedcDriver::new(
        unsafe { peripherals.ledc.channel0.clone_unchecked() },
        LedcTimerDriver::new(
            unsafe { peripherals.ledc.timer0.clone_unchecked() },
            &TimerConfig::new().frequency(25.kHz().into()),
        )
        .unwrap(),
        unsafe { peripherals.pins.gpio6.clone_unchecked() },
    )
    .unwrap();

    let mut channel_b = LedcDriver::new(
        peripherals.ledc.channel1,
        LedcTimerDriver::new(
            unsafe { peripherals.ledc.timer0.clone_unchecked() },
            &TimerConfig::new().frequency(25.kHz().into()),
        )
        .unwrap(),
        unsafe { peripherals.pins.gpio8.clone_unchecked() },
    )
    .unwrap();

    let (red, blue) = match switch_colors {
        true => (&mut channel_a, &mut channel_b),
        false => (&mut channel_b, &mut channel_a),
    };
    match red_pin {
        0 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio0.clone_unchecked() })
            .unwrap(),
        1 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio1.clone_unchecked() })
            .unwrap(),
        2 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio2.clone_unchecked() })
            .unwrap(),
        3 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio3.clone_unchecked() })
            .unwrap(),
        4 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio4.clone_unchecked() })
            .unwrap(),
        5 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio5.clone_unchecked() })
            .unwrap(),
        6 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio6.clone_unchecked() })
            .unwrap(),
        7 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio7.clone_unchecked() })
            .unwrap(),
        8 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio8.clone_unchecked() })
            .unwrap(),
        9 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio9.clone_unchecked() })
            .unwrap(),
        10 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio10.clone_unchecked() })
            .unwrap(),
        11 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio11.clone_unchecked() })
            .unwrap(),
        12 => channel_a
            .config_with_pin(unsafe { peripherals.pins.gpio12.clone_unchecked() })
            .unwrap(),
        _ => panic!("Invalid red pin"),
    };
    match blue_pin {
        0 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio0.clone_unchecked() })
            .unwrap(),
        1 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio1.clone_unchecked() })
            .unwrap(),
        2 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio2.clone_unchecked() })
            .unwrap(),
        3 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio3.clone_unchecked() })
            .unwrap(),
        4 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio4.clone_unchecked() })
            .unwrap(),
        5 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio5.clone_unchecked() })
            .unwrap(),
        6 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio6.clone_unchecked() })
            .unwrap(),
        7 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio7.clone_unchecked() })
            .unwrap(),
        8 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio8.clone_unchecked() })
            .unwrap(),
        9 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio9.clone_unchecked() })
            .unwrap(),
        10 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio10.clone_unchecked() })
            .unwrap(),
        11 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio11.clone_unchecked() })
            .unwrap(),
        12 => channel_b
            .config_with_pin(unsafe { peripherals.pins.gpio12.clone_unchecked() })
            .unwrap(),
        _ => panic!("Invalid blue pin"),
    }

    let max_duty = channel_a.get_max_duty();
    // for numerator in [0, 1, 2, 3, 4, 5].iter().cycle() {
    //     println!("Duty {numerator}/5");
    //     channel_a.set_duty(max_duty * numerator / 5).unwrap();
    //     channel_b.set_duty(max_duty * (5 - numerator) / 5).unwrap();
    //     FreeRtos::delay_ms(50);
    // }
    let mut time: u64 = 0;
    loop {
        let BootConfigService {
            mode,
            speed,
            switch_colors,
            blue_brightness,
            red_brightness,
            red_pin,
            blue_pin,
        } = boot_config_service.lock().clone();
        let (red, blue) = match switch_colors {
            true => (&mut channel_a, &mut channel_b),
            false => (&mut channel_b, &mut channel_a),
        };

        match mode {
            0 => {
                let numerator = (time % 6) as u32;
                red.set_duty(max_duty * numerator / 5 * red_brightness as u32 / 255)
                    .unwrap();
                blue.set_duty(max_duty * numerator / 5 * blue_brightness as u32 / 255)
                    .unwrap();
            }
            1 => {
                let numerator = (time % (6)) as u32;
                let state = numerator;
                red.set_duty(0).unwrap();
                match state {
                    0 | 2 => {
                        blue.set_duty(max_duty * blue_brightness as u32 / 255)
                            .unwrap();
                    }
                    _ => {
                        blue.set_duty(0).unwrap();
                    }
                }
            }
            2 => {
                let numerator = (time % (10)) as u32;
                let state = numerator;
                red.set_duty(0).unwrap();
                match state {
                    5 | 7 => {
                        red.set_duty(max_duty * blue_brightness as u32 / 255)
                            .unwrap();
                    }
                    _ => {
                        red.set_duty(0).unwrap();
                    }
                }
                match state {
                    0 | 2 => {
                        blue.set_duty(max_duty * blue_brightness as u32 / 255)
                            .unwrap();
                    }
                    _ => {
                        blue.set_duty(0).unwrap();
                    }
                }
            }
            _ => {
                red.set_duty(max_duty * red_brightness as u32 / 255)
                    .unwrap();
                blue.set_duty(max_duty * blue_brightness as u32 / 255)
                    .unwrap();
            }
        }
        time += 1;
        FreeRtos::delay_ms(speed as u32);
    }

    // let mut ble_scan = BLEScan::new();
    // ble_scan.active_scan(false).interval(100).window(99);

    // loop {
    //     task::block_on(async {
    //         ble_scan
    //             .start(ble_device, 1000, |dev, data| {
    //                 if let Some(md) = data.manufacture_data() {
    //                     cat_management_service
    //                         .lock()
    //                         .wasm_runner
    //                         .send(WasmHostMessage::BLEAdvRecv(BLEAdvNotification {
    //                             mac: dev.addr().as_be_bytes(),
    //                             data: md.payload.into(),
    //                         }))
    //                         .expect("failed to send ble adv callback");
    //                 }
    //                 None::<()>
    //             })
    //             .await
    //             .expect("scan failed");
    //     });
    // }
}
