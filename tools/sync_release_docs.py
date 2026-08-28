#!/usr/bin/env python3
"""Keep stable OComment pins in user-facing docs on the workspace version."""

from __future__ import annotations

import argparse
import pathlib
import re
import sys
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[1]
PINNED_DOCS = (
    pathlib.Path("README.md"),
    pathlib.Path("docs/ci.md"),
    pathlib.Path("docs/docker.md"),
    pathlib.Path("docs/installation.md"),
    pathlib.Path("docs/verify.md"),
)
OCOMMENT_PIN = re.compile(
    r"(?:"
    r"ghcr\.io/p4suta/ocomment:|"
    r"P4suta/OComment@v|"
    r"\brev:\s+v|"
    r"gh release download v|"
    r"refs/tags/v"
    r")(?P<version>[0-9]+\.[0-9]+\.[0-9]+)"
)


def workspace_version(root: pathlib.Path) -> str:
    with (root / "rust/Cargo.toml").open("rb") as stream:
        manifest = tomllib.load(stream)
    return str(manifest["workspace"]["package"]["version"])


def pinned_versions(root: pathlib.Path) -> dict[pathlib.Path, set[str]]:
    versions: dict[pathlib.Path, set[str]] = {}
    for relative in PINNED_DOCS:
        text = (root / relative).read_text(encoding="utf-8")
        matches = {match.group("version") for match in OCOMMENT_PIN.finditer(text)}
        versions[relative] = matches
    return versions


def failures(root: pathlib.Path) -> list[str]:
    expected = workspace_version(root)
    problems = []
    for relative, versions in pinned_versions(root).items():
        if not versions:
            problems.append(f"{relative} has no stable OComment version pin")
            continue
        wrong = sorted(versions - {expected})
        if wrong:
            problems.append(
                f"{relative} pins {', '.join(wrong)}, expected workspace version {expected}"
            )
    return problems


def synchronize(root: pathlib.Path) -> int:
    expected = workspace_version(root)
    changed = 0
    for relative, versions in pinned_versions(root).items():
        path = root / relative
        original = path.read_text(encoding="utf-8")
        updated = original
        for version in versions - {expected}:
            updated = updated.replace(version, expected)
        if updated != original:
            path.write_text(updated, encoding="utf-8")
            changed += 1
    return changed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--root", type=pathlib.Path, default=ROOT)
    args = parser.parse_args()
    root = args.root.resolve()

    if args.check:
        problems = failures(root)
        if problems:
            print("\n".join(problems), file=sys.stderr)
            return 1
        print(f"{len(PINNED_DOCS)} release-document pins match {workspace_version(root)}")
        return 0

    changed = synchronize(root)
    problems = failures(root)
    if problems:
        print("\n".join(problems), file=sys.stderr)
        return 1
    print(f"synchronized {changed} release-document files to {workspace_version(root)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
