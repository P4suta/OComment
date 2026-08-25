#!/usr/bin/env python3
"""Validate canonical schemas, the default config, and actual CLI JSON output."""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import tempfile
import tomllib

import jsonschema


ROOT = pathlib.Path(__file__).resolve().parents[1]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--binary",
        type=pathlib.Path,
        default=ROOT / "rust/target/debug/ocomment",
    )
    args = parser.parse_args()
    config_schema = json.loads((ROOT / "spec/config.schema.json").read_text())
    result_schema = json.loads((ROOT / "spec/result.schema.json").read_text())
    jsonschema.Draft202012Validator.check_schema(config_schema)
    jsonschema.Draft202012Validator.check_schema(result_schema)

    with (ROOT / "spec/default-config.toml").open("rb") as stream:
        jsonschema.validate(tomllib.load(stream), config_schema)

    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"CLI binary does not exist: {binary}")
    with tempfile.TemporaryDirectory(prefix="ocomment-schema-") as raw:
        fixture = pathlib.Path(raw) / "schema.rs"
        fixture.write_bytes(b"let value = 1; // removable\n")
        completed = subprocess.run(
            [str(binary), "scan", str(fixture), "--format", "json"],
            check=True,
            capture_output=True,
        )
    jsonschema.validate(json.loads(completed.stdout), result_schema)
    print("config and result schemas validate canonical runtime examples")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
