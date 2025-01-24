use super::{
    BleService, BleUuidTrait, CharacteristicType, ConnectionTrait, OnReadArgsTrait,
    OnWriteArgsTrait,
};

#[repr(transparent)]
pub struct NimbleOnWriteArgs<'a> {
    args: esp32_nimble::OnWriteArgs<'a>,
}

impl OnWriteArgsTrait<'_, NimbleConnection> for NimbleOnWriteArgs<'_> {
    fn old_value(&mut self) -> &[u8] {
        todo!();
    }

    fn new_value(&mut self) -> &[u8] {
        todo!();
    }

    fn conn_desc(&self) -> &NimbleConnection {
        todo!();
    }
}

pub struct NimbleOnReadArgs<'a> {
    value: &'a mut Vec<u8>,
    desc: &'a NimbleConnection,
}

impl<'a> OnReadArgsTrait<'a, NimbleConnection> for NimbleOnReadArgs<'a> {
    fn set_value(&mut self, value: &[u8]) {
        // TODO: Improve
        self.value.clear();
        self.value.extend_from_slice(value);
    }

    fn conn_desc(&self) -> &NimbleConnection {
        self.desc
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NimbleBleUuid {
    uuid: esp32_nimble::utilities::BleUuid,
}

impl BleUuidTrait for NimbleBleUuid {
    fn from_uuid16(uuid: u16) -> Self {
        Self {
            uuid: esp32_nimble::utilities::BleUuid::Uuid16(uuid),
        }
    }

    fn from_uuid32(uuid: u32) -> Self {
        Self {
            uuid: esp32_nimble::utilities::BleUuid::Uuid32(uuid),
        }
    }

    fn from_uuid128(uuid: [u8; 16]) -> Self {
        Self {
            uuid: esp32_nimble::utilities::BleUuid::Uuid128(uuid),
        }
    }
}

pub struct NimbleConnection {
    connection: esp32_nimble::BLEConnDesc,
}

impl ConnectionTrait for NimbleConnection {}

pub struct NimbleService {
    service: std::sync::Arc<esp32_nimble::utilities::mutex::Mutex<esp32_nimble::BLEService>>,
}

impl NimbleService {
    fn new(server: &mut esp32_nimble::BLEServer, uuid: NimbleBleUuid) -> Self {
        let service = server.create_service(uuid.uuid);
        Self { service }
    }
}

impl<'a>
    BleService<'a, NimbleBleUuid, NimbleOnReadArgs<'a>, NimbleOnWriteArgs<'a>, NimbleConnection>
    for NimbleService
{
    fn new(server: &mut esp32_nimble::BLEServer, uuid: NimbleBleUuid) -> Self {
        NimbleService::new(server, uuid)
    }
    fn create_characteristic_r(
        &self,
        uuid: NimbleBleUuid,
        name: &str,
        format: CharacteristicType,
        exponent: u8,
        unit: u32,
        read: impl FnMut(NimbleOnReadArgs<'a>) + Send + Sync + 'static,
    ) {
        todo!();
    }
    fn create_characteristic_w(
        &self,
        uuid: NimbleBleUuid,
        name: &str,
        format: CharacteristicType,
        exponent: u8,
        unit: u32,
        write: impl FnMut(NimbleOnWriteArgs<'a>) + Send + Sync + 'static,
    ) {
        todo!();
    }
    fn create_characteristic_rw(
        &self,
        uuid: NimbleBleUuid,
        name: &str,
        format: CharacteristicType,
        exponent: u8,
        unit: u32,
        read: impl FnMut(NimbleOnReadArgs<'a>) + Send + Sync + 'static,
        write: impl FnMut(NimbleOnWriteArgs<'a>) + Send + Sync + 'static,
    ) {
        todo!();
    }
}
// Compare this snippet from rudelblinken-firmware/src/ble_abstraction/nimble.rs:
// use super::{BleService, BleUuidTrait, ConnectionTrait, OnReadArgsTrait};
//
// #[repr(transparent)]
// pub struct NimbleOnWriteArgs<'a> {
//     args: esp32_nimble::OnWriteArgs<'a>,
// }
//
// pub struct NimbleOnReadArgs<'a> {
//     value: &'a mut Vec<u8>,
//     desc: &'a NimbleConnection,
// }
//
// impl OnReadArgsTrait<NimbleConnection> for NimbleOnReadArgs<'_> {
//     fn set_value(&mut self, value: &[u8]) {
