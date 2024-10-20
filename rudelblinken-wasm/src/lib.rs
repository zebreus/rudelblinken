use rudelblinken_sdk::{common::TestArgument, guest::host};

/* extern "C" {
    fn host_log(args: usize);
    fn get_name() -> usize;
    // fn set_led_brightness(rgb: usize);
    fn configure_ble_adv(settings: usize);
    // fn configure_ble_data(data: usize);
    // fn configure_adv_recv_callbacl(cb: usize);
} */

#[no_mangle]
extern "C" fn main() {
    /* let name = host::get_name();
    host::configure_ble_adv(&BLEAdvSettings {
        min_interval: name.len() as u32,
        max_interval: name.bytes().next().unwrap() as u32,
    }); */
    let resp = host::test(&TestArgument {
        min_interval: 13,
        max_interval: 12,
        test_string: "Hello world".to_owned(),
    });
    let _ = host::test(&TestArgument {
        min_interval: resp.min_interval,
        max_interval: resp.max_interval,
        test_string: resp.test_string,
    });
    println!("aa")
}

/* #[no_mangle]
extern "C" fn alloc(cap: usize) -> usize {
    Region::leak(Vec::with_capacity(cap)) as usize
}

#[no_mangle]
extern "C" fn free(ptr: usize) {
    let _ = unsafe { Region::consume(ptr as *mut Region) };
}

mod host {
    use rkyv::rancor::Error;

    pub fn get_name() -> String {
        log("call get_name");
        let (ptr, len) = unsafe { super::get_name() };
        log(format!("ptr = {}, len = {}", ptr, len));
        let ret = unsafe { std::slice::from_raw_parts(ptr as *mut u8, len) };
        String::from_utf8(ret.into()).expect("non-string")
    }

    pub fn log<S: Into<String>>(msg: S) {
        let b = rkyv::to_bytes::<Error>(&super::LogArgs { msg: msg.into() })
            .expect("failed to serialize");
        let arg_ptr = super::Region::leak(b.into_vec()) as usize;
        unsafe { super::host_log(arg_ptr) };
    }

    pub fn configure_ble_adv(settings: &super::BLEAdvSettings) {
        let b = rkyv::to_bytes::<Error>(settings).expect("failed to serialize");
        let arg_ptr = super::Region::leak(b.into_vec()) as usize;
        unsafe { super::configure_ble_adv(arg_ptr) };
    }
}

#[repr(C)]
pub struct Region {
    addr: usize,
    cap: usize,
    len: usize,
}

#[derive(Archive, Deserialize, Serialize)]
struct LogArgs {
    msg: String,
}

#[derive(Archive, Deserialize, Serialize)]
struct BLEAdvSettings {
    min_interval: u32,
    max_interval: u32,
}

impl Region {
    pub fn build(data: &[u8]) -> Box<Self> {
        Box::new(Self {
            addr: data.as_ptr() as usize,
            cap: data.len(),
            len: data.len(),
        })
    }

    pub fn leak(data: Vec<u8>) -> *mut Self {
        let reg = Box::new(Self {
            addr: data.as_ptr() as usize,
            cap: data.capacity(),
            len: data.len(),
        });
        std::mem::forget(data);
        Box::into_raw(reg)
    }

    pub unsafe fn consume(ptr: *mut Region) -> Vec<u8> {
        host::log("test");
        assert!(!ptr.is_null(), "Tried to consume null region");

        let reg = Box::from_raw(ptr);
        let saddr = reg.addr as *mut u8;
        assert!(
            !(reg.addr as *mut u8).is_null(),
            "Tried to consume null region"
        );

        unsafe { Vec::from_raw_parts(saddr, reg.len, reg.cap) }
    }
}
*/
