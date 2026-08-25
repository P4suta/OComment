#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cargo build --manifest-path "$root/rust/Cargo.toml" -p ocomment-core --example ref_driver --locked
dune build --root "$root/ocaml" bin/main.exe
python3 "$root/tools/differential.py"
