#!/usr/bin/env python3
"""Fail when publishable CLI assets drift from the canonical shared spec."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PAIRS = (
    (ROOT / "spec/config.schema.json", ROOT / "rust/ocomment/assets/config.schema.json"),
    (ROOT / "spec/default-config.toml", ROOT / "rust/ocomment/assets/default-config.toml"),
    (ROOT / "spec/ocomment-scanner.wit", ROOT / "rust/ocomment/assets/ocomment-scanner.wit"),
)


def main() -> int:
    failures = []
    for canonical, embedded in PAIRS:
        if canonical.read_bytes() != embedded.read_bytes():
            failures.append(f"{embedded.relative_to(ROOT)} differs from {canonical.relative_to(ROOT)}")
    if failures:
        print("\n".join(failures))
        return 1
    print(f"{len(PAIRS)} embedded specification assets match")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
