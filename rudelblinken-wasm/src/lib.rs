extern "C" {
    fn ping(value: i32);
}

#[no_mangle]
extern "C" fn add(left: u64, right: u64) -> u64 {
    unsafe { ping(0x16c1) };
    left + right
}
