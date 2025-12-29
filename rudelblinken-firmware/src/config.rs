use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, EspNvsPartition, NvsDefault};
use rudelblinken_runtime::host::LedColor;
use std::sync::{LazyLock, RwLock};

pub static NVS_PARTITION: LazyLock<EspNvsPartition<NvsDefault>> = LazyLock::new(|| {
    let nvs_default_partition: EspNvsPartition<NvsDefault> =
        EspDefaultNvsPartition::take().unwrap();
    nvs_default_partition
});

/// NVS Storage for persistent configuration
pub static CONFIG_NVS: LazyLock<RwLock<EspNvs<NvsDefault>>> = LazyLock::new(|| {
    let nvs_default_partition: EspNvsPartition<NvsDefault> = NVS_PARTITION.clone();
    let nvs = EspNvs::new(nvs_default_partition, "config", true)
        .expect("Failed to open NVS storage for configuration");
    RwLock::new(nvs)
});

pub trait StorableValue: Clone {
    fn initial_value() -> Self;
    fn decode(encoded: &[u8]) -> Option<Self>;
    fn encode(&self) -> impl AsRef<[u8]>;
}

pub trait InnerConfig {
    type V;
}

pub trait ConfigValue: Sized + StorableValue + InnerConfig + 'static {
    const IDENTIFIER: &'static str;

    fn storage() -> &'static LazyLock<RwLock<Self>>;

    fn from_inner(inner: Self::V) -> Self;

    fn to_inner(self) -> Self::V;
}

const fn setup_config_storage<V: ConfigValue>() -> LazyLock<RwLock<V>> {
    LazyLock::new(|| {
        tracing::info!(id = V::IDENTIFIER, "initializing config value");

        let nvs = CONFIG_NVS.read().unwrap();

        if let Ok(Some(buf_len)) = nvs.blob_len(V::IDENTIFIER) {
            let mut buf = vec![0u8; buf_len];
            match nvs.get_blob(V::IDENTIFIER, &mut buf) {
                Ok(Some(val)) => match V::decode(val) {
                    Some(val) => {
                        tracing::info!(id = V::IDENTIFIER, buf_len, "decoded blob value");
                        return RwLock::new(val);
                    }
                    None => {
                        tracing::warn!(
                            id = V::IDENTIFIER,
                            ?buf,
                            "decoding of config value return none"
                        );
                    }
                },
                Ok(None) => tracing::warn!(
                    id = V::IDENTIFIER,
                    buf_len,
                    "reading config value returned none"
                ),
                Err(err) => tracing::warn!(id = V::IDENTIFIER, ?err, "reading config value failed"),
            }
        } else {
            tracing::info!(id = V::IDENTIFIER, "config value not stored yet")
        }

        RwLock::new(V::initial_value())
    })
}

pub fn get_config<V: ConfigValue>() -> V::V {
    V::storage().read().unwrap().clone().to_inner()
}

pub fn set_config<V: ConfigValue>(val: V::V) {
    let val = V::from_inner(val);

    {
        let buf = val.encode();
        let mut nvs = CONFIG_NVS.write().unwrap();
        nvs.set_blob(V::IDENTIFIER, buf.as_ref()).unwrap();
    }
    {
        let mut dst = V::storage().write().unwrap();
        *dst = val;
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct DeviceName {
    name: String,
}

static DEVICE_NAME: LazyLock<RwLock<DeviceName>> = setup_config_storage();

impl StorableValue for DeviceName {
    fn initial_value() -> Self {
        let name = unsafe {
            let mut mac = [0u8; 6];
            esp_idf_sys::esp_base_mac_addr_get(mac.as_mut_ptr());
            format!(
                "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            )
        };
        Self { name }
    }

    fn decode(encoded: &[u8]) -> Option<Self> {
        String::from_utf8(encoded.to_vec())
            .ok()
            .map(|name| Self { name })
    }

    fn encode(&self) -> impl AsRef<[u8]> {
        self.name.as_bytes()
    }
}

impl InnerConfig for DeviceName {
    type V = String;
}

impl ConfigValue for DeviceName {
    const IDENTIFIER: &'static str = "device_name";

    fn storage() -> &'static LazyLock<RwLock<Self>> {
        &DEVICE_NAME
    }

    fn from_inner(inner: Self::V) -> Self {
        Self { name: inner }
    }

    fn to_inner(self) -> Self::V {
        self.name
    }
}

#[derive(Clone)]
pub struct LedStripColor {
    color: LedColor,
}

static LED_STRIP_COLOR: LazyLock<RwLock<LedStripColor>> = setup_config_storage();

impl StorableValue for LedStripColor {
    fn initial_value() -> Self {
        Self {
            color: LedColor::new(0xff, 0xff, 0xff),
        }
    }

    fn decode(encoded: &[u8]) -> Option<Self> {
        if encoded.len() == 3 {
            Some(Self {
                color: LedColor::new(encoded[0], encoded[1], encoded[2]),
            })
        } else {
            None
        }
    }

    fn encode(&self) -> impl AsRef<[u8]> {
        self.color.to_array()
    }
}

impl InnerConfig for LedStripColor {
    type V = LedColor;
}

impl ConfigValue for LedStripColor {
    const IDENTIFIER: &'static str = "led_strip_color";

    fn storage() -> &'static LazyLock<RwLock<Self>> {
        &LED_STRIP_COLOR
    }

    fn from_inner(inner: Self::V) -> Self {
        Self { color: inner }
    }

    fn to_inner(self) -> Self::V {
        self.color
    }
}

#[derive(Clone)]
pub struct WasmGuestConfig {
    config: Vec<u8>,
}

static WASM_GUEST_CONFIG: LazyLock<RwLock<WasmGuestConfig>> = setup_config_storage();

impl StorableValue for WasmGuestConfig {
    fn initial_value() -> Self {
        Self { config: vec![] }
    }

    fn decode(encoded: &[u8]) -> Option<Self> {
        Some(Self {
            config: encoded.to_vec(),
        })
    }

    fn encode(&self) -> impl AsRef<[u8]> {
        &self.config
    }
}

impl InnerConfig for WasmGuestConfig {
    type V = Vec<u8>;
}

impl ConfigValue for WasmGuestConfig {
    const IDENTIFIER: &'static str = "wasm_guest_cfg";

    fn storage() -> &'static LazyLock<RwLock<Self>> {
        &WASM_GUEST_CONFIG
    }

    fn from_inner(inner: Self::V) -> Self {
        Self { config: inner }
    }

    fn to_inner(self) -> Self::V {
        self.config
    }
}

macro_rules! config_value {
    ($name:ident, bool) => {
        config_value!(
            $name,
            bool,
            1,
            |v: &bool| Some([if *v { 1 } else { 0 }]),
            |v: Option<&[u8]>| v.map_or(false, |v| v[0] != 0)
        );
    };
    ($name:ident, u32) => {
        config_value!(
            $name,
            u32,
            4,
            |v: &u32| Some(v.to_le_bytes()),
            |v: Option<&[u8]>| {
                if let Some(Result::<&[u8; 4], _>::Ok(bytes)) = v.map(|v| v.try_into()) {
                    u32::from_le_bytes(*bytes)
                } else {
                    0u32
                }
            }
        );
    };
    ($name:ident, Option<[u8; $size:literal]>) => {
        config_value!(
            $name,
            Option<[u8; $size]>,
            $size,
            |v: &Option<[u8; $size]>| *v,
            |v: Option<&[u8]>| v.and_then(|v| {
                if let Result::<&[u8; $size], _>::Ok(hash) = v.try_into() {
                    return Some(*hash);
                } else {
                    return None;
                }
            })
        );
    };
    ($name:ident, Option<String>, $max_size:expr) => {
        config_value!(
            $name,
            Option<String>,
            $max_size + 1,
            |v: &Option<String>| {
                let Some(v) = v else {
                    return None;
                };
                let mut target = [0u8; $max_size + 1];
                let bytes = v.as_bytes();
                let mut length = std::cmp::min(bytes.len(), $max_size);
                loop {
                    if v.is_char_boundary(length) {
                        break;
                    }
                    length -= 1;
                }
                target[..length].copy_from_slice(&bytes[..length]);
                target[$max_size] = length as u8;

                Some(target)
            },
            |v: Option<&[u8]>| match v.map(|v| v.try_into()) {
                Some(Result::<&[u8; $max_size + 1], _>::Ok(buffer)) => {
                    let length = buffer[$max_size] as usize;
                    let string = String::from_utf8_lossy(&buffer[..length]).to_string();
                    return Some(string);
                }
                _ => {
                    return None;
                }
            }
        );
    };
    ($name:ident, $type:ty, $size:expr, $to_bytes:expr, $from_bytes:expr) => {
        pub mod $name {
            use crate::config::CONFIG_NVS;
            use std::sync::{LazyLock, RwLock};

            const KEY: &str = stringify!($name);

            pub fn get() -> $type {
                return get_buffer().read().unwrap().clone();
            }

            pub fn set(new_value: &$type) {
                let mut writable_buffer = get_buffer().write().unwrap();
                {
                    let mut nvs = CONFIG_NVS.write().unwrap();

                    let new_bytes = ($to_bytes)(new_value);
                    match new_bytes {
                        Some(new_bytes) => nvs.set_blob(KEY, new_bytes.as_ref()).unwrap(),
                        None => {
                            nvs.remove(KEY).unwrap();
                        }
                    }
                }
                *writable_buffer = new_value.to_owned();
            }

            fn get_buffer() -> &'static LazyLock<RwLock<$type>> {
                static VALUE: LazyLock<RwLock<$type>> = LazyLock::new(|| {
                    let nvs = CONFIG_NVS.read().unwrap();

                    let mut buffer = [0u8; $size];
                    if let Ok(blob_option) = nvs.get_blob(KEY, &mut buffer) {
                        return RwLock::new(($from_bytes)(blob_option));
                    } else {
                        // TODO: Print warning or something
                        ::tracing::warn!("Failed to read config value {}", KEY);
                        return RwLock::new(($from_bytes)(None));
                    }
                });
                &VALUE
            }
        }
    };
}

config_value!(failure_flag, bool);
config_value!(failure_counter, u32);
config_value!(main_program, Option<[u8; 32]>);
config_value!(device_name, Option<String>, 8);
config_value!(mac_address, Option<[u8; 6]>);
