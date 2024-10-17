use std::sync::Arc;

use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    BLEServer, NimbleProperties,
};
use esp_idf_sys as _;

use wasmi::{Caller, Engine, Func, Linker, Module, Store};

use crate::file_upload_service::FileUploadService;

const CAT_MANAGEMENT_SERVICE: u16 = 0x7992;
const CAT_MANAGEMENT_SERVICE_PROGRAM_HASH: u16 = 0x7893;
const CAT_MANAGEMENT_SERVICE_NAME: u16 = 0x7894;

const CAT_MANAGEMENT_SERVICE_UUID: BleUuid = BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE);
const CAT_MANAGEMENT_SERVICE_PROGRAM_HASH_UUID: BleUuid =
    BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE_PROGRAM_HASH);
const CAT_MANAGEMENT_SERVICE_NAME_UUID: BleUuid = BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE_NAME);
const NAMES: [&str; 256] = [
    "Camdyn",
    "Christan",
    "Kris",
    "Shaya",
    "Hartley",
    "Claudie",
    "Ashtin",
    "Krishna",
    "Terryl",
    "Marvis",
    "Riley",
    "Larkin",
    "Kodi",
    "Michal",
    "Blair",
    "Lavern",
    "Ricci",
    "Lavon",
    "Emery",
    "De",
    "Seneca",
    "Burnice",
    "Ocean",
    "Kendel",
    "Amari",
    "Kerry",
    "Marlowe",
    "Teegan",
    "Baby",
    "Jireh",
    "Talyn",
    "Kylin",
    "Mckinley",
    "Salem",
    "Dallis",
    "Jessie",
    "Waverly",
    "Ardell",
    "Arden",
    "Carey",
    "Kamdyn",
    "Jaziah",
    "Tam",
    "Demetrice",
    "Emerson",
    "Sonnie",
    "Cameran",
    "Kiran",
    "Kalin",
    "Devine",
    "Evyn",
    "Alva",
    "Justice",
    "Lakota",
    "Tristyn",
    "Ocie",
    "Delane",
    "Kailen",
    "Vernie",
    "Isa",
    "Indiana",
    "Shade",
    "Marshell",
    "Devyn",
    "Natividad",
    "Deniz",
    "Parris",
    "Dann",
    "An",
    "Ashten",
    "Shai",
    "Lian",
    "Milan",
    "Lorenza",
    "Britain",
    "Teddie",
    "Jaydyn",
    "Joell",
    "Lorin",
    "Micha",
    "Arlyn",
    "Ivory",
    "Tru",
    "Jae",
    "Adair",
    "Carrol",
    "Kodie",
    "Linn",
    "Bao",
    "Collen",
    "Arie",
    "Yael",
    "Emari",
    "Sol",
    "Kimani",
    "Robbie",
    "Chi",
    "Reilly",
    "Lennie",
    "Schyler",
    "Cedar",
    "Carlin",
    "Landry",
    "Sutton",
    "True",
    "Tenzin",
    "Armani",
    "Ryley",
    "Amaree",
    "Santana",
    "Jaime",
    "Divine",
    "Ossie",
    "Laramie",
    "Dwan",
    "Peyton",
    "Rennie",
    "Campbell",
    "Drue",
    "Jaelin",
    "Remy",
    "Allyn",
    "Aries",
    "Harley",
    "Karsen",
    "Jaylin",
    "Jourdan",
    "Cache",
    "Stevie",
    "Tylar",
    "Daylin",
    "Finley",
    "Adel",
    "Elisha",
    "Kaidyn",
    "Anay",
    "Mycah",
    "Jackie",
    "Shamari",
    "Toy",
    "Verdell",
    "Kenyatta",
    "Casey",
    "Jaedyn",
    "Clair",
    "Shia",
    "Rio",
    "Shea",
    "Shay",
    "Devonne",
    "Kalani",
    "Kriston",
    "Jazz",
    "Lavaughn",
    "Rylin",
    "Carrington",
    "Jeryl",
    "Ryen",
    "Artie",
    "Merlyn",
    "Trinidad",
    "Adi",
    "Rowan",
    "Camari",
    "Rian",
    "Payson",
    "Britt",
    "Tien",
    "Jaidan",
    "Taylen",
    "Aven",
    "Lin",
    "Tai",
    "Kary",
    "Maxie",
    "Lajuan",
    "Rhyan",
    "Aris",
    "Ellery",
    "Lannie",
    "Dru",
    "Mikah",
    "Armoni",
    "Leighton",
    "Berlin",
    "Aaryn",
    "Ashby",
    "Lexington",
    "Codie",
    "Charley",
    "Yuri",
    "Phoenix",
    "Arin",
    "Le",
    "Nazareth",
    "Finnley",
    "Clemmie",
    "Raynell",
    "Jael",
    "Mykah",
    "Earlie",
    "Barrie",
    "Alpha",
    "Onyx",
    "Micaiah",
    "Lashaun",
    "Gentry",
    "Thanh",
    "Kaedyn",
    "Golden",
    "Frankie",
    "Lyrik",
    "Kaylon",
    "Kareen",
    "Dominque",
    "Channing",
    "Dakotah",
    "Kendall",
    "Adrean",
    "Stephane",
    "Aly",
    "Azariah",
    "Yuki",
    "Ara",
    "Marice",
    "Pat",
    "Charly",
    "Samar",
    "Codi",
    "Sage",
    "Kamari",
    "Sloan",
    "Braylin",
    "Vinnie",
    "Nieves",
    "Quinn",
    "Jammie",
    "Berkeley",
    "Jimi",
    "Reese",
    "Storm",
    "Osiris",
    "Oakley",
    "Tory",
    "Jule",
    "Garnett",
    "Halen",
    "Deane",
    "Brittan",
    "Tobie",
    "Skyler",
    "Ellison",
    "Vernell",
    "Jody",
    "Arnell",
    "Berkley",
];

fn get_device_name() -> String {
    let name;
    unsafe {
        let mut mac = [0u8; 6];
        esp_idf_sys::esp_base_mac_addr_get(mac.as_mut_ptr());
        name = format!(
            "{}-{}-{}",
            NAMES[mac[3] as usize], NAMES[mac[4] as usize], NAMES[mac[5] as usize]
        );
    };
    return name;
}

pub struct CatManagementService {
    program_hash: Option<[u8; 32]>,
    name: String,
    file_upload_service: Arc<Mutex<FileUploadService>>,
}

impl CatManagementService {
    pub fn new(
        server: &mut BLEServer,
        files: Arc<Mutex<FileUploadService>>,
    ) -> Arc<Mutex<CatManagementService>> {
        let cat_management_service = Arc::new(Mutex::new(CatManagementService {
            name: get_device_name(),
            program_hash: None,
            file_upload_service: files,
        }));

        let service = server.create_service(CAT_MANAGEMENT_SERVICE_UUID);

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
                ::log::error!("Wrong hash length");
                return;
            };

            if service.program_hash == Some(hash) {
                return;
            }

            let wasm_module;
            {
                let file_upload_service = service.file_upload_service.lock();
                let Some(file) = file_upload_service.get_file(&hash) else {
                    ::log::error!("No file with that hash");
                    return;
                };
                wasm_module = file.content.clone();
            }

            service.program_hash = Some(hash);

            ::log::error!("WASM result: {}", run_wasm_module(&wasm_module).unwrap());

            ::log::error!("Loading the program is not yet implemented");
        });

        let name_characteristic = service.lock().create_characteristic(
            CAT_MANAGEMENT_SERVICE_NAME_UUID,
            NimbleProperties::WRITE | NimbleProperties::READ,
        );
        let cat_management_service_clone = cat_management_service.clone();
        name_characteristic.lock().on_read(move |value, _| {
            let service = cat_management_service_clone.lock();
            let hash = service.name.as_bytes();
            value.set_value(&hash);
        });
        let cat_management_service_clone = cat_management_service.clone();
        name_characteristic.lock().on_write(move |args| {
            let mut service = cat_management_service_clone.lock();
            let data = args.recv_data();
            if data.len() <= 3 {
                ::log::error!("Name too short");
                return;
            }
            if data.len() > 32 {
                ::log::error!("Name too long");
                return;
            }

            let Ok(new_name) = String::from_utf8(data.into()) else {
                ::log::error!("Name not UTF 8");
                return;
            };

            service.name = new_name;
        });

        return cat_management_service;
    }
}

fn run_wasm_module(wasm_module: &[u8]) -> anyhow::Result<u64> {
    let engine = Engine::default();

    let module = Module::new(&engine, wasm_module)?;

    type HostState = ();
    let mut store = Store::new(&engine, ());
    let host_ping = Func::wrap(&mut store, |_caller: Caller<'_, HostState>, param: i32| {
        println!("Got {param} from WebAssembly");
    });

    let mut linker = <Linker<HostState>>::new(&engine);

    linker.define("env", "ping", host_ping)?;
    let pre_instance = linker.instantiate(&mut store, &module)?;
    let instance = pre_instance.start(&mut store)?;
    let add = instance.get_typed_func::<(u64, u64), u64>(&store, "add")?;

    // And finally we can call the wasm!
    Ok(add.call(&mut store, (1, 3))?)
}
