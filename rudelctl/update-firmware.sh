#!/usr/bin/env bash

set -ex

cd ../rudelblinken-firmware
cargo build --release
cd ../rudelctl
mkdir -p ./firmware
cp ../rudelblinken-firmware/target/riscv32imc-esp-espidf/release/rudelblinken-firmware ./firmware
cp ../rudelblinken-firmware/target/riscv32imc-esp-espidf/release/bootloader.bin ./firmware
cp ../rudelblinken-firmware/partition_table.csv ./firmware
