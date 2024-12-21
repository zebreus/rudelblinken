use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, EspNvsPartition, NvsDefault};
use std::sync::{LazyLock, RwLock};

pub mod device_name;

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
