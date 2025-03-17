use super::{super::glue, get_mut_slice};
use super::{get_memory, get_slice, link_function, WrappedCaller};
use crate::host::{
    Advertisement, AdvertisementSettings, BleEvent, Host, ManufacturerData, SemanticVersion,
    ServiceData,
};
use wasmi::{Caller, Extern, Func, Linker, Store};

type GuestPtr = u32;

mod _rt {
    #![allow(dead_code, clippy::all)]

    extern crate alloc as alloc_crate;

    // These should probably alloc in the guest
    pub use alloc_crate::string::String;
    pub use alloc_crate::vec::Vec;

    pub fn as_i64<T: AsI64>(t: T) -> i64 {
        t.as_i64()
    }
    pub trait AsI64 {
        fn as_i64(self) -> i64;
    }
    impl<'a, T: Copy + AsI64> AsI64 for &'a T {
        fn as_i64(self) -> i64 {
            (*self).as_i64()
        }
    }
    impl AsI64 for i64 {
        #[inline]
        fn as_i64(self) -> i64 {
            self as i64
        }
    }
    impl AsI64 for u64 {
        #[inline]
        fn as_i64(self) -> i64 {
            self as i64
        }
    }
    pub fn as_i32<T: AsI32>(t: T) -> i32 {
        t.as_i32()
    }
    pub trait AsI32 {
        fn as_i32(self) -> i32;
    }
    impl<'a, T: Copy + AsI32> AsI32 for &'a T {
        fn as_i32(self) -> i32 {
            (*self).as_i32()
        }
    }
    impl AsI32 for i32 {
        #[inline]
        fn as_i32(self) -> i32 {
            self as i32
        }
    }
    impl AsI32 for u32 {
        #[inline]
        fn as_i32(self) -> i32 {
            self as i32
        }
    }
    impl AsI32 for i16 {
        #[inline]
        fn as_i32(self) -> i32 {
            self as i32
        }
    }
    impl AsI32 for u16 {
        #[inline]
        fn as_i32(self) -> i32 {
            self as i32
        }
    }
    impl AsI32 for i8 {
        #[inline]
        fn as_i32(self) -> i32 {
            self as i32
        }
    }
    impl AsI32 for u8 {
        #[inline]
        fn as_i32(self) -> i32 {
            self as i32
        }
    }
    impl AsI32 for char {
        #[inline]
        fn as_i32(self) -> i32 {
            self as i32
        }
    }
    impl AsI32 for usize {
        #[inline]
        fn as_i32(self) -> i32 {
            self as i32
        }
    }
    // pub use alloc_crate::alloc;
    pub mod alloc {
        use crate::{host::Host, linker::linker::WrappedCaller};

        pub use super::alloc_crate::alloc::Layout;

        /// Original function signature:
        /// ```ignore
        /// pub fn alloc(layout: Layout) -> *mut u8
        /// ```
        pub fn alloc<'a, T: Host>(
            host: &mut WrappedCaller<'a, T>,
            layout: Layout,
        ) -> Result<u32, wasmi::Error> {
            let offset = host.realloc(0, 0, layout.align() as u32, layout.size() as u32)?;
            return Ok(offset);
        }

        /// Copy a byte slice to a newly allocated memory region.
        /// TODO: This function does not belong here
        pub unsafe fn copy_to_alloc<'a, T: Host>(
            host: &mut WrappedCaller<'a, T>,
            slice: &[u8],
        ) -> Result<(u32, Layout), wasmi::Error> {
            let layout = Layout::from_size_align(slice.len(), 1).unwrap();
            let offset: u32 = alloc(host, layout)?;
            if offset == 0 {
                return Err(handle_alloc_error(layout));
            }
            let chunk = host.get_mut_slice(offset, slice.len() as u32)?;
            chunk.copy_from_slice(slice);
            return Ok((offset, layout));
        }
        /// Original function signature:
        /// ```ignore
        /// pub fn handle_alloc_error(layout: Layout) -> !
        /// ```
        pub fn handle_alloc_error(_layout: Layout) -> wasmi::Error {
            return wasmi::Error::new("Allocation error");
        }
        /// Original function signature:
        /// ```ignore
        /// pub fn dealloc(ptr: *mut u8, layout: Layout)
        /// ```
        pub fn dealloc<'a, T: Host>(
            host: &mut WrappedCaller<'a, T>,
            ptr: u32,
            layout: Layout,
        ) -> Result<u32, wasmi::Error> {
            let alloc_result =
                host.realloc(ptr as u32, layout.size() as u32, layout.align() as u32, 0)?;
            return Ok(alloc_result);
        }
    }

    pub unsafe fn string_lift(bytes: Vec<u8>) -> String {
        if cfg!(debug_assertions) {
            String::from_utf8(bytes).unwrap()
        } else {
            String::from_utf8_unchecked(bytes)
        }
    }
    // pub unsafe fn cabi_dealloc(ptr: *mut u8, size: usize, align: usize) {
    //     if size == 0 {
    //         return;
    //     }
    //     let layout = alloc::Layout::from_size_align_unchecked(size, align);
    //     alloc::dealloc(ptr, layout);
    // }
}

impl<'a, T: Host> WrappedCaller<'a, T> {
    pub fn on_event(&mut self, event: BleEvent) -> Result<(), wasmi::Error> {
        let Some(run) = self.0.get_export("rudel:base/ble-guest@0.0.1#on-event") else {
            return Err(wasmi::Error::new("on-event not found"));
        };
        let Extern::Func(run) = run else {
            return Err(wasmi::Error::new("on-event is not a function"));
        };

        type BleEventParams = (
            u32, // Ble Event type
            u64,
            u64,
            u32,
            u32,
            u32, //*mut u8,
            u32, //usize,
            u32, //*mut u8,
            u32, //usize,
        );
        let Ok(run) = run.typed::<BleEventParams, ()>(&self.0) else {
            return Err(wasmi::Error::new(
                "on-advertisement does not have a matching function signature",
            ));
        };

        // THIS IS THE GENERATED CODE (with minor modifications)
        // Update it by pasting new generated code, and then fix the calls to alloc
        let mut cleanup_list = _rt::Vec::new();
        let (
            result7_0,
            result7_1,
            result7_2,
            result7_3,
            result7_4,
            result7_5,
            result7_6,
            result7_7,
            result7_8,
        ) = match event {
            BleEvent::Advertisement(e) => unsafe {
                let Advertisement {
                    address: address0,
                    received_at: received_at0,
                    manufacturer_data: manufacturer_data0,
                    service_data: service_data0,
                } = e;
                let (result3_0, result3_1, result3_2, result3_3) = match manufacturer_data0 {
                    Some(e) => {
                        let ManufacturerData {
                            manufacturer_id: manufacturer_id1,
                            data: data1,
                        } = e;
                        let vec2 = data1;
                        let (ptr, layout) = _rt::alloc::copy_to_alloc(self, &vec2)?;
                        cleanup_list.push((ptr, layout));

                        (1i32, _rt::as_i32(manufacturer_id1), ptr, vec2.len())
                    }
                    None => (0i32, 0i32, 0, 0usize),
                };
                let vec6 = service_data0;
                let len6 = vec6.len();
                let layout6 = _rt::alloc::Layout::from_size_align_unchecked(
                    vec6.len() * (3 * ::core::mem::size_of::<GuestPtr>()),
                    ::core::mem::size_of::<GuestPtr>(),
                );
                let result6_offset = if layout6.size() != 0 {
                    let offset = _rt::alloc::alloc(self, layout6)?;
                    if offset == 0 {
                        return Err(_rt::alloc::handle_alloc_error(layout6));
                    }
                    offset
                } else {
                    0
                };
                let result6_slice = self.get_mut_slice(result6_offset, layout6.size() as u32)?;
                let result6 = result6_slice.as_mut_ptr().cast::<u8>();

                for (i, e) in vec6.into_iter().enumerate() {
                    let base = result6.add(i * (3 * ::core::mem::size_of::<GuestPtr>()));
                    {
                        let ServiceData {
                            uuid: uuid4,
                            data: data4,
                        } = e;
                        *base.add(0).cast::<u16>() = (_rt::as_i32(uuid4)) as u16;
                        let vec5 = data4;
                        let ptr5 = vec5.as_ptr().cast::<u8>();
                        let len5 = vec5.len();
                        *base
                            .add(2 * ::core::mem::size_of::<GuestPtr>())
                            .cast::<usize>() = len5;
                        *base
                            .add(::core::mem::size_of::<GuestPtr>())
                            .cast::<*mut u8>() = ptr5.cast_mut();
                    }
                }
                cleanup_list.extend_from_slice(&[(result6_offset, layout6)]);
                (
                    0i32,
                    _rt::as_i64(address0),
                    _rt::as_i64(received_at0),
                    result3_0,
                    result3_1,
                    result3_2,
                    result3_3,
                    result6,
                    len6,
                )
            },
        };
        // GENERATED CODE ENDS HERE

        run.call(
            &mut self.0,
            (
                result7_0 as u32,
                result7_1 as u64,
                result7_2 as u64,
                result7_3 as u32,
                result7_4 as u32,
                result7_5 as u32,
                result7_6 as u32,
                result7_7 as u32,
                result7_8 as u32,
            ),
        )?;
        // for (ptr, layout) in cleanup_list {
        //     if layout.size() != 0 {
        //         _rt::alloc::dealloc(self, ptr, layout)?;
        //     }
        // }

        return Ok(());
    }
}

/// Link the led functions provided by T.
///
/// This functions will provide the rudel-host functions to the linker by generating glue code for the functionality provided by the host implementation T
pub fn link_ble<T: Host>(
    linker: &mut Linker<T>,
    mut store: &mut Store<T>,
) -> Result<(), wasmi::Error> {
    // __attribute__((__import_module__("rudel:base/ble@0.0.1"), __import_name__("get-ble-version")))
    // extern void __wasm_import_rudel_base_ble_get_ble_version(uint8_t *);
    link_function(
        linker,
        "rudel:base/ble",
        "get-ble-version",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>, offset: i32| -> Result<(), wasmi::Error> {
                let mut caller = WrappedCaller(caller);
                let memory = get_memory(caller.as_ref())?;
                let slice = get_mut_slice(&memory, caller.as_mut(), offset as u32, 4)?;
                // SAFETY: Should be safe because the layout should match
                let version = unsafe {
                    std::mem::transmute::<*mut u8, *mut SemanticVersion>(slice.as_mut_ptr())
                };
                let version_ref = unsafe { &mut *version };
                glue::get_ble_version(caller, version_ref)?;

                return Ok(());
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/ble@0.0.1"), __import_name__("configure-advertisement")))
    // extern void __wasm_import_rudel_base_ble_configure_advertisement(int32_t, int32_t);
    link_function(
        linker,
        "rudel:base/ble",
        "configure-advertisement",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>,
             min_interval: i32,
             max_interval: i32|
             -> Result<u32, wasmi::Error> {
                let caller = WrappedCaller(caller);

                glue::configure_advertisement(
                    caller,
                    AdvertisementSettings {
                        max_interval: max_interval as u16,
                        min_interval: min_interval as u16,
                    },
                )
            },
        ),
    )?;

    // __attribute__((__import_module__("rudel:base/ble@0.0.1"), __import_name__("set-advertisement-data")))
    // extern void __wasm_import_rudel_base_ble_set_advertisement_data(uint8_t *, size_t);
    link_function(
        linker,
        "rudel:base/ble",
        "set-advertisement-data",
        Func::wrap(
            &mut store,
            |caller: Caller<'_, T>, offset: i32, length: i32| -> Result<u32, wasmi::Error> {
                let mut caller = WrappedCaller(caller);
                let memory = get_memory(caller.as_ref())?;
                let slice = get_slice(&memory, caller.as_mut(), offset, length)?;
                // // Remove lifetime
                // let data = unsafe { std::slice::from_raw_parts(slice.as_ptr(), length as usize) };

                glue::set_advertisement_data(caller, slice)
            },
        ),
    )?;

    return Ok(());
}
