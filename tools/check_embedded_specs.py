#!/usr/bin/env python3
"""Fail when publishable CLI assets drift from the canonical shared spec."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PAIRS = (
    (ROOT / "spec/config.schema.json", ROOT / "rust/ocomment/assets/config.schema.json"),
    (ROOT / "spec/default-config.toml", ROOT / "rust/ocomment/assets/default-config.toml"),
    (ROOT / "spec/languages.toml", ROOT / "rust/ocomment/assets/languages.toml"),
    (ROOT / "spec/ocomment-scanner.wit", ROOT / "rust/ocomment/assets/ocomment-scanner.wit"),
    (ROOT / "spec/profiles.toml", ROOT / "rust/ocomment/assets/profiles.toml"),
    (ROOT / "spec/generated.toml", ROOT / "rust/ocomment/assets/generated.toml"),
        # NOTE: Was absent, and drifted: the asset was a copy of `spec/directives.toml` from before the survey that asked every language what its toolchain reads, and it shipped to crates.io in that state.
    # NOTE: Nothing reads it today, which is exactly why nothing noticed.
    (ROOT / "spec/directives.toml", ROOT / "rust/ocomment/assets/directives.toml"),
)
# NOTE: The corpus `ocomment selftest` carries is a derivation rather than a copy, so it is not a pair here: `tools/gen_selftest_corpus.py --check` is what holds it to `spec/fixtures/v1`.


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
