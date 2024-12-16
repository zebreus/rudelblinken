/// Provides functions that glue the relatively raw host functions to the implementation of Host
use super::{MAJOR, MINOR, PATCH};
use crate::host::{
    AmbientLightType, Host, LedColor, LedInfo, LogLevel, SemanticVersion, VibrationSensorType,
};
use wasmi::Caller;

/// `get-base-version: func() -> semantic-version;`
pub(super) fn get_base_version<T: Host>(
    mut _caller: Caller<'_, T>,
    version: &mut SemanticVersion,
) -> Result<(), wasmi::Error> {
    *version = SemanticVersion::new(MAJOR, MINOR, PATCH);
    return Ok(());
}
/// `yield-now: func();`
pub(super) fn yield_now<T: Host>(mut caller: Caller<'_, T>) -> Result<(), wasmi::Error> {
    return caller.data_mut().yield_now();
}
/// `sleep: func(micros: u64);`
pub(super) fn sleep<T: Host>(mut caller: Caller<'_, T>, micros: u64) -> Result<(), wasmi::Error> {
    return caller.data_mut().sleep(micros);
}
/// `time: func() -> u64;`
pub(super) fn time<T: Host>(mut caller: Caller<'_, T>) -> Result<u64, wasmi::Error> {
    return caller.data_mut().time();
}
/// `log: func(level: log-level, message: string)  -> ();`
pub(super) fn log<T: Host>(
    mut caller: Caller<'_, T>,
    level: LogLevel,
    message: &str,
) -> Result<(), wasmi::Error> {
    return caller.data_mut().log(level, message);
}
/// `get-name: func(name: &mut [u8; 16]);`
pub(super) fn get_name<T: Host>(
    mut caller: Caller<'_, T>,
    name: &mut [u8; 16],
) -> Result<(), wasmi::Error> {
    let host_name = caller.data_mut().get_name()?;
    let name_bytes = host_name.as_bytes();
    let name_length = std::cmp::min(name_bytes.len(), name.len());
    name[..name_length].copy_from_slice(&name_bytes[..name_length]);
    name[name_length..].fill(0);
    return Ok(());
}

/// `get-hardware-version: func() -> semantic-version;`
pub(super) fn get_hardware_version<T: Host>(
    mut _caller: Caller<'_, T>,
    version: &mut SemanticVersion,
) -> Result<(), wasmi::Error> {
    *version = SemanticVersion::new(MAJOR, MINOR, PATCH);
    return Ok(());
}
/// `set-leds: func(first-id: u16, lux: list<u16>) -> ();`
pub(super) fn set_leds<T: Host>(
    mut caller: Caller<'_, T>,
    leds: &[u16],
) -> Result<(), wasmi::Error> {
    return caller.data_mut().set_leds(leds);
}
/// `set-rgb: func(color: led-color, lux: u32) -> ();`
pub(super) fn set_rgb<T: Host>(
    mut caller: Caller<'_, T>,
    color: &LedColor,
    lux: u32,
) -> Result<(), wasmi::Error> {
    return caller.data_mut().set_rgb(color, lux);
}
/// `led-count: func() -> u32;`
pub(super) fn led_count<T: Host>(mut caller: Caller<'_, T>) -> Result<u16, wasmi::Error> {
    return caller.data_mut().led_count();
}
/// `get-led-info: func(id: u16) -> led-info;`
pub(super) fn get_led_info<T: Host>(
    mut caller: Caller<'_, T>,
    id: u16,
    info: &mut LedInfo,
) -> Result<(), wasmi::Error> {
    *info = caller.data_mut().get_led_info(id)?;
    return Ok(());
}
/// `get-ambient-light-type: func() -> ambient-light-type;`
pub(super) fn get_ambient_light_type<T: Host>(
    mut caller: Caller<'_, T>,
) -> Result<AmbientLightType, wasmi::Error> {
    match caller.data_mut().has_ambient_light()? {
        true => Ok(AmbientLightType::Basic),
        false => Ok(AmbientLightType::None),
    }
}
/// `get-ambient-light: func() -> u32;`
pub(super) fn get_ambient_light<T: Host>(mut caller: Caller<'_, T>) -> Result<u32, wasmi::Error> {
    return caller.data_mut().get_ambient_light();
}
/// `get-vibration-sensor-type: func() -> vibration-sensor-type;`
pub(super) fn get_vibration_sensor_type<T: Host>(
    mut caller: Caller<'_, T>,
) -> Result<VibrationSensorType, wasmi::Error> {
    match caller.data_mut().has_vibration_sensor()? {
        true => Ok(VibrationSensorType::Basic),
        false => Ok(VibrationSensorType::None),
    }
}
/// `get-vibration: func() -> u32;`
pub(super) fn get_vibration<T: Host>(mut caller: Caller<'_, T>) -> Result<u32, wasmi::Error> {
    return caller.data_mut().get_vibration();
}
