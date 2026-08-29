#!/usr/bin/env python3
"""Build, expand, and test the three crates exactly as they are published."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tarfile
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[1]
MANIFESTS = (
    pathlib.Path("rust/ocomment-core/Cargo.toml"),
    pathlib.Path("rust/ocomment-plugin-sdk/Cargo.toml"),
    pathlib.Path("rust/ocomment/Cargo.toml"),
)


def run(command: list[str], *, environment: dict[str, str], cwd: pathlib.Path = ROOT) -> None:
    subprocess.run(command, cwd=cwd, env=environment, check=True)


def package_identity(
    manifest: pathlib.Path, *, environment: dict[str, str], offline: bool
) -> tuple[str, str]:
    command = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
        "--manifest-path",
        str(manifest),
    ]
    if offline:
        command.append("--offline")
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        check=True,
        text=True,
        capture_output=True,
    )
    metadata = json.loads(completed.stdout)
    absolute = manifest.resolve()
    matches = [
        package
        for package in metadata["packages"]
        if pathlib.Path(package["manifest_path"]).resolve() == absolute
    ]
    if len(matches) != 1:
        raise RuntimeError(f"metadata returned {len(matches)} packages for {manifest}")
    return str(matches[0]["name"]), str(matches[0]["version"])


def safe_extract(archive: pathlib.Path, destination: pathlib.Path) -> None:
    with tarfile.open(archive, "r:gz") as crate:
        destination_resolved = destination.resolve()
        for member in crate.getmembers():
            target = (destination / member.name).resolve()
            if target != destination_resolved and destination_resolved not in target.parents:
                raise RuntimeError(f"{archive} contains an unsafe path: {member.name}")
        crate.extractall(destination, filter="data")


def workspace_manifest(packages: list[tuple[str, str, pathlib.Path]]) -> str:
    members = ",\n  ".join(json.dumps(path.name) for _, _, path in packages)
    patches = "\n".join(
        f'{json.dumps(name)} = {{ path = {json.dumps(path.name)} }}'
        for name, _, path in packages
    )
    return (
        "[workspace]\n"
        'resolver = "2"\n'
        f"members = [\n  {members}\n]\n\n"
        "[patch.crates-io]\n"
        f"{patches}\n"
    )


def add_directory_source(
    package_root: pathlib.Path,
    archive: pathlib.Path,
    vendor: pathlib.Path,
) -> None:
    destination = vendor / package_root.name
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(package_root, destination)
    files = {}
    for path in sorted(destination.rglob("*")):
        if path.is_file() and path.name != ".cargo-checksum.json":
            files[path.relative_to(destination).as_posix()] = hashlib.sha256(
                path.read_bytes()
            ).hexdigest()
    checksum = {
        "$comment": "Generated from the exact cargo package archive for isolated verification.",
        "files": files,
        "package": hashlib.sha256(archive.read_bytes()).hexdigest(),
    }
    (destination / ".cargo-checksum.json").write_text(
        json.dumps(checksum, separators=(",", ":")), encoding="utf-8"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--offline",
        action="store_true",
        default=os.environ.get("PACKAGE_OFFLINE") == "1",
        help="use only the local Cargo cache",
    )
    parser.add_argument(
        "--skip-publish-dry-run",
        action="store_true",
        help="skip the redundant cargo publish --dry-run pass",
    )
    args = parser.parse_args()

    with tempfile.TemporaryDirectory(prefix="ocomment-packages-") as raw:
        isolated = pathlib.Path(raw)
        target = isolated / "package-target"
        expanded = isolated / "expanded"
        vendor = isolated / "vendor"
        cargo_home = isolated / "cargo-home"
        expanded.mkdir()
        environment = dict(os.environ)
        vendor_command = [
            "cargo",
            "vendor",
            "--manifest-path",
            str(ROOT / "rust/Cargo.toml"),
            "--locked",
            "--versioned-dirs",
        ]
        if args.offline:
            vendor_command.append("--offline")
        vendor_command.append(str(vendor))
        subprocess.run(
            vendor_command,
            cwd=ROOT,
            env=environment,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        cargo_home.mkdir()
        (cargo_home / "config.toml").write_text(
            "[source.crates-io]\n"
            'replace-with = "vendored-sources"\n\n'
            "[source.vendored-sources]\n"
            f'directory = {json.dumps(str(vendor))}\n',
            encoding="utf-8",
        )
        environment["CARGO_HOME"] = str(cargo_home)
        environment["CARGO_TARGET_DIR"] = str(target)
        packages: list[tuple[str, str, pathlib.Path]] = []
        for relative in MANIFESTS:
            manifest = (ROOT / relative).resolve()
            name, version = package_identity(
                manifest, environment=environment, offline=args.offline
            )
            package = [
                "cargo",
                "package",
                "--manifest-path",
                str(manifest),
                "--locked",
                "--allow-dirty",
                "--no-verify",
            ]
            if args.offline:
                package.append("--offline")
            run(package, environment=environment)
            archive = target / "package" / f"{name}-{version}.crate"
            if not archive.is_file():
                raise RuntimeError(f"cargo package did not create {archive}")
            safe_extract(archive, expanded)
            package_root = expanded / f"{name}-{version}"
            if not (package_root / "Cargo.toml").is_file():
                raise RuntimeError(f"{archive} has no self-contained Cargo.toml")
            packages.append((name, version, package_root))
            add_directory_source(package_root, archive, vendor)

            if not args.skip_publish_dry_run:
                publish = [
                    "cargo",
                    "publish",
                    "--manifest-path",
                    str(manifest),
                    "--dry-run",
                    "--registry",
                    "crates-io",
                    "--locked",
                    "--allow-dirty",
                    "--no-verify",
                ]
                if args.offline:
                    publish.append("--offline")
                run(publish, environment=environment)

        (expanded / "Cargo.toml").write_text(
            workspace_manifest(packages), encoding="utf-8"
        )
        verification_environment = dict(environment)
        verification_environment["CARGO_TARGET_DIR"] = str(isolated / "verify-target")
        suffix = ["--offline"] if args.offline else []
        run(
            ["cargo", "generate-lockfile", "--manifest-path", str(expanded / "Cargo.toml"), *suffix],
            environment=verification_environment,
            cwd=expanded,
        )
        for command in ("check", "test"):
            run(
                [
                    "cargo",
                    command,
                    "--manifest-path",
                    str(expanded / "Cargo.toml"),
                    "--workspace",
                    "--all-targets",
                    "--locked",
                    *suffix,
                ],
                environment=verification_environment,
                cwd=expanded,
            )
        print(f"packaged and verified {len(packages)} self-contained crate archives")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
