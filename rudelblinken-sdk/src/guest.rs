mod host_raw {
    use crate::common::{BLEAdvNotification, Region};

    extern "C" {
        // () -> ()
        pub(super) fn rt_yield();

        // () -> bool
        pub(super) fn has_host_base() -> bool;
        // Log -> ()
        pub(super) fn host_log(log_args: usize);
        // () -> String
        pub(super) fn get_name() -> usize;

        // () -> bool
        pub(super) fn has_led_brightness() -> bool;
        // LEDBrightnessSettings -> ()
        pub(super) fn set_led_brightness(settings: usize);

        // () -> bool
        pub(super) fn has_ble_adv() -> bool;
        // BLEAdvSettings -> ()
        pub(super) fn configure_ble_adv(settings: usize);
        // BLEAdvData -> ()
        pub(super) fn configure_ble_data(data: usize);
        // fn configure_adv_recv_callbacl(cb: usize);
    }

    #[no_mangle]
    extern "C" fn alloc_mem(len: usize) -> usize {
        leak_vec(vec![0; len]) as usize
    }

    pub(super) fn leak_vec(v: Vec<u8>) -> *mut Region {
        let r = Box::new(Region {
            ptr: v.as_ptr() as u32,
            cap: v.capacity() as u32,
            len: v.len() as u32,
        });
        std::mem::forget(v);
        Box::into_raw(r)
    }

    pub(super) fn recover_vec(ptr: *mut Region) -> Vec<u8> {
        let reg = unsafe { Box::from_raw(ptr) };
        unsafe { Vec::from_raw_parts(reg.ptr as *mut u8, reg.len as usize, reg.cap as usize) }
    }

    #[no_mangle]
    extern "C" fn dealloc_mem(ptr: usize) {
        drop(recover_vec(ptr as *mut Region))
    }

    pub(super) static mut ON_BLE_ADV_RECV: Option<
        Box<dyn FnMut(BLEAdvNotification) + Send + Sync>,
    > = None;

    #[no_mangle]
    extern "C" fn on_ble_adv_recv(arg_ptr: usize) {
        let arg_buf = super::host_raw::recover_vec(arg_ptr as *mut Region);
        let arg =
            rkyv::from_bytes::<_, rkyv::rancor::Error>(&arg_buf).expect("failed to deserialize");

        // FIXME(lmv): this is not thread-safe
        if let Some(ref mut cb) = unsafe {
            #[allow(static_mut_refs)]
            &mut ON_BLE_ADV_RECV
        } {
            cb(arg);
        }
    }
}

pub mod host {
    use super::host_raw;
    use crate::common::{
        BLEAdvData, BLEAdvNotification, BLEAdvSettings, LEDBrightnessSettings, Log, Region,
    };

    // must be called regularly or the guest might run out of fuel
    pub fn rt_yield() {
        unsafe { host_raw::rt_yield() }
    }

    // TODO(lmv): maybe generate these wrappers this with a proc macro?

    pub fn has_host_base() -> bool {
        unsafe { host_raw::has_host_base() }
    }

    // FIXME(lmv): return (de)serialization errors properly

    // FIXME(lmv): provide a tracing-compatible abstraction for logging from wasm
    pub fn host_log(log: &Log) {
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(log).expect("failed to serialize");
        let ptr_arg = host_raw::leak_vec(b.into_vec()) as usize;
        unsafe { host_raw::host_log(ptr_arg) };
    }

    pub fn get_name() -> String {
        let res_ptr = unsafe { host_raw::get_name() };

        let res_buf = super::host_raw::recover_vec(res_ptr as *mut Region);
        rkyv::from_bytes::<_, rkyv::rancor::Error>(&res_buf).expect("failed to deserialize")
    }

    pub fn has_led_brightness() -> bool {
        unsafe { host_raw::has_led_brightness() }
    }

    pub fn set_led_brightness(settings: &LEDBrightnessSettings) {
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(settings).expect("failed to serialize");
        let ptr_arg = host_raw::leak_vec(b.into_vec()) as usize;
        unsafe { host_raw::set_led_brightness(ptr_arg) };
    }

    pub fn has_ble_adv() -> bool {
        unsafe { host_raw::has_ble_adv() }
    }

    pub fn configure_ble_adv(settings: &BLEAdvSettings) {
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(settings).expect("failed to serialize");
        let ptr_arg = host_raw::leak_vec(b.into_vec()) as usize;
        unsafe { host_raw::configure_ble_adv(ptr_arg) };
    }

    pub fn configure_ble_data(data: &BLEAdvData) {
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(data).expect("failed to serialize");
        let ptr_arg = host_raw::leak_vec(b.into_vec()) as usize;
        unsafe { host_raw::configure_ble_data(ptr_arg) };
    }

    pub fn set_on_ble_adv_recv_callback<F>(cb: Option<F>)
    where
        F: FnMut(BLEAdvNotification) + Send + Sync + 'static,
    {
        // FIXME(lmv): this is not thread-safe
        unsafe {
            host_raw::ON_BLE_ADV_RECV =
                cb.map(|cb| -> Box<dyn FnMut(BLEAdvNotification) + Send + Sync> { Box::new(cb) })
        }
    }
}

// TODO(lmv): provide a way to nicely implement callbacks (e.g. bluetooth
// advertisement received) from wasm; not sure how to implement this
//
// ideas:
//
// - somehow (not sure if it's possible) pass the actual function reference to
//   the host and have it call it properly
//
// - export constant callback functions in here, which are called from the host
//   and then use a mutable global to call to registered callbacks
