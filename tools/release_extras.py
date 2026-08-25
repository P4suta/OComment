#!/usr/bin/env python3
"""Generate shell completions and copy stable documentation for release archives."""

from __future__ import annotations

import argparse
import pathlib
import shutil
import subprocess


ROOT = pathlib.Path(__file__).resolve().parents[1]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument(
        "--output", default=ROOT / "release-extras", type=pathlib.Path
    )
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    names = {
        "bash": "ocomment.bash",
        "zsh": "_ocomment",
        "fish": "ocomment.fish",
        "powershell": "_ocomment.ps1",
        "elvish": "ocomment.elv",
    }
    for shell, filename in names.items():
        completed = subprocess.run(
            [str(args.binary), "completions", shell],
            check=True,
            capture_output=True,
        )
        (args.output / filename).write_bytes(completed.stdout)
    shutil.copyfile(ROOT / "docs/ocomment.1", args.output / "ocomment.1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
