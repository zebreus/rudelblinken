use crate::host::{Advertisement, Host, LedColor, LedInfo, LogLevel, SemanticVersion};
use wasmi::{Caller, Extern, Func, Linker, Memory, Store};

use super::glue;

#[repr(transparent)]
pub struct WrappedCaller<'a, T: Host + Sized>(Caller<'a, T>);

impl<'a, T: Host> WrappedCaller<'a, T> {
    pub fn new(caller: Caller<'a, T>) -> WrappedCaller<'a, T> {
        return WrappedCaller(caller);
    }
    pub fn into_inner(self) -> Caller<'a, T> {
        return self.0;
    }
    pub fn data(&self) -> &T {
        return self.0.data();
    }
    pub fn data_mut(&mut self) -> &mut T {
        return self.0.data_mut();
    }

    pub fn run(&mut self) -> Result<(), wasmi::Error> {
        let Some(run) = self.0.get_export("rudel:base/run@0.0.1#run") else {
            return Err(wasmi::Error::new("run not found"));
        };
        let Extern::Func(run) = run else {
            return Err(wasmi::Error::new("run is not a function"));
        };
        let Ok(run) = run.typed::<(), ()>(&self.0) else {
            return Err(wasmi::Error::new(
                "run does not have a matching function signature",
            ));
        };
        run.call(&mut self.0, ())?;
        return Ok(());
    }
    pub fn on_advertisement(&mut self, advertisement: Advertisement) -> Result<(), wasmi::Error> {
        let Some(run) = self
            .0
            .get_export("rudel:base/ble-guest@0.0.1#on-advertisement")
        else {
            return Err(wasmi::Error::new("on-advertisement not found"));
        };
        let Extern::Func(run) = run else {
            return Err(wasmi::Error::new("on-advertisement is not a function"));
        };
        let Ok(run) =
            run.typed::<(u64, u32, u32, u32, u32, u32, u32, u32, u32, u32, u64), ()>(&self.0)
        else {
            return Err(wasmi::Error::new(
                "on-advertisement does not have a matching function signature",
            ));
        };

        let address = u64::from_le_bytes(advertisement.address);
        let data = unsafe { std::mem::transmute::<[u8; 32], [u32; 8]>(advertisement.data) };
        run.call(
            &mut self.0,
            (
                address,
                data[0],
                data[1],
                data[2],
                data[3],
                data[4],
                data[5],
                data[6],
                data[7],
                advertisement.data_length as u32,
                advertisement.received_at,
            ),
        )?;
        return Ok(());
    }
}

impl<'a, T: Host> AsRef<Caller<'a, T>> for WrappedCaller<'a, T> {
    fn as_ref(&self) -> &Caller<'a, T> {
        return &self.0;
    }
}
impl<'a, T: Host> AsMut<Caller<'a, T>> for WrappedCaller<'a, T> {
    fn as_mut(&mut self) -> &mut Caller<'a, T> {
        return &mut self.0;
    }
}

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
pub fn link_base<T: Host>(
    linker: &mut Linker<T>,
    mut store: &mut Store<T>,
) -> Result<(), wasmi::Error> {
    // __attribute__((__import_module__("rudel:base/base@0.0.1"), __import_name__("get-base-version")))
    // extern void __wasm_import_rudel_base_base_get_base_version(uint8_t *);
    link_function(
        linker,
        "rudel:base/base",
        "get-base-version",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>, offset: i32| -> Result<(), wasmi::Error> {
                let mut caller = WrappedCaller(caller);
                let memory = get_memory(caller.as_ref())?;
                let slice = get_mut_slice(&memory, caller.as_mut(), offset, 4)?;
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
                let caller = WrappedCaller(caller);
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
                let caller = WrappedCaller(caller);
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
                let caller = WrappedCaller(caller);
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
                let caller = WrappedCaller(caller);

                let log_level = LogLevel::lift(level);

                let memory = get_memory(caller.as_ref())?;
                let data = get_slice(&memory, caller.as_ref(), message_offset, message_length)?;
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
            |caller: Caller<'_, T>, offset: i32| -> Result<(), wasmi::Error> {
                let mut caller = WrappedCaller(caller);
                let memory = get_memory(caller.as_ref())?;
                let data = get_mut_array::<T, 16>(&memory, caller.as_mut(), offset)?;
                return glue::get_name(caller, data);
            },
        ),
    )?;

    return Ok(());
}

/// Link the led functions provided by T.
///
/// This functions will provide the rudel-host functions to the linker by generating glue code for the functionality provided by the host implementation T
pub fn link_hardware<T: Host>(
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
            |caller: Caller<'_, T>, offset: i32| -> Result<(), wasmi::Error> {
                let mut caller = WrappedCaller(caller);
                let memory = get_memory(caller.as_ref())?;
                let slice = get_mut_slice(&memory, caller.as_mut(), offset, 4)?;
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
            |caller: Caller<'_, T>, offset: i32, length: i32| -> Result<(), wasmi::Error> {
                let mut caller = WrappedCaller(caller);
                let memory = get_memory(caller.as_ref())?;
                let slice = get_slice(&memory, caller.as_mut(), offset, length * 2)?;
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
        "set-rgb",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>,
             red: i32,
             green: i32,
             blue: i32,
             lux: i32|
             -> Result<(), wasmi::Error> {
                let caller = WrappedCaller(caller);
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
                let caller = WrappedCaller(caller);
                return glue::led_count(caller).map(|result| result as i32);
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/hardware@0.0.1"), __import_name__("get-led-info")))
    // extern void __wasm_import_rudel_base_hardware_get_led_info(int32_t, uint8_t *);
    link_function(
        linker,
        "rudel:base/hardware",
        "get-led-info",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>, id: i32, offset: i32| -> Result<(), wasmi::Error> {
                let mut caller = WrappedCaller(caller);
                let memory = get_memory(caller.as_ref())?;
                let slice = get_mut_slice(&memory, caller.as_mut(), offset, 6)?;
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
                let caller = WrappedCaller(caller);
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
                let caller = WrappedCaller(caller);
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
                let caller = WrappedCaller(caller);
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
                let caller = WrappedCaller(caller);
                return glue::get_vibration(caller).map(|result| result as i32);
            },
        ),
    )?;

    return Ok(());
}
