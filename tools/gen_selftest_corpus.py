#!/usr/bin/env python3
"""Build the corpus `ocomment selftest` carries inside the binary.

The self-test re-runs the shared fixtures on the installed executable, where
`spec/` is not on disk, so the cases have to travel with it. Embedding the two
canonical files verbatim would carry 533 KB, and more than half of that is
material the self-test has no use for: the `note` explaining each case to a
reader, the `diagnostics` block, the recorded `edits` and `source_map` of the
differential protocol, and the indentation.

So this writes a derived file holding exactly what the check compares -- the
input, the options, and the recorded comments and output -- with no formatting.
It is a derivation rather than a second source: `--check` fails when it no
longer matches what `spec/fixtures/v1` would produce, which is what stops the
binary from certifying itself against cases the project has moved on from.

Only the standard library is used, because this runs in a job that installs
nothing beyond the toolchain.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
CORPUS = ROOT / "spec/fixtures/v1"
FLOOR = CORPUS / "floor.txt"
TARGET = ROOT / "rust/ocomment/assets/selftest-corpus.json"

# NOTE: The keys the self-test reads.
# NOTE: `operation` decides which entry point runs,
# NOTE: `profile` is needed by the two profile operations, and a case carries its source as one of the two spellings.
CASE_KEYS = (
    "id",
    "language",
    "dialect",
    "operation",
    "options",
    "profile",
    "source_utf8",
    "source_base64",
)

# NOTE: What it compares against.
# NOTE: `diagnostics` is deliberately absent: the self-test asks whether this binary classifies and rewrites the way the corpus records, and the diagnostic codes are checked by the library test and the differential run, which both have the whole file.
EXPECT_KEYS = ("valid", "comments", "output_utf8", "output_base64")


def floors() -> dict[str, int]:
    """The floors recorded beside the corpus, which travel with it."""
    found: dict[str, int] = {}
    for number, line in enumerate(FLOOR.read_text(encoding="utf-8").splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        fields = stripped.split()
        if len(fields) != 2:
            raise SystemExit(f"floor.txt:{number}: expected `name count`, got {line!r}")
        found[fields[0]] = int(fields[1])
    for name in ("cases", "expectations"):
        if name not in found:
            raise SystemExit(f"floor.txt names no `{name}`")
    return found


def corpus() -> str:
    """The derived corpus, as the bytes that belong in the asset."""
    cases = []
    for path in sorted(CORPUS.glob("*.json")):
        document = json.loads(path.read_text(encoding="utf-8"))
        for case in document["cases"]:
            if "expect" not in case:
                continue
            slim = {key: case[key] for key in CASE_KEYS if key in case}
            slim["expect"] = {
                key: case["expect"][key] for key in EXPECT_KEYS if key in case["expect"]
            }
            cases.append(slim)
    document = {"version": 1, "floors": floors(), "cases": cases}
    return json.dumps(document, separators=(",", ":"), ensure_ascii=False) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail instead of writing when the embedded corpus is out of date",
    )
    args = parser.parse_args()

    expected = corpus()
    actual = TARGET.read_text(encoding="utf-8") if TARGET.is_file() else None
    if args.check:
        if actual != expected:
            print(
                f"{TARGET.relative_to(ROOT)} is not what spec/fixtures/v1 would"
                " produce; regenerate it with"
                " `python3 tools/gen_selftest_corpus.py`",
                file=sys.stderr,
            )
            return 1
        print(f"the embedded self-test corpus matches spec/fixtures/v1")
        return 0
    if actual == expected:
        print("the embedded self-test corpus is already current")
        return 0
    TARGET.write_text(expected, encoding="utf-8")
    print(f"wrote {TARGET.relative_to(ROOT)} ({len(expected):,} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
