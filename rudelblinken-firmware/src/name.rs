use crate::config;

const NAMES: [&str; 256] = [
    "Addison", "Aero", "Amari", "Angel", "Arden", "Ariel", "Arrow", "Artemis", "Aspen", "Atlas",
    "August", "Avalon", "Avery", "Bagheera", "Bailey", "Bellamy", "Bengal", "Blair", "Blake",
    "Blue", "Briar", "Brooklyn", "Caelan", "Camden", "Cameron", "Carey", "Casey", "Cedar",
    "Charlie", "Chase", "Cheetah", "Clove", "Cypress", "Dakota", "Dallas", "Dani", "Darcy", "Dash",
    "Denim", "Devon", "Drew", "Echo", "Eden", "Elio", "Ellery", "Ellis", "Ember", "Emerson",
    "Emery", "Everest", "Everly", "Ezra", "Fable", "Fallon", "Feline", "Felix", "Fenix", "Fern",
    "Finn", "Flynn", "Frankie", "Furby", "Galaxy", "Gale", "Glade", "Gray", "Greer", "Halo",
    "Harper", "Haven", "Hayden", "Hero", "Hollis", "Indigo", "Ion", "Ira", "Ivory", "Izzy",
    "Jackie", "Jaden", "Jaguar", "Jamie", "Janis", "Jay", "Jesse", "Jett", "Jinx", "Jules",
    "Julian", "Juniper", "Jupiter", "Justice", "Kai", "Kamari", "Keegan", "Keely", "Kellan",
    "Kendall", "Kisa", "Kit", "Kitty", "Koa", "Kye", "Lake", "Landry", "Lane", "Lark", "Laurie",
    "Leo", "Levi", "Lior", "Logan", "London", "Lotus", "Lou", "Luca", "Lucky", "Lumen", "Lux",
    "Lynx", "Lyric", "Mackerel", "Mads", "Maine", "Marley", "Marlowe", "Maru", "Max", "Memphis",
    "Meow", "Micah", "Midnight", "Milan", "Milo", "Mistral", "Mittens", "Momo", "Moon", "Morgan",
    "Navy", "Nebula", "Neko", "Nico", "Noel", "Nomad", "North", "Nova", "Nyx", "Oak", "Ocean",
    "Ocelot", "Ollie", "Onyx", "Orion", "Owen", "Panther", "Parker", "Paws", "Pax", "Payton",
    "Percy", "Phoenix", "Prism", "Purrcy", "Purrin", "Quill", "Quinley", "Quinn", "Rain", "Raine",
    "Ray", "Reese", "Remi", "Ren", "Reverie", "Riley", "Rio", "River", "Robin", "Rory", "Rowan",
    "Royal", "Rumi", "Rune", "Rylan", "Saber", "Sable", "Sage", "Salem", "Sam", "Sasha", "Scout",
    "Seven", "Shadow", "Shiloh", "Silver", "Sirius", "Sky", "Skyfire", "Skylar", "Sol", "Solstice",
    "Sparrow", "Sphinx", "Stellar", "Storm", "Sutton", "Tabby", "Tarian", "Tatum", "Taylor",
    "Teddy", "Tempest", "Tenzin", "Teo", "Theo", "Tiger", "Timber", "Tora", "Tori", "Toulouse",
    "True", "Truth", "Utah", "Vail", "Val", "Valor", "Vega", "Velour", "Velvet", "Vesper", "Vireo",
    "Waverly", "West", "Whisper", "Wilder", "Winter", "Wren", "Wynn", "Xander", "Xen", "Xenith",
    "Xenon", "Yael", "Yale", "Yarrow", "York", "Zahar", "Zane", "Zariah", "Zenith", "Zephyr",
    "Zev", "Ziggy", "Zimba", "Zinnia",
];

pub fn initialize_name() -> String {
    let name = config::device_name::get();
    if let Some(name) = name {
        return name;
    };
    let id = unsafe {
        esp_idf_sys::bootloader_random_enable();
        let id: u8 = esp_idf_sys::esp_random().to_le_bytes()[0];
        esp_idf_sys::bootloader_random_disable();
        id
    };
    let name = NAMES[id as usize % NAMES.len()].to_string();
    config::device_name::set(&Some(name.to_string()));
    config::device_name::get().unwrap()
}
