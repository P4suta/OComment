#!/usr/bin/env python3
"""Validate every version-bearing input before release artifacts are built."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import sys
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[1]
PUBLIC_CRATES = (
    ("ocomment-core", pathlib.Path("rust/ocomment-core/Cargo.toml")),
    ("ocomment-plugin-sdk", pathlib.Path("rust/ocomment-plugin-sdk/Cargo.toml")),
    ("ocomment", pathlib.Path("rust/ocomment/Cargo.toml")),
)
SEMVER = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+")


def load_toml(path: pathlib.Path) -> dict:
    with path.open("rb") as stream:
        return tomllib.load(stream)


def metadata_failures(root: pathlib.Path, version: str) -> list[str]:
    failures: list[str] = []
    if not SEMVER.fullmatch(version):
        return [f"version must be MAJOR.MINOR.PATCH, got {version!r}"]

    workspace_path = root / "rust/Cargo.toml"
    workspace = load_toml(workspace_path)
    workspace_version = str(workspace["workspace"]["package"]["version"])
    if workspace_version != version:
        failures.append(
            f"rust/Cargo.toml workspace version is {workspace_version}, expected {version}"
        )

    for package_name, relative in PUBLIC_CRATES:
        manifest = load_toml(root / relative)
        package = manifest.get("package", {})
        if package.get("name") != package_name:
            failures.append(f"{relative} does not describe {package_name}")
        declared = package.get("version")
        if declared is None:
            failures.append(f"{relative} has no package version")
        elif isinstance(declared, dict):
            if declared != {"workspace": True}:
                failures.append(f"{relative} has an unsupported workspace version form")
        elif str(declared) != version:
            failures.append(f"{relative} version is {declared}, expected {version}")

    lock = load_toml(root / "rust/Cargo.lock")
    locked = {
        str(package.get("name")): str(package.get("version"))
        for package in lock.get("package", [])
        if package.get("name") in {name for name, _ in PUBLIC_CRATES}
    }
    for package_name, _ in PUBLIC_CRATES:
        if locked.get(package_name) != version:
            failures.append(
                f"rust/Cargo.lock has {package_name}@{locked.get(package_name)}, expected {version}"
            )

    extension_path = root / "editors/vscode/package.json"
    extension = json.loads(extension_path.read_text(encoding="utf-8"))
    if str(extension.get("version")) != version:
        failures.append(
            f"editors/vscode/package.json version is {extension.get('version')}, expected {version}"
        )

    heading = re.compile(rf"^##\s+\[?{re.escape(version)}\]?(?:\s|$)", re.MULTILINE)
    for relative in (pathlib.Path("CHANGELOG.md"), pathlib.Path("editors/vscode/CHANGELOG.md")):
        if not heading.search((root / relative).read_text(encoding="utf-8")):
            failures.append(f"{relative} has no {version} release heading")
    return failures


def binary_failures(binary: pathlib.Path, version: str) -> list[str]:
    try:
        completed = subprocess.run(
            [str(binary), "--version"],
            check=False,
            capture_output=True,
        )
    except OSError as error:
        return [f"cannot run release binary {binary}: {error}"]
    expected = f"ocomment {version}\n".encode()
    failures = []
    if completed.returncode != 0:
        failures.append(f"release binary exited {completed.returncode} for --version")
    if completed.stdout != expected:
        failures.append(
            f"release binary version output is {completed.stdout!r}, expected {expected!r}"
        )
    if completed.stderr:
        failures.append(f"release binary wrote to stderr for --version: {completed.stderr!r}")
    return failures


def git_output(root: pathlib.Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(root), *arguments],
        check=False,
        text=True,
        capture_output=True,
    )


def tag_failures(root: pathlib.Path, tag: str) -> list[str]:
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
        return [f"release tag must be vMAJOR.MINOR.PATCH, got {tag!r}"]
    failures = []
    tagged = git_output(root, "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
    if tagged.returncode != 0:
        return [f"tag {tag} does not resolve to a commit"]
    commit = tagged.stdout.strip()
    head = git_output(root, "rev-parse", "--verify", "HEAD^{commit}")
    if head.returncode != 0 or head.stdout.strip() != commit:
        failures.append(f"HEAD is not the commit named by {tag}")
    main = git_output(root, "rev-parse", "--verify", "refs/remotes/origin/main^{commit}")
    if main.returncode != 0:
        failures.append("origin/main is unavailable; fetch it before releasing")
    else:
        ancestor = git_output(
            root,
            "merge-base",
            "--is-ancestor",
            commit,
            main.stdout.strip(),
        )
        if ancestor.returncode != 0:
            failures.append(f"tag {tag} is not an ancestor of origin/main")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    selector = parser.add_mutually_exclusive_group(required=True)
    selector.add_argument("--tag", help="release tag; also verifies Git ancestry")
    selector.add_argument("--version", help="version to check without Git ancestry")
    selector.add_argument(
        "--workspace",
        action="store_true",
        help="derive the version from rust/Cargo.toml and check local metadata",
    )
    parser.add_argument("--root", type=pathlib.Path, default=ROOT)
    parser.add_argument("--binary", type=pathlib.Path)
    args = parser.parse_args()

    root = args.root.resolve()
    tag = args.tag
    if args.workspace:
        workspace = load_toml(root / "rust/Cargo.toml")
        version = str(workspace["workspace"]["package"]["version"])
    else:
        version = tag.removeprefix("v") if tag is not None else args.version
    assert version is not None
    failures = metadata_failures(root, version)
    if args.binary is not None:
        failures.extend(binary_failures(args.binary.resolve(), version))
    elif tag is not None:
        failures.append("tag mode requires --binary")
    if tag is not None:
        failures.extend(tag_failures(root, tag))
    if failures:
        for failure in failures:
            print(f"release metadata: {failure}", file=sys.stderr)
        return 1
    print(f"release metadata agrees on {version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
