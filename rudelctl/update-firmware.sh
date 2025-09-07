#!/usr/bin/env bash

set -ex

mkdir -p ./firmware
cp ../rudelblinken-firmware/target/riscv32imc-esp-espidf/release/bootloader.bin ./firmware
cp ../rudelblinken-firmware/partition_table.csv ./firmware

cd ../rudelblinken-firmware
rm ./target/riscv32imc-esp-espidf/release/rudelblinken-firmware || true

cargo build --release
cp ./target/riscv32imc-esp-espidf/release/rudelblinken-firmware ../rudelctl/firmware/rudelblinken-firmware

cargo build --release -F board-test
cp ./target/riscv32imc-esp-espidf/release/rudelblinken-firmware ../rudelctl/firmware/board-test-firmware
