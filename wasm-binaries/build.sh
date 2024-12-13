#!/usr/bin/env bash
set -e

workspace_metadata="$(cargo metadata --no-deps --format-version 1)"
members="$(echo "$workspace_metadata" | jq -r '.workspace_members[]' | sed 's/^.*wasm-tests\/// ; s/#.*$//')"

cargo build --release

mkdir -p binaries
for member in $members; do
    member_name="$(basename "$member" | tr - _)"
    wasm_file="target/wasm32-unknown-unknown/release/$member_name.wasm"
    cp "$wasm_file" "binaries/$member_name.wasm"
done
