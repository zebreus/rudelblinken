//! Generated BLE types

/// A service UUID, can be a shortened one
#[derive(Clone, Copy)]
pub enum ServiceUuid {
    Uuid16(u16),
    Uuid32(u32),
    Uuid128((u64, u64)),
}
impl ::core::fmt::Debug for ServiceUuid {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        match self {
            ServiceUuid::Uuid16(e) => f.debug_tuple("ServiceUuid::Uuid16").field(e).finish(),
            ServiceUuid::Uuid32(e) => f.debug_tuple("ServiceUuid::Uuid32").field(e).finish(),
            ServiceUuid::Uuid128(e) => f.debug_tuple("ServiceUuid::Uuid128").field(e).finish(),
        }
    }
}

impl ServiceUuid {
    pub fn lower_into(&self, target: &mut [u8; 24]) {}
}

/// Service specific data
#[derive(Clone)]
pub struct ServiceData {
    pub uuid: ServiceUuid,
    pub data: Vec<u8>,
}
impl ::core::fmt::Debug for ServiceData {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_struct("ServiceData")
            .field("uuid", &self.uuid)
            .field("data", &self.data)
            .finish()
    }
}
/// Manufacturer specific data
#[derive(Clone)]
pub struct ManufacturerData {
    pub company_id: u16,
    pub data: Vec<u8>,
}
impl ::core::fmt::Debug for ManufacturerData {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_struct("ManufacturerData")
            .field("company-id", &self.company_id)
            .field("data", &self.data)
            .finish()
    }
}
/// Configure the BLE advertisements
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AdvertisementInterval {
    pub min_interval: u16,
    pub max_interval: u16,
}
impl ::core::fmt::Debug for AdvertisementInterval {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_struct("AdvertisementInterval")
            .field("min-interval", &self.min_interval)
            .field("max-interval", &self.max_interval)
            .finish()
    }
}
/// Sendable BLE advertisement data
///
/// Can be at most 31 bytes - (size of the name + 6 bytes)
#[derive(Clone)]
pub struct EncodedData {
    /// Include the transmission power. (3 bytes)
    pub include_tx_power: bool,
    /// service UUIDs (2 bytes per used class (16, 32, 128 bit UUID) + size of the UUIDs)
    pub uuids: Vec<ServiceUuid>,
    /// service data (2 byte + size of the UUID + size of data) for each service data)
    pub service_data: Vec<ServiceData>,
    /// appearance (4 byte)
    pub appearance: Option<u16>,
    /// manufacturer specific data (2 byte + 2 byte company ID + size of data)
    pub manufacturer_data: Option<ManufacturerData>,
}
impl ::core::fmt::Debug for EncodedData {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_struct("EncodedData")
            .field("include-tx-power", &self.include_tx_power)
            .field("uuids", &self.uuids)
            .field("service-data", &self.service_data)
            .field("appearance", &self.appearance)
            .field("manufacturer-data", &self.manufacturer_data)
            .finish()
    }
}

::bitflags::bitflags! {
    #[doc = " TODO: Check order"] #[derive(PartialEq, Eq, PartialOrd, Ord,
    Hash, Debug, Clone, Copy)] pub struct AdvertisementFlags : u8 { #[doc =
    " LE Limited Discoverable Mode"] const LIMITED_DISCOVERABLE = 1 << 0;
    #[doc = " LE General Discoverable Mode"] const GENERAL_DISCOVERABLE = 1
    << 1; #[doc = " BR/EDR Not Supported"] const BR_EDR_NOT_SUPPORTED = 1 <<
    2; #[doc =
    " Simultaneous LE and BR/EDR to Same Device Capable (Controller)"] const
    SIMULTANEOUS_CONTROLLER = 1 << 3; #[doc =
    " Simultaneous LE and BR/EDR to Same Device Capable (Host)"] const
    SIMULTANEOUS_HOST = 1 << 4; }
}
#[repr(u8)]
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub enum AdvertisementType {
    /// indirect advertising
    Indirect,
    /// direct advertising
    DirectInd,
    /// indirect scan response
    IndirectScan,
    /// indirect advertising - not connectable
    IndirectNotConnectable,
    /// scan responst
    ScanResponse,
}
impl ::core::fmt::Debug for AdvertisementType {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        match self {
            AdvertisementType::Indirect => f.debug_tuple("AdvertisementType::Indirect").finish(),
            AdvertisementType::DirectInd => f.debug_tuple("AdvertisementType::DirectInd").finish(),
            AdvertisementType::IndirectScan => {
                f.debug_tuple("AdvertisementType::IndirectScan").finish()
            }
            AdvertisementType::IndirectNotConnectable => f
                .debug_tuple("AdvertisementType::IndirectNotConnectable")
                .finish(),
            AdvertisementType::ScanResponse => {
                f.debug_tuple("AdvertisementType::ScanResponse").finish()
            }
        }
    }
}
impl AdvertisementType {
    #[doc(hidden)]
    pub unsafe fn _lift(val: u8) -> AdvertisementType {
        if !cfg!(debug_assertions) {
            return ::core::mem::transmute(val);
        }
        match val {
            0 => AdvertisementType::Indirect,
            1 => AdvertisementType::DirectInd,
            2 => AdvertisementType::IndirectScan,
            3 => AdvertisementType::IndirectNotConnectable,
            4 => AdvertisementType::ScanResponse,
            _ => panic!("invalid enum discriminant"),
        }
    }
}
/// Decoded BLE advertisement
#[derive(Clone)]
pub struct DecodedData {
    /// name of the remote device
    pub name: Option<String>,
    /// flags
    pub advertisement_flags: AdvertisementFlags,
    /// tx power
    pub tx_power: Option<u8>,
    /// service UUIDs
    pub uuids: Vec<ServiceUuid>,
    /// service data
    pub service_data: Vec<ServiceData>,
    /// appearance
    pub appearance: Option<u16>,
    /// manufacturer specific data
    pub manufacturer_data: Option<ManufacturerData>,
}
impl ::core::fmt::Debug for DecodedData {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_struct("DecodedData")
            .field("name", &self.name)
            .field("advertisement-flags", &self.advertisement_flags)
            .field("tx-power", &self.tx_power)
            .field("uuids", &self.uuids)
            .field("service-data", &self.service_data)
            .field("appearance", &self.appearance)
            .field("manufacturer-data", &self.manufacturer_data)
            .finish()
    }
}
#[derive(Clone)]
pub enum AdvertisementData {
    /// Decoded advertisement data
    Decoded(DecodedData),
    /// Raw advertisement data. Returned if there were some fields that failed decoding
    Raw(Vec<u8>),
}
impl ::core::fmt::Debug for AdvertisementData {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        match self {
            AdvertisementData::Decoded(e) => f
                .debug_tuple("AdvertisementData::Decoded")
                .field(e)
                .finish(),
            AdvertisementData::Raw(e) => f.debug_tuple("AdvertisementData::Raw").field(e).finish(),
        }
    }
}
/// Decoded BLE advertisement
#[derive(Clone)]
pub struct Advertisement {
    /// When the advertisement was received
    /// There may be some delay between when the advertisement was received and when the WASM guest is notified
    pub received_at: u64,
    /// The address of the sender 48bit integer
    pub address: u64,
    /// Received signal strength
    pub rssi: i8,
    /// Received advertisement type
    pub advertisement_type: AdvertisementType,
    /// Received data
    /// Will be decoded if it can be decoded
    pub data: AdvertisementData,
}
impl ::core::fmt::Debug for Advertisement {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_struct("Advertisement")
            .field("received-at", &self.received_at)
            .field("address", &self.address)
            .field("rssi", &self.rssi)
            .field("advertisement-type", &self.advertisement_type)
            .field("data", &self.data)
            .finish()
    }
}
/// A ble event
/// For now only advertisements
#[derive(Clone)]
pub enum BleEvent {
    Advertisement(Advertisement),
}
impl ::core::fmt::Debug for BleEvent {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        match self {
            BleEvent::Advertisement(e) => {
                f.debug_tuple("BleEvent::Advertisement").field(e).finish()
            }
        }
    }
}
