use btleplug::api::{
    bleuuid::uuid_from_u16, Central, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::api::{BDAddr, Service};
use btleplug::platform::{Adapter, Manager, Peripheral};
use rand::{thread_rng, Rng};
use std::error::Error;
use std::thread;
use std::time::Duration;
use thiserror::Error;
use tokio::task::JoinSet;
use tokio::time;
use uuid::Uuid;

const LIGHT_CHARACTERISTIC_UUID: Uuid = uuid_from_u16(0xFFE9);

const UPDATE_SERVICE_UUID: Uuid = uuid_from_u16(29342);
const UPDATE_SERVICE_RECEIVE_DATA_UUID: Uuid = uuid_from_u16(13443);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let manager = Manager::new().await.unwrap();

    // get the first bluetooth adapter
    let adapters = manager.adapters().await?;
    let central = adapters.into_iter().nth(0).unwrap();
    // start scanning for devices
    central.start_scan(ScanFilter::default()).await?;
    // instead of waiting, you can use central.events() to get a stream which will
    // notify you of new devices, for an example of that see examples/event_driven_discovery.rs
    time::sleep(Duration::from_secs(2)).await;

    // find the device we're interested in
    let targets = find_update_targets(&central).await?;

    for target in targets {
        let properties = target.peripheral.properties().await.unwrap().unwrap();
        let name = properties.local_name.unwrap();
        println!("{} is providing the update service", name);
    }

    // connect to the device
    // light.connect().await?;

    // discover services and characteristics
    // light.discover_services().await?;

    // // find the characteristic we want
    // let chars = light.characteristics();
    // let cmd_char = chars
    //     .iter()
    //     .find(|c| c.uuid == LIGHT_CHARACTERISTIC_UUID)
    //     .unwrap();

    // dance party
    // let mut rng = thread_rng();
    // for _ in 0..20 {
    //     let color_cmd = vec![0x56, rng.gen(), rng.gen(), rng.gen(), 0x00, 0xF0, 0xAA];
    //     light
    //         .write(&cmd_char, &color_cmd, WriteType::WithoutResponse)
    //         .await?;
    //     time::sleep(Duration::from_millis(200)).await;
    // }
    Ok(())
}

#[derive(Error, Debug)]
pub enum UpdateTargetError {
    #[error("btleplug error")]
    BtleplugError(#[from] btleplug::Error),
    #[error("Not an update target")]
    NotAnUpdateTarget,
    #[error("Not an update target")]
    MacDoesNotLookLikeAnUpdateTarget,
    // TODO: Write better message
    #[error("Something weird happened")]
    WeirdError,
}

struct UpdateTarget {
    pub peripheral: Peripheral,
    service: Service,
}

impl UpdateTarget {
    async fn new_from_peripheral(
        peripheral: Peripheral,
    ) -> Result<UpdateTarget, UpdateTargetError> {
        let mac_address = peripheral.address().to_string().to_ascii_uppercase();
        if !(peripheral.address().is_random_static() || mac_address.starts_with("24:EC:4A")) {
            return Err(UpdateTargetError::MacDoesNotLookLikeAnUpdateTarget);
        }

        // // Sometimes this is required to actually discover services
        peripheral.connect().await?;
        peripheral
            .properties()
            .await?
            .ok_or(UpdateTargetError::WeirdError)?;
        peripheral.discover_services().await?;

        println!("Checking {}", mac_address);

        // Make sure that the peripheral provides the update service
        let update_service = peripheral
            .services()
            .into_iter()
            .find(|service| {
                // println!("{:?}", service);
                service.uuid == UPDATE_SERVICE_UUID
            })
            .ok_or(UpdateTargetError::NotAnUpdateTarget)?;

        println!("Checked {}", mac_address);

        return Ok(UpdateTarget {
            peripheral,
            service: update_service,
        });
    }

    fn program(&mut self, code: &[u8]) {}
}

// async fn get_update_service(target: &Peripheral) -> Result<&Service, UpdateTargetError> {

// }

async fn find_update_targets(central: &Adapter) -> Result<Vec<UpdateTarget>, UpdateTargetError> {
    let mut set = JoinSet::new();
    // Discover services
    for peripheral in central.peripherals().await? {
        set.spawn(async move {
            return UpdateTarget::new_from_peripheral(peripheral).await;
        });
    }
    let peripherals = set.join_all().await;
    let update_targets: Vec<_> = peripherals
        .into_iter()
        .filter_map(|update_target| {
            return update_target.ok();
        })
        .collect();

    println!("Something something {}", update_targets.len());

    return Ok(update_targets);
}
