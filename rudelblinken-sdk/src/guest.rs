mod host_raw {
    use crate::common::Region;

    extern "C" {
        // fn host_log(args: usize);
        // fn get_name() -> usize;
        // fn set_led_brightness(rgb: usize);
        // fn configure_ble_adv(settings: usize);
        // fn configure_ble_data(data: usize);
        // fn configure_adv_recv_callbacl(cb: usize);
        pub(super) fn test(arg: usize) -> usize;
    }

    #[no_mangle]
    extern "C" fn alloc_mem(len: usize) -> usize {
        /*
        // FIXME(lmv): can offsetting everything by 4 bytes for the length
        // prefix cause any alignment issues? for now I am sticking with 8, just
        // in case
        let full_len = len + 8;
        let mut mem_vec = vec![0; full_len];
        let cap = mem_vec.capacity();
        mem_vec.copy_from_slice(&full_len.to_le_bytes());
        mem_vec.copy_from_slice(&cap.to_le_bytes());
        let ptr = mem_vec.as_ptr() as usize;
        std::mem::forget(mem_vec);
        ptr + 8 */
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
        /* let base_ptr = ptr - 8;
        let len_mem = unsafe { std::slice::from_raw_parts(base_ptr as *mut u8, 8) };
        let len = u32::from_le_bytes(len_mem[..4].try_into().unwrap()) as usize;
        let cap = u32::from_le_bytes(len_mem[4..8].try_into().unwrap()) as usize;
        let mem = unsafe { Vec::<u8>::from_raw_parts(base_ptr as *mut u8, len, cap) };
        drop(mem); */
        /* std::mem::forget(len_mem);
        let mem =
            unsafe { std::slice::from_raw_parts(base_ptr as *mut MaybeUninit<u8>, len as usize) };
        drop(mem); */
    }
}

pub mod host {
    use super::host_raw;
    use crate::common::{Region, TestArgument, TestResult};

    pub fn test(arg: &TestArgument) -> TestResult {
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(arg).expect("failed to serialize");
        let ptr_arg = host_raw::leak_vec(b.into_vec()) as usize;
        let res_ptr = unsafe { host_raw::test(ptr_arg) };

        let res_buf = super::host_raw::recover_vec(res_ptr as *mut Region);
        rkyv::from_bytes::<_, rkyv::rancor::Error>(&res_buf).expect("failed to deserialize")
    }
}
