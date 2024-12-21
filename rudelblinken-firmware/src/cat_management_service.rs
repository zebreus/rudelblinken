use crate::{
    file_upload_service::{self, FileUploadService},
    service_helpers::DocumentableCharacteristic,
    storage::FlashStorage,
    wasm_service::wasm_host::WasmHost,
};
use device_name::{get_device_name, set_device_name};
use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    BLEAdvertisementData, BLEAdvertising, BLEDevice, BLEServer, NimbleProperties,
};
use esp_idf_hal::{
    gpio::{self, PinDriver},
    ledc::LedcDriver,
};
use esp_idf_sys::{self as _, BLE_GATT_CHR_UNIT_UNITLESS};
use rudelblinken_filesystem::file::{File, FileState};
use std::{
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};
use tracing::{debug, error, info, instrument, Level};
use wasmi::{AsContext, Caller, Engine, Linker, Module, Store};

pub mod device_name;

const CAT_MANAGEMENT_SERVICE: u16 = 0x7992;
const CAT_MANAGEMENT_SERVICE_PROGRAM_HASH: u16 = 0x7893;
const CAT_MANAGEMENT_SERVICE_NAME: u16 = 0x7894;

const CAT_MANAGEMENT_SERVICE_UUID: BleUuid = BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE);
const CAT_MANAGEMENT_SERVICE_PROGRAM_HASH_UUID: BleUuid =
    BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE_PROGRAM_HASH);
const CAT_MANAGEMENT_SERVICE_NAME_UUID: BleUuid = BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE_NAME);

pub struct CatManagementService {
    program_hash: Option<[u8; 32]>,
    pub wasm_runner: mpsc::Sender<File<FlashStorage, { FileState::Reader }>>,
    file_upload_service: Arc<Mutex<FileUploadService>>,
}

// pub enum WasmHostMessage {
//     StartModule([u8; 32]),
//     BLEAdvRecv(BLEAdvNotification),
// }

const WASM_MOD: &[u8] = include_bytes!(
    "../../rudelblinken-wasm/target/wasm32-unknown-unknown/release/rudelblinken_wasm.wasm"
);

fn log_heap_stats() {
    info!(
        free_heap = unsafe { esp_idf_sys::esp_get_free_heap_size() },
        largest_block = unsafe {
            esp_idf_sys::heap_caps_get_largest_free_block(
                esp_idf_sys::MALLOC_CAP_DMA
                    | esp_idf_sys::MALLOC_CAP_32BIT
                    | esp_idf_sys::MALLOC_CAP_DEFAULT,
            )
        },
        "heap stats",
    )
}

fn wasm_runner(
    mut host: WasmHost,
    receiver: mpsc::Receiver<File<FlashStorage, { FileState::Reader }>>,
) {
    loop {
        std::thread::sleep(Duration::from_millis(200));

        let Ok(file) = receiver.try_recv() else {
            continue;
        };

        info!("before creating and linking instance");
        log_heap_stats();
        let wasm: &[u8] = file.as_ref();
        let mut instance = rudelblinken_runtime::linker::setup(&file, host.clone()).unwrap();

        info!("after creating and linking instance");
        log_heap_stats();

        let result = instance.run().unwrap();
        info!("Finished wasm execution")
    }
}

impl CatManagementService {
    pub fn new(
        ble_device: &'static BLEDevice,
        files: Arc<Mutex<FileUploadService>>,
        host: WasmHost,
    ) -> Arc<Mutex<CatManagementService>> {
        let wasm_send = {
            let (send, recv) = mpsc::channel::<File<FlashStorage, { FileState::Reader }>>();

            // let files = files.clone();
            // let name = name.clone();

            std::thread::Builder::new()
                .name("wasm-runner".to_owned())
                .stack_size(0x2000)
                .spawn(move || {
                    wasm_runner(host, recv);
                })
                .expect("failed to spawn wasm runner thread");

            send
        };

        let cat_management_service = Arc::new(Mutex::new(CatManagementService {
            program_hash: None,
            wasm_runner: wasm_send,
            file_upload_service: files,
        }));

        let service = ble_device
            .get_server()
            .create_service(CAT_MANAGEMENT_SERVICE_UUID);

        let program_hash_characteristic = service.lock().create_characteristic(
            CAT_MANAGEMENT_SERVICE_PROGRAM_HASH_UUID,
            NimbleProperties::WRITE | NimbleProperties::READ,
        );
        program_hash_characteristic.document(
            "Current program hash",
            esp32_nimble::BLE2904Format::UTF8,
            0,
            BLE_GATT_CHR_UNIT_UNITLESS,
        );

        let name_characteristic = service.lock().create_characteristic(
            CAT_MANAGEMENT_SERVICE_NAME_UUID,
            NimbleProperties::WRITE | NimbleProperties::READ,
        );
        name_characteristic.document(
            "Name",
            esp32_nimble::BLE2904Format::UTF8,
            0,
            BLE_GATT_CHR_UNIT_UNITLESS,
        );

        let cat_management_service_clone = cat_management_service.clone();
        program_hash_characteristic.lock().on_read(move |value, _| {
            let service = cat_management_service_clone.lock();
            let hash = service.program_hash.unwrap_or([0; 32]);
            value.set_value(&hash);
        });
        let cat_management_service_clone = cat_management_service.clone();
        program_hash_characteristic.lock().on_write(move |args| {
            let mut service = cat_management_service_clone.lock();
            let Ok(hash): Result<[u8; 32], _> = args.recv_data().try_into() else {
                error!("Wrong hash length");
                return;
            };

            service.program_hash = Some(hash);
            let file_upload_service = service.file_upload_service.lock();
            let file = file_upload_service
                .get_file(&hash)
                .expect("failed to get file");
            let content = file.content.upgrade().unwrap();

            service
                .wasm_runner
                .send(content)
                .expect("failed to send new wasm module to runner");
        });

        name_characteristic.lock().on_read(move |value, _| {
            // let service = cat_management_service_clone.lock();
            // let hash = service.name.as_bytes();
            // value.set_value(hash);
            value.set_value(get_device_name().as_bytes());
        });
        name_characteristic.lock().on_write(move |args| {
            let data = args.recv_data();
            if data.len() <= 3 {
                error!("Name too short");
                return;
            }
            if data.len() > 16 {
                error!("Name too long");
                return;
            }

            let Ok(new_name) = String::from_utf8(data.into()) else {
                error!("Name not UTF 8");
                return;
            };

            set_device_name(&new_name);
        });

        cat_management_service
    }
}

// struct HostState {
//     files: Arc<Mutex<FileUploadService>>,
//     name: String,
//     start: Instant,
//     recv: mpsc::Receiver<WasmHostMessage>,
//     next_wasm: Option<[u8; 32]>,
//     ble_device: &'static BLEDevice,
//     // led_driver: Mutex<LedcDriver<'static>>,
//     led_pin: Mutex<PinDriver<'static, gpio::Gpio8, gpio::Output>>,
//     callback_inhibited: bool,
// }

// impl HostState {
//     fn handle_msg(host: &mut Host<Caller<'_, HostState>>, msg: WasmHostMessage) -> bool {
//         match msg {
//             WasmHostMessage::StartModule(hash) => {
//                 host.state_mut().next_wasm = Some(hash);
//                 return false;
//             }
//             WasmHostMessage::BLEAdvRecv(msg) => {
//                 host.on_ble_adv_recv(&msg)
//                     .expect("failed to trigger ble adv callback");
//             }
//         }

//         true
//     }
// }

// impl HostBase for HostState {
//     #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_base::host_log")]
//     fn host_log(&mut self, log: common::Log) {
//         match log.level {
//             common::LogLevel::Error => ::tracing::error!(msg = &log.message, "guest logged"),
//             common::LogLevel::Warn => ::tracing::warn!(msg = &log.message, "guest logged"),
//             common::LogLevel::Info => ::tracing::info!(msg = &log.message, "guest logged"),
//             common::LogLevel::Debug => ::tracing::debug!(msg = &log.message, "guest logged"),
//             common::LogLevel::Trace => ::tracing::trace!(msg = &log.message, "guest logged"),
//         }
//     }

//     #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_base::get_name")]
//     fn get_name(&mut self) -> String {
//         self.name.clone()
//     }

//     #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_base::get_time_millis")]
//     fn get_time_millis(&mut self) -> u32 {
//         self.start.elapsed().as_millis() as u32
//     }

//     #[instrument(level = Level::TRACE, skip(host), target = "cms::wasm::host_base::on_yield")]
//     fn on_yield(host: &mut Host<Caller<'_, Self>>, timeout: u32) -> bool {
//         if host.state().callback_inhibited {
//             // FIXME(lmv): don't block termination if yiedling during a callback
//             debug!("on_yield inhibited");
//             return true;
//         }
//         host.state_mut().callback_inhibited = true;
//         if 0 < timeout {
//             let mut now = Instant::now();
//             let deadline = now + Duration::from_micros(timeout as u64);
//             while now < deadline {
//                 match host.state_mut().recv.recv_timeout(deadline - now) {
//                     Ok(msg) => {
//                         if !Self::handle_msg(host, msg) {
//                             return false;
//                         }
//                     }
//                     Err(mpsc::RecvTimeoutError::Timeout) => break,
//                     Err(err) => {
//                         error!(err = ?err, "recv on_yield failed");
//                         break;
//                     }
//                 }
//                 now = Instant::now();
//             }
//         }

//         loop {
//             match host.state_mut().recv.try_recv() {
//                 Ok(msg) => {
//                     if !Self::handle_msg(host, msg) {
//                         return false;
//                     }
//                 }
//                 Err(mpsc::TryRecvError::Empty) => break,
//                 Err(err) => {
//                     error!(err = ?err, "recv on_yield failed");
//                     break;
//                 }
//             }
//         }

//         host.state_mut().callback_inhibited = false;

//         true
//     }
// }

// impl LEDBrightness for HostState {
//     #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_led::set_led_brightness")]
//     fn set_led_brightness(&mut self, settings: common::LEDBrightnessSettings) {
//         let b = settings.rgb[0] as u16 + settings.rgb[1] as u16 + settings.rgb[2] as u16;
//         /* let mut led_pin = self.led_driver.lock();
//         let duty = (led_pin.get_max_duty() as u64) * (b as u64) / (3 * 255);
//         if let Err(err) = led_pin.set_duty(duty as u32) {
//             error!(duty = duty, err = ?err "set_duty failed")
//         }; */
//         if b < 2 * 256 {
//             info!("guest set led bightness low");
//             self.led_pin.lock().set_low().unwrap();
//         } else {
//             info!("guest set led bightness high");
//             self.led_pin.lock().set_high().unwrap();
//         }
//     }
// }

// impl BLEAdv for HostState {
//     #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_ble::configure_ble_adv")]
//     fn configure_ble_adv(&mut self, settings: common::BLEAdvSettings) {
//         let min_interval = settings.min_interval.clamp(400, 1000);
//         let max_interval = settings.min_interval.clamp(min_interval, 1500);
//         self.ble_device
//             .get_advertising()
//             .lock()
//             .min_interval(min_interval)
//             .max_interval(max_interval);
//     }

//     #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_ble::configure_ble_data")]
//     fn configure_ble_data(&mut self, data: common::BLEAdvData) {
//         if let Err(err) = self.ble_device.get_advertising().lock().set_data(
//             BLEAdvertisementData::new()
//                 .name(&self.name)
//                 .manufacturer_data(&data.data),
//         ) {
//             error!("set manufacturer data ({:?}) failed: {:?}", data.data, err)
//         };
//     }
// }
