#!/bin/sh
set -eu

: "${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN is required}"
: "${PUBLISH_MAX_ATTEMPTS:=10}"
: "${PUBLISH_RETRY_DELAY:=15}"

package_identity() {
    manifest=$1
    cargo metadata --format-version 1 --no-deps --manifest-path "$manifest" |
        python3 -c '
import json, os, sys
manifest = os.path.realpath(sys.argv[1])
metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if os.path.realpath(package["manifest_path"]) == manifest]
if len(matches) != 1:
    raise SystemExit(f"metadata returned {len(matches)} packages for {manifest}")
package = matches[0]
print(package["name"], package["version"])
' "$manifest"
}

publish_one() {
    manifest=$1
    identity=$(package_identity "$manifest")
    package=${identity%% *}
    version=${identity#* }
    if [ -z "$package" ] || [ "$version" = "$identity" ]; then
        echo "could not resolve package name and version from $manifest" >&2
        return 1
    fi
    if cargo info "$package@$version" --registry crates-io >/dev/null 2>&1; then
        echo "$package@$version is already published (exact version verified by cargo info)"
        return
    fi
    attempt=1
    while [ "$attempt" -le "$PUBLISH_MAX_ATTEMPTS" ]
    do
        if cargo publish --manifest-path "$manifest" --locked --registry crates-io; then
            return
        fi
        if [ "$attempt" -eq "$PUBLISH_MAX_ATTEMPTS" ]; then
            echo "failed to publish $package@$version after registry propagation retries" >&2
            return 1
        fi
        sleep "$PUBLISH_RETRY_DELAY"
        attempt=$((attempt + 1))
    done
}

publish_one rust/ocomment-core/Cargo.toml
publish_one rust/ocomment-plugin-sdk/Cargo.toml
publish_one rust/ocomment/Cargo.toml
