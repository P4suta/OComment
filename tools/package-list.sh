#!/bin/sh
set -eu

for manifest in \
    rust/vendor/wasm_runtime_layer/Cargo.toml \
    rust/vendor/wasmi_runtime_layer/Cargo.toml \
    rust/vendor/wasm_component_layer/Cargo.toml \
    rust/ocomment-core/Cargo.toml \
    rust/ocomment-plugin-sdk/Cargo.toml \
    rust/ocomment/Cargo.toml
do
    cargo package --manifest-path "$manifest" --locked --allow-dirty --list >/dev/null
done
