use std::sync::Arc;

use bitflags::bitflags;
use esp32_nimble::BLEServer;
use esp_idf_sys as _;
mod nimble;

#[repr(u8)]
pub enum CharacteristicType {
    BOOLEAN = 1,
    UINT2 = 2,
    UINT4 = 3,
    UINT8 = 4,
    UINT12 = 5,
    UINT16 = 6,
    UINT24 = 7,
    UINT32 = 8,
    UINT48 = 9,
    UINT64 = 10,
    UINT128 = 11,
    SINT8 = 12,
    SINT12 = 13,
    SINT16 = 14,
    SINT24 = 15,
    SINT32 = 16,
    SINT48 = 17,
    SINT64 = 18,
    SINT128 = 19,
    FLOAT32 = 20,
    FLOAT64 = 21,
    SFLOAT16 = 22,
    SFLOAT32 = 23,
    IEEE20601 = 24,
    UTF8 = 25,
    UTF16 = 26,
    OPAQUE = 27,
}

pub trait BleUuidTrait
where
    Self: Sized + Clone + Copy + PartialEq + core::fmt::Debug,
{
    /// Creates a new [`BleUuid`] from a 16-bit integer.
    #[must_use]
    fn from_uuid16(uuid: u16) -> Self;

    /// Creates a new [`BleUuid`] from a 32-bit integer.
    #[must_use]
    fn from_uuid32(uuid: u32) -> Self;

    /// Creates a new [`BleUuid`] from a 16 byte array.
    #[must_use]
    fn from_uuid128(uuid: [u8; 16]) -> Self;
}

pub trait OnReadArgsTrait<'a, Connection>
where
    Self: Sized,
    Connection: ConnectionTrait,
{
    fn set_value(&mut self, value: &[u8]);
    fn conn_desc(&self) -> &Connection;
}

pub trait OnWriteArgsTrait<'a, Connection>
where
    Self: Sized,
    Connection: ConnectionTrait,
{
    fn old_value(&mut self) -> &[u8];
    fn new_value(&mut self) -> &[u8];
    fn conn_desc(&self) -> &Connection;
}

pub trait ConnectionTrait
where
    Self: Sized,
{
}

pub trait BleService<'a, BleUuid, OnReadArgs, OnWriteArgs, Connection>
where
    Self: Sized,
    BleUuid: BleUuidTrait,
    OnReadArgs: OnReadArgsTrait<'a, Connection>,
    OnWriteArgs: OnWriteArgsTrait<'a, Connection>,
    Connection: ConnectionTrait,
{
    fn new(server: &mut BLEServer, uuid: BleUuid) -> Self;
    fn create_characteristic_r(
        &self,
        uuid: BleUuid,
        name: &str,
        format: CharacteristicType,
        exponent: u8,
        unit: u32,
        read: impl FnMut(OnReadArgs) + Send + Sync + 'static,
    );
    fn create_characteristic_w(
        &self,
        uuid: BleUuid,
        name: &str,
        format: CharacteristicType,
        exponent: u8,
        unit: u32,
        write: impl FnMut(OnWriteArgs) + Send + Sync + 'static,
    );
    fn create_characteristic_rw(
        &self,
        uuid: BleUuid,
        name: &str,
        format: CharacteristicType,
        exponent: u8,
        unit: u32,
        read: impl FnMut(OnReadArgs) + Send + Sync + 'static,
        write: impl FnMut(OnWriteArgs) + Send + Sync + 'static,
    );
}
