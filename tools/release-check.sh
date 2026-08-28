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
python3 tools/check_directives.py --binary rust/target/release/ocomment
python3 tools/release_gate.py

# NOTE: The VS Code extension carries the crate version, and `publish-vscode`
# NOTE: refuses a Marketplace version that is not the tag. Checking it here
# NOTE: needs no npm install, so a forgotten bump is caught before the tag is
# NOTE: pushed rather than after the crates are already published.
python3 - <<'VERSIONS'
import json, re, sys

workspace = open("rust/Cargo.toml", encoding="utf-8").read()
section = workspace[workspace.index("[workspace.package]"):]
crate = re.search(r'^version\s*=\s*"([^"]+)"', section, re.M).group(1)
extension = json.load(open("editors/vscode/package.json", encoding="utf-8"))["version"]
if crate != extension:
    sys.exit(f"editors/vscode/package.json is {extension}, but the crates are {crate}")
VERSIONS

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
