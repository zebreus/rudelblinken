#[rudelblinken_sdk_macro::main]
pub fn main() {
    println!("Hello, world!");
}

#[rudelblinken_sdk_macro::on_event]
fn on_event(_: rudelblinken_sdk::BleEvent) {}
