// pub mod common;
mod rudel;
pub use rudel::{
    export, exports,
    exports::rudel::base::run::Guest,
    rudel::base::base::{has_host_base, log, time, yield_now, LogLevel},
};

pub fn get_name() -> String {
    let tuple = rudel::rudel::base::base::get_name();
    let array: [u8; 16] = [
        tuple.0, tuple.1, tuple.2, tuple.3, tuple.4, tuple.5, tuple.6, tuple.7, tuple.8, tuple.9,
        tuple.10, tuple.11, tuple.12, tuple.13, tuple.14, tuple.15,
    ];
    let length = array
        .iter()
        .enumerate()
        .find(|(_, x)| **x == 0)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let array = &array[0..length];
    String::from_utf8_lossy(array).to_string()
}
