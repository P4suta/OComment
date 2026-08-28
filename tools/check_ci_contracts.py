#!/usr/bin/env python3
"""Enforce the security and release-DAG contracts of repository automation."""

from __future__ import annotations

import pathlib
import re
import sys
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[1]
AUTOMATION = [ROOT / "action.yml", *sorted((ROOT / ".github/workflows").glob("*.yml"))]
PINS = {
    "actions/attest": ("1e69f48acb82d1966a394da916b4c1698aa569d6", "v4.2.2"),
    "actions/attest-build-provenance": (
        "4d101475d8b20a2381f78447822ac1eab6504dd8",
        "v4.2.2",
    ),
    "actions/checkout": ("3d3c42e5aac5ba805825da76410c181273ba90b1", "v7.0.1"),
    "actions/deploy-pages": ("cd2ce8fcbc39b97be8ca5fce6e763baed58fa128", "v5.0.0"),
    "actions/download-artifact": (
        "3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c",
        "v8.0.1",
    ),
    "actions/setup-node": ("820762786026740c76f36085b0efc47a31fe5020", "v7.0.0"),
    "actions/upload-artifact": (
        "043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
        "v7.0.1",
    ),
    "actions/upload-pages-artifact": (
        "fc324d3547104276b827a68afc52ff2a11cc49c9",
        "v5.0.0",
    ),
    "anchore/sbom-action": ("e22c389904149dbc22b58101806040fa8d37a610", "v0.24.0"),
    "docker/build-push-action": (
        "53b7df96c91f9c12dcc8a07bcb9ccacbed38856a",
        "v7.3.0",
    ),
    "docker/login-action": ("dbcb813823bdd20940b903addbd779551569679f", "v4.6.0"),
    "docker/metadata-action": ("dc802804100637a589fabce1cb79ff13a1411302", "v6.2.0"),
    "docker/setup-buildx-action": (
        "37fe631027851001ddb9b187196cc803df7f5f0e",
        "v4.3.0",
    ),
    "docker/setup-qemu-action": (
        "96fe6ef7f33517b61c61be40b68a1882f3264fb8",
        "v4.2.0",
    ),
    "dtolnay/rust-toolchain": ("4360b52568e2003a75bf9bc1d59f33a8e3fc893c", "stable toolchain action"),
    "github/codeql-action": ("db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28", "v4.37.8"),
    "ocaml/setup-ocaml": ("f92e0606b7ae4873dd1238465ea4bf6f8e40d85c", "v3"),
    "sigstore/cosign-installer": ("6f9f17788090df1f26f669e9d70d6ae9567deba6", "v4.1.2"),
    "taiki-e/upload-rust-binary-action": (
        "f0d45ae91ee7b8ee928de7a9d04d893a08bcbec6",
        "v1.30.2",
    ),
}
USES = re.compile(r"^\s*-?\s*uses:\s*([^@\s]+)@([^\s#]+)(?:\s+#\s*(.*))?$", re.MULTILINE)


def main() -> int:
    failures = []
    seen = set()
    for path in AUTOMATION:
        text = path.read_text(encoding="utf-8")
        for line_number, line in enumerate(text.splitlines(), start=1):
            if "uses:" not in line or line.lstrip().startswith("#"):
                continue
            if re.search(r"\buses:\s*\./", line):
                continue
            match = USES.match(line)
            if match is None:
                failures.append(f"{path.relative_to(ROOT)}:{line_number}: remote action is not SHA-pinned")
                continue
            action, revision, comment = match.groups()
            pin_name = next(
                (name for name in PINS if action == name or action.startswith(f"{name}/")),
                None,
            )
            if pin_name is None:
                failures.append(f"{path.relative_to(ROOT)}:{line_number}: unreviewed action {action}")
                continue
            seen.add(pin_name)
            expected = PINS[pin_name]
            if revision != expected[0]:
                failures.append(
                    f"{path.relative_to(ROOT)}:{line_number}: {action} is {revision}, expected {expected[0]}"
                )
            if comment is None or expected[1] not in comment:
                failures.append(
                    f"{path.relative_to(ROOT)}:{line_number}: {action} needs version comment {expected[1]!r}"
                )
    unused = sorted(set(PINS) - seen)
    if unused:
        failures.append(f"reviewed action pin table has unused entries: {', '.join(unused)}")

    dockerfile = (ROOT / "Dockerfile").read_text(encoding="utf-8")
    if not re.search(r"^FROM rust:1\.88-alpine@sha256:[0-9a-f]{64} AS builder$", dockerfile, re.MULTILINE):
        failures.append("Dockerfile builder image is not pinned by a full index digest")

    benchmark = (ROOT / ".github/workflows/benchmark.yml").read_text(encoding="utf-8")
    for forbidden in ("pull_request:",):
        if forbidden in benchmark:
            failures.append("benchmark workflow must be manual-only")
    for required in (
        "^[0-9a-fA-F]{40}$",
        "environment: benchmark",
        "ocomment-benchmark, ephemeral",
        "runs-on: ubuntu-latest",
        "DISPATCH_SHA: ${{ github.sha }}",
    ):
        if required not in benchmark:
            failures.append(f"benchmark workflow is missing {required!r}")
    if benchmark.count("ref: ${{ github.sha }}") != 2:
        failures.append("both benchmark checkouts must use the immutable workflow dispatch SHA")
    if "needs.verify-commit.outputs.commit_sha" in benchmark:
        failures.append("benchmark must not execute a user-derived job output on a self-hosted runner")

    codeql = (ROOT / ".github/workflows/codeql.yml").read_text(encoding="utf-8")
    if "language: javascript-typescript" not in codeql:
        failures.append("CodeQL does not analyze JavaScript/TypeScript")

    ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    if "chmod 0755 release/binaries/out/amd64/ocomment" not in ci:
        failures.append("Docker CI does not reproduce the released archive's executable mode")
    for required in (
        "  vscode:",
        "npm run lint",
        "npm run compile",
        "npm run unit",
        "xvfb-run -a npm test",
        "npm run package -- --out ocomment.vsix",
        "name: ocomment-vsix",
    ):
        if required not in ci:
            failures.append(f"VS Code CI is missing {required!r}")

    release = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    for required in (
        "metadata:",
        "draft-release:",
        "publish-container:",
        "publish-crates:",
        "finalize:",
        "environment: release",
        "--draft --generate-notes --verify-tag",
        "python3 tools/release_metadata.py",
    ):
        if required not in release:
            failures.append(f"release workflow is missing {required!r}")
    for forbidden in (
        "build-vscode:",
        "publish-vscode-marketplace:",
        "publish-open-vsx:",
        "vscode-marketplace",
        "editors/vscode",
        ".vsix",
        "VSCE_PAT",
        "OVSX_PAT",
    ):
        if forbidden in release:
            failures.append(f"CLI release workflow still contains {forbidden!r}")
    finalize = re.search(
        r"(?ms)^  finalize:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)", release
    )
    if finalize is not None:
        needs = re.search(
            r"(?m)^    needs:\n(?P<items>(?:^      - [^\n]+\n)+)",
            finalize.group("body"),
        )
        actual_needs = (
            re.findall(r"(?m)^      - ([^\n]+)$", needs.group("items"))
            if needs is not None
            else []
        )
        if actual_needs != ["publish-container", "publish-crates"]:
            failures.append(
                "release finalize must depend only on publish-container and publish-crates"
            )
    if "  draft-release:\n    needs: build\n" not in release:
        failures.append("draft release must depend only on the CLI archive build")
    if 'chmod 0755 "release/binaries/out/$2/ocomment"' not in release:
        failures.append("release workflow does not preserve archive executable mode")

    release_pr = (ROOT / ".github/workflows/release-pr.yml").read_text(encoding="utf-8")
    for required in (
        "release-plz release-pr",
        "--config release-plz.toml",
        "--manifest-path rust/Cargo.toml",
        "release-plz-v0.3.160",
        "2263c4f95eac1513da96a114a77fde20ea038742a8c8050f7514b8f93b828646",
        "python3 tools/sync_release_docs.py",
        "gh workflow run ci.yml",
        "gh workflow run docs.yml",
        "gh workflow run codeql.yml",
        "dispatch-checks:",
        "actions: write # NOTE: Dispatch only",
    ):
        if required not in release_pr:
            failures.append(f"Release PR automation is missing {required!r}")
    for forbidden in ("release-plz release ", "CARGO_REGISTRY_TOKEN", "cargo publish"):
        if forbidden in release_pr:
            failures.append(f"Release PR automation must not contain {forbidden!r}")
    prepare_job = re.search(
        r"(?ms)^  prepare:\n(?P<body>.*?)(?=^  dispatch-checks:\n)", release_pr
    )
    if prepare_job is None or "actions: write" in prepare_job.group("body"):
        failures.append("the Release PR preparation job must not receive actions: write")

    with (ROOT / "release-plz.toml").open("rb") as stream:
        release_plz = tomllib.load(stream)
    release_workspace = release_plz.get("workspace", {})
    for field in ("publish", "git_tag_enable", "git_release_enable"):
        if release_workspace.get(field) is not False:
            failures.append(f"release-plz must set workspace.{field} = false")
    if release_workspace.get("git_tag_name") != "v{{ version }}":
        failures.append("release-plz must recognize the repository's single vVERSION tag")
    configured_packages = release_plz.get("package", [])
    package_names = [package.get("name") for package in configured_packages]
    expected_packages = ["ocomment-core", "ocomment-plugin-sdk", "ocomment"]
    if package_names != expected_packages:
        failures.append(
            "release-plz must manage exactly the three product crates in dependency order"
        )
    if any(package.get("version_group") != "ocomment" for package in configured_packages):
        failures.append("all release-plz packages must share the ocomment version group")
    cli_release = next(
        (package for package in configured_packages if package.get("name") == "ocomment"),
        {},
    )
    if cli_release.get("changelog_path") != "../CHANGELOG.md":
        failures.append("release-plz must update only the root CLI CHANGELOG.md")
    if cli_release.get("changelog_include") != ["ocomment-core", "ocomment-plugin-sdk"]:
        failures.append("the CLI changelog must include core and plugin SDK changes")

    action = (ROOT / "action.yml").read_text(encoding="utf-8")
    for required in (
        "verify-attestation is true but the gh CLI is unavailable",
        "Validate the OComment result",
        "not a SARIF 2.1.0 document",
        "must not contain a line break",
        "printf '%s<<%s",
    ):
        if required not in action:
            failures.append(f"composite action is missing {required!r}")

    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"{len(PINS)} reviewed action pins and CI/release contracts match")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
