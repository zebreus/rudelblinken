#[rudelblinken_sdk_macro::main]
pub fn main() {
    println!("Hello, world!");
}

#[rudelblinken_sdk_macro::on_advertisement]
fn on_advertisement(_: rudelblinken_sdk::Advertisement) {}
