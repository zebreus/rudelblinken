fn main() {
    embuild::espidf::sysenv::output();
    println!("cargo:rerun-if-changed=../wasm-binaries/binaries/board_test.wasm");
}
