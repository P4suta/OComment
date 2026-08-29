#!/usr/bin/env python3
"""Exercise publish-crates.sh with a stateful fake Cargo registry."""

from __future__ import annotations

import os
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]


class PublishCratesTests(unittest.TestCase):
    def test_failed_run_resumes_in_dependency_order(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ocomment-publish-test-") as raw:
            state = pathlib.Path(raw)
            binary = state / "bin"
            binary.mkdir()
            fake = binary / "cargo"
            fake.write_text(
                """#!/bin/sh
set -eu
command=$1
shift
manifest=
previous=
registry=
for argument in "$@"; do
  if [ "$previous" = "--manifest-path" ]; then manifest=$argument; fi
  if [ "$previous" = "--registry" ]; then registry=$argument; fi
  previous=$argument
done
identity() {
  case "$manifest" in
    *ocomment-core*) echo 'ocomment-core 0.1.0' ;;
    *ocomment-plugin-sdk*) echo 'ocomment-plugin-sdk 0.1.0' ;;
    *ocomment/Cargo.toml) echo 'ocomment 0.1.0' ;;
    *) exit 90 ;;
  esac
}
case "$command" in
  metadata)
    set -- $(identity)
    absolute=$(cd "$(dirname "$manifest")" && pwd)/$(basename "$manifest")
    printf '{"packages":[{"name":"%s","version":"%s","manifest_path":"%s"}]}\n' "$1" "$2" "$absolute"
    ;;
  info)
    requested=$1
    package=${requested%@*}
    test "$registry" = crates-io
    test -f "$FAKE_CARGO_STATE/published-$package"
    ;;
  publish)
    test "$registry" = crates-io
    set -- $(identity)
    package=$1
    printf '%s\n' "$package" >>"$FAKE_CARGO_STATE/publish.log"
    if [ "$package" = "ocomment-plugin-sdk" ]; then
      count_file="$FAKE_CARGO_STATE/sdk-attempts"
      count=0
      if [ -f "$count_file" ]; then count=$(cat "$count_file"); fi
      count=$((count + 1))
      printf '%s\n' "$count" >"$count_file"
      if [ "$count" -le 2 ]; then exit 1; fi
    fi
    : >"$FAKE_CARGO_STATE/published-$package"
    ;;
  *) exit 91 ;;
esac
""",
                encoding="utf-8",
            )
            fake.chmod(0o755)
            environment = dict(os.environ)
            environment.update(
                {
                    "PATH": f"{binary}:{environment['PATH']}",
                    "FAKE_CARGO_STATE": str(state),
                    "CARGO_REGISTRY_TOKEN": "test-token",
                    "PUBLISH_MAX_ATTEMPTS": "2",
                    "PUBLISH_RETRY_DELAY": "0",
                }
            )
            first = subprocess.run(
                ["./tools/publish-crates.sh"], cwd=ROOT, env=environment, check=False
            )
            self.assertNotEqual(first.returncode, 0)
            second = subprocess.run(
                ["./tools/publish-crates.sh"], cwd=ROOT, env=environment, check=False
            )
            self.assertEqual(second.returncode, 0)
            attempts = (state / "publish.log").read_text(encoding="utf-8").splitlines()
            self.assertEqual(
                attempts,
                [
                    "ocomment-core",
                    "ocomment-plugin-sdk",
                    "ocomment-plugin-sdk",
                    "ocomment-plugin-sdk",
                    "ocomment",
                ],
            )


if __name__ == "__main__":
    unittest.main()
