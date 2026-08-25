#!/usr/bin/env python3
"""Generate the shell completions and the manual page for release archives."""

from __future__ import annotations

import argparse
import pathlib
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
    page = subprocess.run(
        [str(args.binary), "man"],
        check=True,
        capture_output=True,
    )
    (args.output / "ocomment.1").write_bytes(page.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
