use crate::file_upload_client::UpdateTargetError;
use bluer::{
    gatt::remote::{Characteristic, Service},
    Device,
};
use std::time::Duration;
use thiserror::Error;
use tokio::time::sleep;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum FindServiceError {
    #[error("BlueR error")]
    BluerError(#[from] bluer::Error),
    #[error("Does not contain the requested service")]
    NoUpdateService,
}

pub async fn find_service(device: &Device, uuid: uuid::Uuid) -> Result<Service, FindServiceError> {
    for service in device.services().await? {
        if service.uuid().await? == uuid {
            return Ok(service);
        }
    }

    return Err(FindServiceError::NoUpdateService);
}

#[derive(Error, Debug)]
pub enum FindCharacteristicError {
    #[error("BlueR error")]
    BluerError(#[from] bluer::Error),
    #[error("Does not contain the specified characteristic")]
    NotFound,
}

pub async fn find_characteristic(
    service: &Service,
    uuid: Uuid,
) -> Result<Characteristic, FindCharacteristicError> {
    for characteristic in service.characteristics().await? {
        if characteristic.uuid().await? == uuid {
            return Ok(characteristic);
        }
    }

    return Err(FindCharacteristicError::NotFound);
}

pub async fn connect_to_device(device: &Device) -> Result<(), UpdateTargetError> {
    let rssi = device.rssi().await?;
    let max_attempts = if rssi.is_some() { 3 } else { 1 };
    if device.is_connected().await? {
        return Ok(());
    }
    log::debug!("Connecting...");
    // Prefer a plain GATT connect for LE devices
    for attempt in 0..=max_attempts {
        match device.connect().await {
            Ok(()) => break,
            Err(err) => {
                log::info!("Connect error {}/{}: {}", attempt, max_attempts, &err);
                if attempt < max_attempts {
                    sleep(Duration::from_millis(700)).await;
                    continue;
                }
                // As a fallback, try pairing once if still not connected
                if !device.is_connected().await.unwrap_or(false) {
                    if let Err(perr) = device.pair().await {
                        return Err(UpdateTargetError::FailedToConnect(perr));
                    }
                }
                break;
            }
        }
    }
    Ok(())
}
