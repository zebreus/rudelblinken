use rkyv::{Archive, Deserialize, Serialize};

#[derive(Debug, Archive, Deserialize, Serialize)]
pub struct TestArgument {
    pub min_interval: u32,
    pub max_interval: u32,
    pub test_string: String,
}

#[derive(Debug, Archive, Deserialize, Serialize)]
pub struct TestResult {
    pub min_interval: u32,
    pub max_interval: u32,
    pub test_string: String,
}

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Region {
    pub(crate) ptr: u32,
    pub(crate) len: u32,
    pub(crate) cap: u32,
}
