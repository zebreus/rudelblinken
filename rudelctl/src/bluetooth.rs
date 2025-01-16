use bluer::Device;
use futures::{
    pin_mut,
    stream::{AbortHandle, Abortable},
    StreamExt as STTT,
};
use futures_time::stream::StreamExt;
use futures_time::time::Duration;
use std::future::Future;

// pub enum BluetoothError {
//     BluerError(bluer::Error),
// }

// pub enum Device {
//     Ble { device: bluer::Device },
//     Simulated { address: Address },
// }

// impl Device {
//     pub fn address(&self) -> Result<Address, BluetoothError> {
//         match self {
//             Device::Ble { device } => device.address().into(),
//             Device::Simulated { address } => address.clone(),
//         }
//     }
//     pub async fn is_connected(&self) -> Result<bool, BluetoothError> {
//         match self {
//             Device::Ble { device } => device.is_connected().await?,
//             Device::Simulated { .. } => true,
//         }
//     }
// }

// /// Bluetooth address. Copied from bluer.
// ///
// /// The serialized representation is a string in colon-hexadecimal notation.
// #[derive(Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
// pub struct Address(pub [u8; 6]);
// impl Address {
//     /// Creates a new Bluetooth address with the specified value.
//     pub const fn new(addr: [u8; 6]) -> Self {
//         Self(addr)
//     }

//     /// Any Bluetooth address.
//     ///
//     /// Corresponds to `00:00:00:00:00:00`.
//     pub const fn any() -> Self {
//         Self([0; 6])
//     }
// }
// impl Deref for Address {
//     type Target = [u8; 6];

//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }
// impl DerefMut for Address {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.0
//     }
// }
// impl Display for Address {
//     fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
//         write!(
//             f,
//             "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
//             self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
//         )
//     }
// }
// impl Debug for Address {
//     fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
//         write!(f, "{self}")
//     }
// }
// impl From<bluer::Address> for Address {
//     fn from(addr: bluer::Address) -> Self {
//         Self(addr.0)
//     }
// }
// impl Into<bluer::Address> for Address {
//     fn into(self) -> bluer::Address {
//         bluer::Address(self.0)
//     }
// }

pub async fn scan_for<Fut, Err>(
    duration: Duration,
    max_devices: u32,
    f: &dyn Fn(Device) -> Fut,
) -> bluer::Result<()>
where
    Err: std::fmt::Debug,
    Fut: Future<Output = Result<(), Err>>,
{
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    {
        // eprintln!(
        //     "Discovering on Bluetooth adapter {} with address {}\n",
        //     adapter.name(),
        //     adapter.address().await?
        // );
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        let discover = adapter.discover_devices().await?;
        pin_mut!(discover);
        let stream = Abortable::new(discover, abort_registration);
        let mut stream = stream.timeout(duration);
        let mut programmed_devices = 0;
        while let Some(evt) = stream.next().await {
            let Ok(evt) = evt else {
                break;
            };
            match evt {
                bluer::AdapterEvent::DeviceAdded(addr) => {
                    let device = adapter.device(addr)?;
                    // let wrapped_device = Device::Ble { device: device };
                    // let result = f(wrapped_device).await;
                    let result = f(device).await;
                    if result.is_ok() {
                        programmed_devices += 1;
                    }
                    println!("Got result {:?}", result);
                    if programmed_devices >= max_devices {
                        abort_handle.abort();
                    }
                }
                // AdapterEvent::DeviceRemoved(addr) => {
                //     // println!("Device removed {addr}");
                // }
                _ => (),
            }
        }
    }

    return Ok(());
}
