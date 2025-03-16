fn main() {
    println!("cargo:rerun-if-changed=./firmware/rudelblinken-firmware");
    println!("cargo:rerun-if-changed=./firmware/bootloader.bin");
    println!("cargo:rerun-if-changed=./firmware/partition_table.csv");
}
