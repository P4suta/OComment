#!/bin/sh
set -eu

cargo fmt --all --manifest-path rust/Cargo.toml -- --check
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo test --manifest-path rust/Cargo.toml --workspace --all-targets --locked
dune runtest --root ocaml
./tools/differential.sh
python3 tools/check_embedded_specs.py
cargo build --manifest-path rust/Cargo.toml --release --locked -p ocomment
cargo build --manifest-path rust/Cargo.toml --release --locked -p ocomment-core --example throughput
python3 tools/validate_schemas.py --binary rust/target/release/ocomment
python3 tools/release_gate.py

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
