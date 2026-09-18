#!/bin/sh
# NOTE: See "Before you push" in CONTRIBUTING.md.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

quick=0
for argument in "$@"; do
  case "$argument" in
    --quick) quick=1 ;;
    *) echo "usage: $0 [--quick]" >&2; exit 2 ;;
  esac
done

started=$(date +%s)
step_number=0

step() {
  step_number=$((step_number + 1))
  printf '\n\033[1m[%02d] %s\033[0m\n' "$step_number" "$1"
  shift
  if ! "$@"; then
    printf '\n\033[31mpreflight failed at step %02d.\033[0m\n' "$step_number" >&2
    exit 1
  fi
}

python=${OCOMMENT_PYTHON:-python3}
if ! "$python" -c 'import tomllib' >/dev/null 2>&1; then
  echo "preflight needs a Python with tomllib (3.11+); set OCOMMENT_PYTHON" >&2
  exit 2
fi

manifest=rust/Cargo.toml
binary=rust/target/debug/ocomment

step "Format"   cargo fmt --all --manifest-path "$manifest" -- --check
step "Build"    cargo build --manifest-path "$manifest" --locked --workspace
step "Clippy"   cargo clippy --manifest-path "$manifest" --workspace --all-targets --locked -- -D warnings

if command -v gofmt >/dev/null 2>&1; then
  OCOMMENT_REQUIRE_FORMATTERS=1 && export OCOMMENT_REQUIRE_FORMATTERS
fi
step "Tests"    cargo test --manifest-path "$manifest" --workspace --all-targets --locked
step "Doctests" cargo test --manifest-path "$manifest" --doc --workspace --locked

step "Library page examples" sh -c '
  cargo build --manifest-path rust/Cargo.toml --locked -p ocomment-core
  rustdoc --test docs/library.md --edition 2024 \
    --extern ocomment_core=rust/target/debug/libocomment_core.rlib \
    -L rust/target/debug/deps'

step "Schemas"          "$python" tools/validate_schemas.py --binary "$binary"
step "Embedded specs"   "$python" tools/check_embedded_specs.py
step "Directives"       "$python" tools/check_directives.py --binary "$binary"
step "Generated docs"   "$python" tools/gen_docs.py --binary "$binary" --check
step "Self-test corpus" "$python" tools/gen_selftest_corpus.py --check
step "Hooks"            "$python" tools/check_hooks.py
step "Editor ids"       "$python" tools/check_editor_ids.py
step "CI contracts"     "$python" tools/check_ci_contracts.py
step "Gate symmetry"    "$python" tools/check_gate_symmetry.py
step "Release metadata" "$python" tools/release_metadata.py --workspace --binary "$binary"
step "Release docs"     "$python" tools/sync_release_docs.py --check
step "Tool tests"       "$python" -m unittest tools/test_release_metadata.py tools/test_publish_crates.py tools/test_sync_release_docs.py

step "Own gate"      "./$binary" --format github
step "Own coverage"  "./$binary" coverage --deny-skipped --quiet
step "Self-test"     "./$binary" selftest

if command -v mdbook >/dev/null 2>&1; then
  step "Book" mdbook build docs
else
  printf '\n\033[33m  skipped: mdbook is not installed, so the book was not built.\033[0m\n'
fi

if [ "$quick" -eq 0 ]; then
  if command -v dune >/dev/null 2>&1 || command -v opam >/dev/null 2>&1; then
    step "Differential" sh tools/differential.sh
  else
    printf '\n\033[33m  skipped: no OCaml toolchain, so the two implementations were not compared.\033[0m\n'
  fi
  step "YAML round trip" "$python" tools/yaml_roundtrip.py --binary "$binary" --cases 200
  step "Documentation links" cargo doc --manifest-path "$manifest" --no-deps --workspace --locked
fi

printf '\n\033[32mpreflight passed: %d steps in %ds.\033[0m\n' "$step_number" "$(( $(date +%s) - started ))"
