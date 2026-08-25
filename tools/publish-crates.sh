#!/bin/sh
set -eu

: "${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN is required}"

publish_one() {
    package=$1
    version=$2
    manifest=$3
    if cargo info "$package@$version" >/dev/null 2>&1; then
        echo "$package@$version is already published"
        return
    fi
    attempt=1
    while [ "$attempt" -le 10 ]
    do
        if cargo publish --manifest-path "$manifest" --locked; then
            return
        fi
        if [ "$attempt" -eq 10 ]; then
            echo "failed to publish $package@$version after registry propagation retries" >&2
            return 1
        fi
        sleep 15
        attempt=$((attempt + 1))
    done
}

publish_one ocomment-wasm-runtime-layer 0.4.2 rust/vendor/wasm_runtime_layer/Cargo.toml
publish_one ocomment-wasmi-runtime-layer 0.31.0 rust/vendor/wasmi_runtime_layer/Cargo.toml
publish_one ocomment-wasm-component-layer 0.1.18-ocomment.1 rust/vendor/wasm_component_layer/Cargo.toml
publish_one ocomment-core 0.1.0 rust/ocomment-core/Cargo.toml
publish_one ocomment-plugin-sdk 0.1.0 rust/ocomment-plugin-sdk/Cargo.toml
publish_one ocomment 0.1.0 rust/ocomment/Cargo.toml
