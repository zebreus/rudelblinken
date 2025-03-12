#!/usr/bin/env bash

cargo build --release
cp target/wasm32-unknown-unknown/release/rudelblinken_wasm.wasm ../wasm-binaries/binaries
