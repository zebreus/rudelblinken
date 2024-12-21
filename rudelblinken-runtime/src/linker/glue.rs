/// Provides functions that glue the relatively raw host functions to the implementation of Host
use super::{linker::WrappedCaller, MAJOR, MINOR, PATCH};
use crate::host::{
    AdvertisementSettings, AmbientLightType, Host, LedColor, LedInfo, LogLevel, SemanticVersion,
    VibrationSensorType,
};

/// `get-base-version: func() -> semantic-version;`
pub(super) fn get_base_version<T: Host>(
    mut _caller: WrappedCaller<'_, T>,
    version: &mut SemanticVersion,
) -> Result<(), wasmi::Error> {
    *version = SemanticVersion::new(MAJOR, MINOR, PATCH);
    return Ok(());
}
/// `yield-now: func();`
pub(super) fn yield_now<T: Host>(
    mut caller: WrappedCaller<'_, T>,
    micros: u64,
) -> Result<u32, wasmi::Error> {
    return T::yield_now(&mut caller, micros);
}
/// `sleep: func(micros: u64);`
pub(super) fn sleep<T: Host>(
    mut caller: WrappedCaller<'_, T>,
    micros: u64,
) -> Result<(), wasmi::Error> {
    return T::sleep(&mut caller, micros);
}
/// `time: func() -> u64;`
pub(super) fn time<T: Host>(mut caller: WrappedCaller<'_, T>) -> Result<u64, wasmi::Error> {
    return T::time(&mut caller);
}
/// `log: func(level: log-level, message: string)  -> ();`
pub(super) fn log<T: Host>(
    mut caller: WrappedCaller<'_, T>,
    level: LogLevel,
    message: &str,
) -> Result<(), wasmi::Error> {
    return T::log(&mut caller, level, message);
}
/// `get-name: func(name: &mut [u8; 16]);`
pub(super) fn get_name<T: Host>(
    mut caller: WrappedCaller<'_, T>,
    name: &mut [u8; 16],
) -> Result<(), wasmi::Error> {
    let host_name = T::get_name(&mut caller)?;
    let name_bytes = host_name.as_bytes();
    let name_length = std::cmp::min(name_bytes.len(), name.len());
    name[..name_length].copy_from_slice(&name_bytes[..name_length]);
    name[name_length..].fill(0);
    return Ok(());
}

/// `get-hardware-version: func() -> semantic-version;`
pub(super) fn get_hardware_version<T: Host>(
    mut _caller: WrappedCaller<'_, T>,
    version: &mut SemanticVersion,
) -> Result<(), wasmi::Error> {
    *version = SemanticVersion::new(MAJOR, MINOR, PATCH);
    return Ok(());
}
/// `set-leds: func(first-id: u16, lux: list<u16>) -> ();`
pub(super) fn set_leds<T: Host>(
    mut caller: WrappedCaller<'_, T>,
    _first_id: u16,
    leds: &[u16],
) -> Result<(), wasmi::Error> {
    return T::set_leds(&mut caller, leds);
}
/// `set-rgb: func(color: led-color, lux: u32) -> ();`
pub(super) fn set_rgb<T: Host>(
    mut caller: WrappedCaller<'_, T>,
    color: &LedColor,
    lux: u32,
) -> Result<(), wasmi::Error> {
    return T::set_rgb(&mut caller, color, lux);
}
/// `led-count: func() -> u32;`
pub(super) fn led_count<T: Host>(mut caller: WrappedCaller<'_, T>) -> Result<u16, wasmi::Error> {
    return T::led_count(&mut caller);
}
/// `get-led-info: func(id: u16) -> led-info;`
pub(super) fn get_led_info<T: Host>(
    mut caller: WrappedCaller<'_, T>,
    id: u16,
    info: &mut LedInfo,
) -> Result<(), wasmi::Error> {
    *info = T::get_led_info(&mut caller, id)?;
    return Ok(());
}
/// `get-ambient-light-type: func() -> ambient-light-type;`
pub(super) fn get_ambient_light_type<T: Host>(
    mut caller: WrappedCaller<'_, T>,
) -> Result<AmbientLightType, wasmi::Error> {
    match T::has_ambient_light(&mut caller)? {
        true => Ok(AmbientLightType::Basic),
        false => Ok(AmbientLightType::None),
    }
}
/// `get-ambient-light: func() -> u32;`
pub(super) fn get_ambient_light<T: Host>(
    mut caller: WrappedCaller<'_, T>,
) -> Result<u32, wasmi::Error> {
    return T::get_ambient_light(&mut caller);
}
/// `get-vibration-sensor-type: func() -> vibration-sensor-type;`
pub(super) fn get_vibration_sensor_type<T: Host>(
    mut caller: WrappedCaller<'_, T>,
) -> Result<VibrationSensorType, wasmi::Error> {
    match T::has_vibration_sensor(&mut caller)? {
        true => Ok(VibrationSensorType::Basic),
        false => Ok(VibrationSensorType::None),
    }
}
/// `get-vibration: func() -> u32;`
pub(super) fn get_vibration<T: Host>(
    mut caller: WrappedCaller<'_, T>,
) -> Result<u32, wasmi::Error> {
    return T::get_vibration(&mut caller);
}

/// `get-ble-version: func() -> semantic-version;`
pub(super) fn get_ble_version<T: Host>(
    mut _caller: WrappedCaller<'_, T>,
    version: &mut SemanticVersion,
) -> Result<(), wasmi::Error> {
    *version = SemanticVersion::new(MAJOR, MINOR, PATCH);
    return Ok(());
}

/// `configure-advertisement: func(settings: advertisement-settings) -> ();`
pub(super) fn configure_advertisement<T: Host>(
    mut caller: WrappedCaller<'_, T>,
    settings: AdvertisementSettings,
) -> Result<(), wasmi::Error> {
    return T::configure_advertisement(&mut caller, settings);
}

/// `set-advertisement-data: func(data: advertisement-data) -> ();`
pub(super) fn set_advertisement_data<T: Host>(
    mut caller: WrappedCaller<'_, T>,
    data: &[u8],
) -> Result<(), wasmi::Error> {
    return T::set_advertisement_data(&mut caller, data);
}
