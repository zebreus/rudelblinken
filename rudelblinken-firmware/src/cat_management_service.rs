use std::{
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    BLEAdvertisementData, BLEAdvertising, BLEDevice, BLEServer, NimbleProperties,
};
use esp_idf_hal::{
    gpio::{self, PinDriver},
    ledc::LedcDriver,
};
use esp_idf_sys as _;

use rudelblinken_sdk::{
    common::{self, BLEAdvNotification},
    host::{self, BLEAdv, Host, HostBase, InstanceWithContext, LEDBrightness},
};
use tracing::{debug, error, info, instrument, Level};
use wasmi::{AsContext, Caller, Engine, Linker, Module, Store};

use crate::file_upload_service::FileUploadService;

const CAT_MANAGEMENT_SERVICE: u16 = 0x7992;
const CAT_MANAGEMENT_SERVICE_PROGRAM_HASH: u16 = 0x7893;
const CAT_MANAGEMENT_SERVICE_NAME: u16 = 0x7894;

const CAT_MANAGEMENT_SERVICE_UUID: BleUuid = BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE);
const CAT_MANAGEMENT_SERVICE_PROGRAM_HASH_UUID: BleUuid =
    BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE_PROGRAM_HASH);
const CAT_MANAGEMENT_SERVICE_NAME_UUID: BleUuid = BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE_NAME);
// const NAMES: [&str; 256] = [
//     "Camdyn",
//     "Christan",
//     "Kris",
//     "Shaya",
//     "Hartley",
//     "Claudie",
//     "Ashtin",
//     "Krishna",
//     "Terryl",
//     "Marvis",
//     "Riley",
//     "Larkin",
//     "Kodi",
//     "Michal",
//     "Blair",
//     "Lavern",
//     "Ricci",
//     "Lavon",
//     "Emery",
//     "De",
//     "Seneca",
//     "Burnice",
//     "Ocean",
//     "Kendel",
//     "Amari",
//     "Kerry",
//     "Marlowe",
//     "Teegan",
//     "Baby",
//     "Jireh",
//     "Talyn",
//     "Kylin",
//     "Mckinley",
//     "Salem",
//     "Dallis",
//     "Jessie",
//     "Waverly",
//     "Ardell",
//     "Arden",
//     "Carey",
//     "Kamdyn",
//     "Jaziah",
//     "Tam",
//     "Demetrice",
//     "Emerson",
//     "Sonnie",
//     "Cameran",
//     "Kiran",
//     "Kalin",
//     "Devine",
//     "Evyn",
//     "Alva",
//     "Justice",
//     "Lakota",
//     "Tristyn",
//     "Ocie",
//     "Delane",
//     "Kailen",
//     "Vernie",
//     "Isa",
//     "Indiana",
//     "Shade",
//     "Marshell",
//     "Devyn",
//     "Natividad",
//     "Deniz",
//     "Parris",
//     "Dann",
//     "An",
//     "Ashten",
//     "Shai",
//     "Lian",
//     "Milan",
//     "Lorenza",
//     "Britain",
//     "Teddie",
//     "Jaydyn",
//     "Joell",
//     "Lorin",
//     "Micha",
//     "Arlyn",
//     "Ivory",
//     "Tru",
//     "Jae",
//     "Adair",
//     "Carrol",
//     "Kodie",
//     "Linn",
//     "Bao",
//     "Collen",
//     "Arie",
//     "Yael",
//     "Emari",
//     "Sol",
//     "Kimani",
//     "Robbie",
//     "Chi",
//     "Reilly",
//     "Lennie",
//     "Schyler",
//     "Cedar",
//     "Carlin",
//     "Landry",
//     "Sutton",
//     "True",
//     "Tenzin",
//     "Armani",
//     "Ryley",
//     "Amaree",
//     "Santana",
//     "Jaime",
//     "Divine",
//     "Ossie",
//     "Laramie",
//     "Dwan",
//     "Peyton",
//     "Rennie",
//     "Campbell",
//     "Drue",
//     "Jaelin",
//     "Remy",
//     "Allyn",
//     "Aries",
//     "Harley",
//     "Karsen",
//     "Jaylin",
//     "Jourdan",
//     "Cache",
//     "Stevie",
//     "Tylar",
//     "Daylin",
//     "Finley",
//     "Adel",
//     "Elisha",
//     "Kaidyn",
//     "Anay",
//     "Mycah",
//     "Jackie",
//     "Shamari",
//     "Toy",
//     "Verdell",
//     "Kenyatta",
//     "Casey",
//     "Jaedyn",
//     "Clair",
//     "Shia",
//     "Rio",
//     "Shea",
//     "Shay",
//     "Devonne",
//     "Kalani",
//     "Kriston",
//     "Jazz",
//     "Lavaughn",
//     "Rylin",
//     "Carrington",
//     "Jeryl",
//     "Ryen",
//     "Artie",
//     "Merlyn",
//     "Trinidad",
//     "Adi",
//     "Rowan",
//     "Camari",
//     "Rian",
//     "Payson",
//     "Britt",
//     "Tien",
//     "Jaidan",
//     "Taylen",
//     "Aven",
//     "Lin",
//     "Tai",
//     "Kary",
//     "Maxie",
//     "Lajuan",
//     "Rhyan",
//     "Aris",
//     "Ellery",
//     "Lannie",
//     "Dru",
//     "Mikah",
//     "Armoni",
//     "Leighton",
//     "Berlin",
//     "Aaryn",
//     "Ashby",
//     "Lexington",
//     "Codie",
//     "Charley",
//     "Yuri",
//     "Phoenix",
//     "Arin",
//     "Le",
//     "Nazareth",
//     "Finnley",
//     "Clemmie",
//     "Raynell",
//     "Jael",
//     "Mykah",
//     "Earlie",
//     "Barrie",
//     "Alpha",
//     "Onyx",
//     "Micaiah",
//     "Lashaun",
//     "Gentry",
//     "Thanh",
//     "Kaedyn",
//     "Golden",
//     "Frankie",
//     "Lyrik",
//     "Kaylon",
//     "Kareen",
//     "Dominque",
//     "Channing",
//     "Dakotah",
//     "Kendall",
//     "Adrean",
//     "Stephane",
//     "Aly",
//     "Azariah",
//     "Yuki",
//     "Ara",
//     "Marice",
//     "Pat",
//     "Charly",
//     "Samar",
//     "Codi",
//     "Sage",
//     "Kamari",
//     "Sloan",
//     "Braylin",
//     "Vinnie",
//     "Nieves",
//     "Quinn",
//     "Jammie",
//     "Berkeley",
//     "Jimi",
//     "Reese",
//     "Storm",
//     "Osiris",
//     "Oakley",
//     "Tory",
//     "Jule",
//     "Garnett",
//     "Halen",
//     "Deane",
//     "Brittan",
//     "Tobie",
//     "Skyler",
//     "Ellison",
//     "Vernell",
//     "Jody",
//     "Arnell",
//     "Berkley",
// ];

fn get_device_name() -> String {
    return "dirk".into();
    // let name;
    // unsafe {
    //     let mut mac = [0u8; 6];
    //     esp_idf_sys::esp_base_mac_addr_get(mac.as_mut_ptr());
    //     name = format!(
    //         "{}-{}-{}",
    //         NAMES[mac[3] as usize], NAMES[mac[4] as usize], NAMES[mac[5] as usize]
    //     );
    // };
    // name
}

pub struct CatManagementService {
    program_hash: Option<[u8; 32]>,
    name: String,
    pub wasm_runner: mpsc::Sender<WasmHostMessage>,
    file_upload_service: Arc<Mutex<FileUploadService>>,
}

pub enum WasmHostMessage {
    StartModule([u8; 32]),
    BLEAdvRecv(BLEAdvNotification),
}

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

fn wasm_runner(mut state: HostState) {
    'run_loop: loop {
        // let mut limits = StackLimits::default();
        // limits.initial_value_stack_height = 1024;
        let engine = Engine::new(
            wasmi::Config::default()
                .consume_fuel(true)
                // parsing custom sections trigers a ~20kb memory allocation during Module::new))
                .ignore_custom_sections(true), // .compilation_mode(wasmi::CompilationMode::Lazy)
                                               // .enforced_limits(EnforcedLimits::strict())
                                               // .set_stack_limits(limits),
        );

        let mut linker = <Linker<HostState>>::new(&engine);

        while state.next_wasm.is_none() {
            match state.recv.recv() {
                Ok(WasmHostMessage::StartModule(hash)) => state.next_wasm = Some(hash),
                Ok(_) => {}
                Err(err) => error!(err = ?err, "wasm runner failed to recv"),
            }
        }

        let module = {
            // avoid cloning the wasm binary
            /* let files = state.files.lock();
            let wasm_file = match files.get_file(state.next_wasm.as_ref().unwrap()) {
                Some(f) => f,
                None => {
                    drop(files);
                    state.next_wasm = None;
                    continue 'run_loop;
                }
            }; */

            // let wasm_bin = &wasm_file.content;
            let wasm_bin = WASM_MOD;

            info!(size = wasm_bin.len(), "creating new wasm module");
            log_heap_stats();
            match unsafe { Module::new_unchecked(&engine, wasm_bin) } {
                Ok(m) => m,
                Err(err) => {
                    error!(err = ?err, "error during wasm module creation");
                    continue 'run_loop;
                }
            }
        };

        info!("preparing store and linker for wasm runtime");
        log_heap_stats();
        state.start = Instant::now();
        let mut store = Store::new(&engine, state);

        host::helper::prepare_link_host_base(&mut store, &mut linker)
            .expect("failed to link host base");
        host::helper::prepare_link_led_brightness(&mut store, &mut linker)
            .expect("failed to link led brightness");
        host::helper::prepare_link_ble_adv(&mut store, &mut linker)
            .expect("failed to link ble adv");
        host::helper::prepare_link_stubs(&mut store, &mut linker, module.imports())
            .expect("failed to link stubs");

        info!("instantiating wasm module");
        log_heap_stats();
        let pre_instance = linker
            .instantiate(&mut store, &module)
            .expect("failed to instanciate module");
        info!("starting wasm module");
        log_heap_stats();
        let instance = pre_instance
            .start(&mut store)
            .expect("failed to start instance");

        let mut host: Host<_> = InstanceWithContext::new(store, instance).into();

        info!("invoking wasm main");
        log_heap_stats();
        match host.main() {
            Ok(()) => info!("wasm guest exited"),
            Err(err) => info!(err = ?err, "wasm guest failed"),
        }

        state = host.get_runtime_info().context.into_data();
    }
}

impl CatManagementService {
    pub fn new(
        ble_device: &'static BLEDevice,
        files: Arc<Mutex<FileUploadService>>,
        led_pin: Mutex<PinDriver<'static, gpio::Gpio8, gpio::Output>>,
    ) -> Arc<Mutex<CatManagementService>> {
        let name = get_device_name();

        let wasm_send = {
            let (send, recv) = mpsc::channel();

            let files = files.clone();
            let name = name.clone();

            std::thread::Builder::new()
                .name("wasm-runner".to_owned())
                .stack_size(0x2000)
                .spawn(move || {
                    let wasm_host = HostState {
                        files,
                        name,
                        recv,
                        start: Instant::now(),
                        next_wasm: Some([0u8; 32]),
                        ble_device,
                        led_pin,
                        callback_inhibited: false,
                    };

                    wasm_runner(wasm_host);
                })
                .expect("failed to spawn wasm runner thread");

            send
        };

        let cat_management_service = Arc::new(Mutex::new(CatManagementService {
            name,
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

            // service
            //     .wasm_runner
            //     .send(WasmHostMessage::StartModule(hash))
            //     .expect("failed to send new wasm module to runner");
        });

        let name_characteristic = service.lock().create_characteristic(
            CAT_MANAGEMENT_SERVICE_NAME_UUID,
            NimbleProperties::WRITE | NimbleProperties::READ,
        );
        let cat_management_service_clone = cat_management_service.clone();
        name_characteristic.lock().on_read(move |value, _| {
            let service = cat_management_service_clone.lock();
            let hash = service.name.as_bytes();
            value.set_value(hash);
        });
        let cat_management_service_clone = cat_management_service.clone();
        name_characteristic.lock().on_write(move |args| {
            let mut service = cat_management_service_clone.lock();
            let data = args.recv_data();
            if data.len() <= 3 {
                error!("Name too short");
                return;
            }
            if data.len() > 32 {
                error!("Name too long");
                return;
            }

            let Ok(new_name) = String::from_utf8(data.into()) else {
                error!("Name not UTF 8");
                return;
            };

            service.name = new_name;
        });

        cat_management_service
    }
}

struct HostState {
    files: Arc<Mutex<FileUploadService>>,
    name: String,
    start: Instant,
    recv: mpsc::Receiver<WasmHostMessage>,
    next_wasm: Option<[u8; 32]>,
    ble_device: &'static BLEDevice,
    // led_driver: Mutex<LedcDriver<'static>>,
    led_pin: Mutex<PinDriver<'static, gpio::Gpio8, gpio::Output>>,
    callback_inhibited: bool,
}

impl HostState {
    fn handle_msg(host: &mut Host<Caller<'_, HostState>>, msg: WasmHostMessage) -> bool {
        match msg {
            WasmHostMessage::StartModule(hash) => {
                host.state_mut().next_wasm = Some(hash);
                return false;
            }
            WasmHostMessage::BLEAdvRecv(msg) => {
                host.on_ble_adv_recv(&msg)
                    .expect("failed to trigger ble adv callback");
            }
        }

        true
    }
}

impl HostBase for HostState {
    #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_base::host_log")]
    fn host_log(&mut self, log: common::Log) {
        match log.level {
            common::LogLevel::Error => ::tracing::error!(msg = &log.message, "guest logged"),
            common::LogLevel::Warn => ::tracing::warn!(msg = &log.message, "guest logged"),
            common::LogLevel::Info => ::tracing::info!(msg = &log.message, "guest logged"),
            common::LogLevel::Debug => ::tracing::debug!(msg = &log.message, "guest logged"),
            common::LogLevel::Trace => ::tracing::trace!(msg = &log.message, "guest logged"),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_base::get_name")]
    fn get_name(&mut self) -> String {
        self.name.clone()
    }

    #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_base::get_time_millis")]
    fn get_time_millis(&mut self) -> u32 {
        self.start.elapsed().as_millis() as u32
    }

    #[instrument(level = Level::TRACE, skip(host), target = "cms::wasm::host_base::on_yield")]
    fn on_yield(host: &mut Host<Caller<'_, Self>>, timeout: u32) -> bool {
        if host.state().callback_inhibited {
            // FIXME(lmv): don't block termination if yiedling during a callback
            debug!("on_yield inhibited");
            return true;
        }
        host.state_mut().callback_inhibited = true;
        if 0 < timeout {
            let mut now = Instant::now();
            let deadline = now + Duration::from_micros(timeout as u64);
            while now < deadline {
                match host.state_mut().recv.recv_timeout(deadline - now) {
                    Ok(msg) => {
                        if !Self::handle_msg(host, msg) {
                            return false;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(err) => {
                        error!(err = ?err, "recv on_yield failed");
                        break;
                    }
                }
                now = Instant::now();
            }
        }

        loop {
            match host.state_mut().recv.try_recv() {
                Ok(msg) => {
                    if !Self::handle_msg(host, msg) {
                        return false;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(err) => {
                    error!(err = ?err, "recv on_yield failed");
                    break;
                }
            }
        }

        host.state_mut().callback_inhibited = false;

        true
    }
}

impl LEDBrightness for HostState {
    #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_led::set_led_brightness")]
    fn set_led_brightness(&mut self, settings: common::LEDBrightnessSettings) {
        let b = settings.rgb[0] as u16 + settings.rgb[1] as u16 + settings.rgb[2] as u16;
        /* let mut led_pin = self.led_driver.lock();
        let duty = (led_pin.get_max_duty() as u64) * (b as u64) / (3 * 255);
        if let Err(err) = led_pin.set_duty(duty as u32) {
            error!(duty = duty, err = ?err "set_duty failed")
        }; */
        if b < 2 * 256 {
            info!("guest set led bightness low");
            self.led_pin.lock().set_low().unwrap();
        } else {
            info!("guest set led bightness high");
            self.led_pin.lock().set_high().unwrap();
        }
    }
}

impl BLEAdv for HostState {
    #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_ble::configure_ble_adv")]
    fn configure_ble_adv(&mut self, settings: common::BLEAdvSettings) {
        let min_interval = settings.min_interval.clamp(400, 1000);
        let max_interval = settings.min_interval.clamp(min_interval, 1500);
        self.ble_device
            .get_advertising()
            .lock()
            .min_interval(min_interval)
            .max_interval(max_interval);
    }

    #[instrument(level = Level::DEBUG, skip(self), target = "cms::wasm::host_ble::configure_ble_data")]
    fn configure_ble_data(&mut self, data: common::BLEAdvData) {
        if let Err(err) = self.ble_device.get_advertising().lock().set_data(
            BLEAdvertisementData::new()
                .name(&self.name)
                .manufacturer_data(&data.data),
        ) {
            error!("set manufacturer data ({:?}) failed: {:?}", data.data, err)
        };
    }
}
