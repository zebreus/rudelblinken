use rudelblinken_sdk::{
    common::{
        BLEAdvData, BLEAdvNotification, BLEAdvSettings, LEDBrightnessSettings, Log, LogLevel,
    },
    guest::host,
};

#[no_mangle]
extern "C" fn main() {
    let name = host::get_name();
    host::host_log(&Log {
        level: LogLevel::Info,
        message: format!("name = {}", name).to_owned(),
    });
    let has_host_base = host::has_host_base();
    let has_ble_adv = host::has_ble_adv();
    let has_led_brightness = host::has_led_brightness();

    host::host_log(&Log {
        level: LogLevel::Info,
        message: format!(
            "has_host_base = {}; has_ble_adv = {}; has_led_brightness = {}",
            has_host_base, has_ble_adv, has_led_brightness
        )
        .to_owned(),
    });

    host::set_led_brightness(&LEDBrightnessSettings { rgb: [128, 0, 64] });

    host::configure_ble_adv(&BLEAdvSettings {
        min_interval: 1024,
        max_interval: 2048,
    });

    host::configure_ble_data(&BLEAdvData {
        data: vec![0, 1, 2, 3],
    });

    let mut last = None;

    host::set_on_ble_adv_recv_callback(Some(move |info: BLEAdvNotification| {
        host::host_log(&Log {
            level: LogLevel::Info,
            message: format!("callback_recv'ed: {:?}, last: {:?}", info, last).to_owned(),
        });
        last.replace(info.data.clone());
    }));

    loop {
        host::rt_yield();
    }
}
