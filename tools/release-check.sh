#!/bin/sh
set -eu

cargo fmt --all --manifest-path rust/Cargo.toml -- --check
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo test --manifest-path rust/Cargo.toml --workspace --all-targets --locked
dune runtest --root ocaml
./tools/differential.sh
python3 tools/check_embedded_specs.py
python3 tools/check_hooks.py
python3 tools/check_editor_ids.py
python3 tools/check_ci_contracts.py
python3 -m unittest tools/test_release_metadata.py tools/test_publish_crates.py
cargo build --manifest-path rust/Cargo.toml --release --locked -p ocomment
cargo build --manifest-path rust/Cargo.toml --release --locked -p ocomment-core --example throughput
python3 tools/validate_schemas.py --binary rust/target/release/ocomment
python3 tools/check_directives.py --binary rust/target/release/ocomment
python3 tools/release_gate.py
python3 tools/release_metadata.py --workspace --binary rust/target/release/ocomment

./tools/package-list.sh
