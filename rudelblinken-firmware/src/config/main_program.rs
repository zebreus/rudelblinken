use std::sync::{LazyLock, RwLock};

use super::CONFIG_NVS;

static MAIN_PROGRAM_HASH: LazyLock<RwLock<Option<[u8; 32]>>> = LazyLock::new(|| {
    let nvs = CONFIG_NVS.read().unwrap();

    let mut buffer = [0u8; 32];
    if let Ok(Some(hash)) = nvs.get_blob("main_program", &mut buffer) {
        if let Result::<&[u8; 32], _>::Ok(hash) = hash.try_into() {
            return RwLock::new(Some(hash.clone()));
        }
    }

    RwLock::new(None)
});

pub fn get_main_program() -> Option<[u8; 32]> {
    return MAIN_PROGRAM_HASH.read().unwrap().clone();
}

pub fn set_main_program(new_hash: Option<&[u8; 32]>) {
    let mut nvs = CONFIG_NVS.write().unwrap();

    match new_hash {
        Some(hash) => nvs.set_blob("main_program", hash).unwrap(),
        None => {
            nvs.remove("main_program").unwrap();
        }
    }
    let mut hash = MAIN_PROGRAM_HASH.write().unwrap();
    *hash = new_hash.cloned();
    // *hash = new_hash.as_ref();
}
