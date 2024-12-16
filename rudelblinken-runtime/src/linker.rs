use crate::host::{Host, LedColor, LedInfo, LogLevel, SemanticVersion};
use wasmi::{Caller, Config, Engine, Extern, Func, Instance, Linker, Memory, Module, Store};

const MAJOR: u8 = 0;
const MINOR: u8 = 0;
const PATCH: u8 = 1;

fn get_memory<'a, T: Host>(caller: &Caller<'a, T>) -> Result<Memory, wasmi::Error> {
    match caller.get_export("memory") {
        Some(wasmi::Extern::Memory(mem)) => Ok(mem),
        _ => Err(wasmi::Error::new(
            "memory not found. Does the guest export 'memory'?",
        )),
    }
}

fn get_slice<T: Host>(
    memory: &Memory,
    caller: &Caller<'_, T>,
    offset: i32,
    length: i32,
) -> Result<&'static [u8], wasmi::Error> {
    let slice = memory
        .data(caller)
        .get(offset as u32 as usize..)
        .ok_or(wasmi::Error::new("pointer out of bounds"))?
        .get(..length as u32 as usize)
        .ok_or(wasmi::Error::new("length out of bounds"))?;

    let static_slice = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(slice) };

    return Ok(static_slice);
}

fn get_mut_slice<T: Host>(
    memory: &Memory,
    caller: &mut Caller<'_, T>,
    offset: i32,
    length: i32,
) -> Result<&'static mut [u8], wasmi::Error> {
    let slice = memory
        .data_mut(caller)
        .get_mut(offset as u32 as usize..)
        .ok_or(wasmi::Error::new("pointer out of bounds"))?
        .get_mut(..length as u32 as usize)
        .ok_or(wasmi::Error::new("length out of bounds"))?;

    let static_slice = unsafe { std::mem::transmute::<&mut [u8], &'static mut [u8]>(slice) };

    return Ok(static_slice);
}

fn get_mut_array<T: Host, const L: usize>(
    memory: &Memory,
    caller: &mut Caller<'_, T>,
    offset: i32,
) -> Result<&'static mut [u8; L], wasmi::Error> {
    let data = memory
        .data_mut(caller)
        .get_mut(offset as u32 as usize..)
        .ok_or(wasmi::Error::new("pointer out of bounds"))?
        .get_mut(..L)
        .ok_or(wasmi::Error::new("length out of bounds"))?;

    let data_array: &mut [u8; L] = unsafe { data.try_into().unwrap_unchecked() };

    let static_result =
        unsafe { std::mem::transmute::<&mut [u8; L], &'static mut [u8; L]>(data_array) };
    return Ok(static_result);
}

pub struct LinkedHost<T: Host> {
    instance: Instance,
    store: Store<T>,
}

impl<T: Host> LinkedHost<T> {
    fn new(instance: Instance, store: Store<T>) -> Self {
        return LinkedHost { instance, store };
    }
    pub fn run(&mut self) -> Result<(), wasmi::Error> {
        let run = self
            .instance
            .get_typed_func::<(), ()>(&self.store, "rudel:base/run@0.0.1#run")?;
        run.call(&mut self.store, ())?;
        return Ok(());
    }
}

pub fn setup<T: Host>(wasm: &[u8], host: T) -> Result<LinkedHost<T>, wasmi::Error> {
    let engine = Engine::new(
        Config::default()
            .consume_fuel(true)
            .ignore_custom_sections(true),
    );
    let module = Module::new(&engine, wasm)?;

    let mut store = Store::new(&engine, host);
    store.set_fuel(99999).unwrap();

    let mut linker = <Linker<T>>::new(&engine);

    setup_linker(&mut linker, &mut store)?;

    let instance = linker.instantiate(&mut store, &module)?.start(&mut store)?;

    let linked_instance = LinkedHost::new(instance, store);
    return Ok(linked_instance);
}

/// Link the host functions provided by T.
///
/// This functions will provide the rudel-host functions to the linker by generating glue code for the functionality provided by the host implementation T
pub fn link_function<T: Host>(
    linker: &mut Linker<T>,
    module: &str,
    function: &str,
    implementation: impl Into<Extern>,
) -> Result<(), wasmi::Error> {
    linker.define(&format!("{}@0.0.1", module), function, implementation)?;
    return Ok(());
}

/// Link the host functions provided by T.
///
/// This functions will provide the rudel-host functions to the linker by generating glue code for the functionality provided by the host implementation T
fn link_base<T: Host>(
    linker: &mut Linker<T>,
    mut store: &mut Store<T>,
) -> Result<(), wasmi::Error> {
    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("get-hardware-version")))
    // extern void __wasm_import_rudel_base_hardware_get_hardware_version(uint8_t *);
    link_function(
        linker,
        "rudel:base/base",
        "get-base-version",
        Func::wrap(
            &mut store,
            |mut caller: Caller<'_, T>, offset: i32| -> Result<(), wasmi::Error> {
                let memory = get_memory(&caller)?;
                let slice = get_mut_slice(&memory, &mut caller, offset, 4)?;
                // SAFETY: Should be safe because the layout should match
                let version = unsafe {
                    std::mem::transmute::<*mut u8, *mut SemanticVersion>(slice.as_mut_ptr())
                };
                let version_ref = unsafe { &mut *version };
                glue::get_base_version(caller, version_ref)?;

                return Ok(());
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/base@0.0.1"), __import_name__("yield-now")))
    // extern void __wasm_import_rudel_base_base_yield_now(void);
    link_function(
        linker,
        "rudel:base/base",
        "yield-now",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>| -> Result<(), wasmi::Error> {
                return glue::yield_now(caller);
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/base@0.0.1"), __import_name__("sleep")))
    // extern void __wasm_import_rudel_base_base_sleep(int64_t);
    link_function(
        linker,
        "rudel:base/base",
        "sleep",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>, micros: u64| -> Result<(), wasmi::Error> {
                return glue::sleep(caller, micros);
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/base@0.0.1"), __import_name__("time")))
    // extern int64_t __wasm_import_rudel_base_base_time(void);
    link_function(
        linker,
        "rudel:base/base",
        "time",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>| -> Result<u64, wasmi::Error> {
                return glue::time(caller);
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/base@0.0.1"), __import_name__("log")))
    // extern void __wasm_import_rudel_base_base_log(int32_t, uint8_t *, size_t);
    link_function(
        linker,
        "rudel:base/base",
        "log",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>,
             level: i32,
             message_offset: i32,
             message_length: i32|
             -> Result<(), wasmi::Error> {
                let log_level = LogLevel::lift(level);

                let memory = get_memory(&caller)?;
                let data = get_slice(&memory, &caller, message_offset, message_length)?;
                let message = match std::str::from_utf8(data) {
                    Ok(s) => s,
                    Err(_) => return Err(wasmi::Error::new("invalid utf-8")),
                };
                return glue::log(caller, log_level, message);
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/base@0.0.1"), __import_name__("get-name")))
    // extern void __wasm_import_rudel_base_base_get_name(uint8_t *);
    link_function(
        linker,
        "rudel:base/base",
        "get-name",
        Func::wrap(
            &mut store,
            |mut caller: Caller<'_, T>, offset: i32| -> Result<(), wasmi::Error> {
                let memory = get_memory(&caller)?;
                let data = get_mut_array::<T, 16>(&memory, &mut caller, offset)?;
                return glue::get_name(caller, data);
            },
        ),
    )?;

    return Ok(());
}

/// Provides functions that glue the relatively raw host functions to the implementation of Host
mod glue {
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
    pub(super) fn sleep<T: Host>(
        mut caller: Caller<'_, T>,
        micros: u64,
    ) -> Result<(), wasmi::Error> {
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
    pub(super) fn get_ambient_light<T: Host>(
        mut caller: Caller<'_, T>,
    ) -> Result<u32, wasmi::Error> {
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
}

/// Link the led functions provided by T.
///
/// This functions will provide the rudel-host functions to the linker by generating glue code for the functionality provided by the host implementation T
fn link_hardware<T: Host>(
    linker: &mut Linker<T>,
    mut store: &mut Store<T>,
) -> Result<(), wasmi::Error> {
    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("get-hardware-version")))
    // extern void __wasm_import_rudel_base_hardware_get_hardware_version(uint8_t *);
    link_function(
        linker,
        "rudel:base/hardware",
        "get-hardware-version",
        Func::wrap(
            &mut store,
            |mut caller: Caller<'_, T>, offset: i32| -> Result<(), wasmi::Error> {
                let memory = get_memory(&caller)?;
                let slice = get_mut_slice(&memory, &mut caller, offset, 4)?;
                // SAFETY: Should be safe because the layout should match
                let version = unsafe {
                    std::mem::transmute::<*mut u8, *mut SemanticVersion>(slice.as_mut_ptr())
                };
                let version_ref = unsafe { &mut *version };
                glue::get_hardware_version(caller, version_ref)?;

                return Ok(());
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("set-leds")))
    // extern void __wasm_import_rudel_base_hardware_set_leds(int32_t, uint8_t *, size_t);
    link_function(
        linker,
        "rudel:base/hardware",
        "set-leds",
        Func::wrap(
            &mut store,
            |mut caller: Caller<'_, T>, offset: i32, length: i32| -> Result<(), wasmi::Error> {
                let memory = get_memory(&caller)?;
                let slice = get_slice(&memory, &mut caller, offset, length * 2)?;
                // SAFETY: Should be safe because the layout should match
                let led_values =
                    unsafe { std::mem::transmute::<*const u8, *const u16>(slice.as_ptr()) };
                let values_slice =
                    unsafe { std::slice::from_raw_parts(led_values, length as usize) };
                glue::set_leds(caller, values_slice)?;

                return Ok(());
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("set-rgb")))
    // extern void __wasm_import_rudel_base_hardware_set_rgb(int32_t, int32_t, int32_t, int32_t);
    link_function(
        linker,
        "rudel:base/hardware",
        "set-leds",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>,
             red: i32,
             green: i32,
             blue: i32,
             lux: i32|
             -> Result<(), wasmi::Error> {
                let color = LedColor {
                    red: red.to_le_bytes()[0],
                    green: green.to_le_bytes()[0],
                    blue: blue.to_le_bytes()[0],
                };
                return glue::set_rgb(caller, &color, lux as u32);
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("led-count")))
    // extern int32_t __wasm_import_rudel_base_hardware_led_count(void);
    link_function(
        linker,
        "rudel:base/hardware",
        "led-count",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>| -> Result<i32, wasmi::Error> {
                return glue::led_count(caller).map(|result| result as i32);
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("get-led-info")))
    // extern void __wasm_import_rudel_base_hardware_get_led_info(int32_t, uint8_t *);
    link_function(
        linker,
        "rudel:base/hardware",
        "get-hardware-version",
        Func::wrap(
            &mut store,
            |mut caller: Caller<'_, T>, id: i32, offset: i32| -> Result<(), wasmi::Error> {
                let memory = get_memory(&caller)?;
                let slice = get_mut_slice(&memory, &mut caller, offset, 6)?;
                // Layout in memory is
                // 0: red
                // 1: green
                // 2: blue
                // 4: -
                // 5: lux_high
                // 6: lux_low
                // SAFETY: Should be safe because the layout should match
                let led_info_ptr =
                    unsafe { std::mem::transmute::<*mut u8, *mut LedInfo>(slice.as_mut_ptr()) };
                let led_info = unsafe { &mut *led_info_ptr };
                return glue::get_led_info(caller, id as u16, led_info);
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("get-ambient-light-type")))
    // extern int32_t __wasm_import_rudel_base_hardware_get_ambient_light_type(void);
    link_function(
        linker,
        "rudel:base/hardware",
        "get-ambient-light-type",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>| -> Result<i32, wasmi::Error> {
                return glue::get_ambient_light_type(caller).map(|result| result.lower());
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("get-ambient-light")))
    // extern int32_t __wasm_import_rudel_base_hardware_get_ambient_light(void);
    link_function(
        linker,
        "rudel:base/hardware",
        "get-ambient-light",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>| -> Result<i32, wasmi::Error> {
                return glue::get_ambient_light(caller).map(|result| result as i32);
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("get-vibration-sensor-type")))
    // extern int32_t __wasm_import_rudel_base_hardware_vibration_type(void);
    link_function(
        linker,
        "rudel:base/hardware",
        "get-vibration-sensor-type",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>| -> Result<i32, wasmi::Error> {
                return glue::get_vibration_sensor_type(caller).map(|result| result.lower());
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("get-vibration")))
    // extern int32_t __wasm_import_rudel_base_hardware_get_vibration(void);
    link_function(
        linker,
        "rudel:base/hardware",
        "get-vibration",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>| -> Result<i32, wasmi::Error> {
                return glue::get_vibration(caller).map(|result| result as i32);
            },
        ),
    )?;

    return Ok(());
}

/// Link the host functions provided by T.
///
/// This functions will provide the rudel-host functions to the linker by generating glue code for the functionality provided by the host implementation T
pub fn setup_linker<T: Host>(
    linker: &mut Linker<T>,
    store: &mut Store<T>,
) -> Result<(), wasmi::Error> {
    link_base(linker, store)?;
    link_hardware(linker, store)?;

    return Ok(());
}
