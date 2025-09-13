#!/usr/bin/env bash

set -ex

mkdir -p ./firmware
cp ../rudelblinken-firmware/target/riscv32imc-esp-espidf/release/bootloader.bin ./firmware
cp ../rudelblinken-firmware/partition_table.csv ./firmware
cp ../wasm-binaries/binaries/reference_sync_v1.wasm ./firmware/default_program.wasm
cp ../wasm-binaries/binaries/board_test.wasm ./firmware/test_program.wasm

cd ../rudelblinken-firmware
rm ./target/riscv32imc-esp-espidf/release/rudelblinken-firmware || true

cargo build --release
cp ./target/riscv32imc-esp-espidf/release/rudelblinken-firmware ../rudelctl/firmware/rudelblinken-firmware
cp ./target/riscv32imc-esp-espidf/release/rudelblinken-firmware ../rudelctl/firmware/board-test-firmware
