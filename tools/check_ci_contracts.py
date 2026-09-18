#!/usr/bin/env python3
"""Enforce the security and release-DAG contracts of repository automation."""

from __future__ import annotations

import pathlib
import re
import subprocess
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
    "rust-lang/crates-io-auth-action": (
        "c6f97d42243bad5fab37ca0427f495c86d5b1a18",
        "v1.0.5",
    ),
    "sigstore/cosign-installer": ("6f9f17788090df1f26f669e9d70d6ae9567deba6", "v4.1.2"),
    "taiki-e/upload-rust-binary-action": (
        "f0d45ae91ee7b8ee928de7a9d04d893a08bcbec6",
        "v1.30.2",
    ),
}
USES = re.compile(r"^\s*-?\s*uses:\s*([^@\s]+)@([^\s#]+)(?:\s+#\s*(.*))?$", re.MULTILINE)


SHELL_KEPT = frozenset({"tools/publish-crates.sh"})


def refuse_shell_scripts(found: set[str]) -> list[str]:
    """Complain about any of `found` that is not the release workflow's script.

    A task runner is code, and the code that decides what a gate does should be
    read and typed by the same toolchain as what it gates -- and a shell step is
    the one thing here that would not survive the Windows job it stands in for.
    `cargo xtask` is where a new one goes.

    Takes the set rather than looking it up, so the negative control below can
    hand it any tree at all. A rule that can only be asked about the tree it is
    standing in is a rule that can only be watched agreeing.
    """
    return [
        f"{shell} is a shell script; add a `cargo xtask` task instead"
        for shell in sorted(found - SHELL_KEPT)
    ]


def shell_scripts_here() -> set[str]:
    """Every `tools/*.sh` the repository actually holds."""
    return {str(path.relative_to(ROOT)) for path in ROOT.glob("tools/*.sh")}


def self_test_shell_rule() -> int:
    """Watch the shell rule refuse something, which the tree never makes it do.

    A rule whose subject has been removed reports `ok` for the same reason an
    empty room is quiet, and that is not the gate working. Both directions are
    asked here, of a tree made up for the purpose.
    """
    if not refuse_shell_scripts({"tools/reintroduced.sh"}):
        print("the shell rule did not object to a new shell script", file=sys.stderr)
        return 1
    if refuse_shell_scripts(set(SHELL_KEPT)):
        print("the shell rule objects to the release script it exempts", file=sys.stderr)
        return 1
    return 0


def members_missing_workspace_lints(manifests: dict[str, str]) -> list[str]:
    """Complain about a workspace member that does not inherit the lints.

    `[workspace.lints]` does nothing on its own: a member has to opt in with
    `[lints] workspace = true`, and a member that forgets is silently outside
    every rule the workspace states. Three of this repository's four crates had
    opted in and the fourth had not, so neither `missing_docs` nor the
    exhaustive-match rule had ever applied to the CLI.

    Takes the manifests rather than reading them, so the negative control can
    hand it a workspace that is wrong.
    """
    return [
        f"{name} does not inherit the workspace lints;"
        " add `[lints]\nworkspace = true` to its Cargo.toml"
        for name, text in sorted(manifests.items())
        if not re.search(r"^\[lints\]\s*\nworkspace\s*=\s*true", text, re.MULTILINE)
    ]


def member_manifests() -> dict[str, str]:
    """Every workspace member's manifest, by the directory it sits in.

    The member list comes from the workspace manifest rather than from a glob,
    so a directory that is not a member is not asked about and a member that is
    not a directory fails loudly.
    """
    workspace = tomllib.loads((ROOT / "rust/Cargo.toml").read_text(encoding="utf-8"))
    manifests = {}
    for member in workspace["workspace"]["members"]:
        path = ROOT / "rust" / member / "Cargo.toml"
        manifests[member] = path.read_text(encoding="utf-8")
    return manifests


def self_test_lint_rule() -> int:
    """Watch the lint-inheritance rule refuse something.

    Every member inherits today, so the rule reports nothing for the same
    reason an empty room is quiet.
    """
    if not members_missing_workspace_lints({"forgetful": "[package]\nname = 'x'\n"}):
        print("the lint rule did not object to a member that opts out", file=sys.stderr)
        return 1
    if members_missing_workspace_lints({"careful": "[lints]\nworkspace = true\n"}):
        print("the lint rule objects to a member that opts in", file=sys.stderr)
        return 1
    return 0


def run_blocks(text: str) -> list[tuple[int, str]]:
    """Every `run:` block in a workflow, as (line number, body).

    Read by indentation rather than by a YAML parser, because this file has no
    third-party dependency and a `run:` block is the one shape that does not
    need one: the key names the column, and the body is every line past it.
    """
    lines = text.splitlines()
    blocks = []
    index = 0
    while index < len(lines):
        match = re.match(r"^(\s*)(?:- )?run: [|>]", lines[index])
        if match is None:
            index += 1
            continue
        column = len(match.group(1))
        start = index
        index += 1
        body = []
        while index < len(lines):
            line = lines[index]
            if line.strip() and len(line) - len(line.lstrip()) <= column:
                break
            body.append(line)
            index += 1
        blocks.append((start + 1, "\n".join(body)))
    return blocks


def refuse_pipes_without_pipefail(blocks: list[tuple[int, str]], where: str) -> list[str]:
    """Complain about a `run:` block that pipes without `set -o pipefail`.

    A workflow's default shell is `bash -e {0}`, which is not `pipefail`, so
    `a | tee log` reports whatever `tee` did and the failure of `a` is lost.
    This repository pipes `ocomment fix` into `tee` in the step that strips
    every comment from a copy of the workspace: without `pipefail` that step
    would carry on after a failed rewrite and build whatever was left.

    Takes the blocks rather than reading them, so the negative control can hand
    it a workflow that is wrong.
    """
    failures = []
    for number, body in blocks:
        piped = [
            line
            for line in body.splitlines()
            if re.search(r"[^|]\|[^|]", line)
            and not line.strip().startswith(("#", "*", '"', "true|", "check|"))
            and "=>" not in line
            and "| ---" not in line
            and "| |" not in line
        ]
        if piped and "pipefail" not in body:
            failures.append(
                f"{where}:{number}: a `run:` block pipes without `set -o pipefail`,"
                f" so a failure on the left of the pipe is lost: {piped[0].strip()[:60]}"
            )
    return failures


def refuse_a_shell_that_is_not_bash(text: str, where: str) -> list[str]:
    """Complain about a `shell:` that is neither `bash` nor `pwsh`.

    `shell: sh` is not a smaller `bash`, it is a different program: on Ubuntu it
    is dash, and dash has no `$'...'`. `action.yml` rejects an input holding a
    line break with `case "$2" in *$'\n'*)`, which is what stops a value from
    writing extra lines into `GITHUB_OUTPUT`. Under dash that pattern matches
    nothing and the check accepts the value instead of rejecting it -- measured,
    with no error and no message.

    So the shell named in a composite step is load-bearing for a security check,
    and a change from `bash` to `sh` would look like tidying. `pwsh` is allowed
    because the two Windows steps that use it run no shell fragment of ours.
    """
    failures = []
    for number, line in enumerate(text.splitlines(), start=1):
        match = re.match(r"\s*shell:\s*(\S+)\s*$", line)
        if match is None or match.group(1) in ("bash", "pwsh"):
            continue
        failures.append(
            f"{where}:{number}: `shell: {match.group(1)}` -- only `bash` and `pwsh`"
            " are reviewed here, and `sh` is dash on Ubuntu, where the line-break"
            " check in `action.yml` silently accepts what it exists to reject"
        )
    return failures


def self_test_shell_name_rule() -> int:
    """Watch the shell-name rule refuse something the automation never says."""
    if not refuse_a_shell_that_is_not_bash("      shell: sh\n", "made-up.yml"):
        print("the shell-name rule did not object to `shell: sh`", file=sys.stderr)
        return 1
    if refuse_a_shell_that_is_not_bash("      shell: bash\n", "made-up.yml"):
        print("the shell-name rule objects to `shell: bash`", file=sys.stderr)
        return 1
    return 0


def self_test_pipefail_rule() -> int:
    """Watch the pipefail rule refuse something; the workflows never make it."""
    bad = [(1, '          set -eu\n          cargo test | tee log\n')]
    if not refuse_pipes_without_pipefail(bad, "made-up.yml"):
        print("the pipefail rule did not object to an unguarded pipe", file=sys.stderr)
        return 1
    good = [(1, '          set -euo pipefail\n          cargo test | tee log\n')]
    if refuse_pipes_without_pipefail(good, "made-up.yml"):
        print("the pipefail rule objects to a guarded pipe", file=sys.stderr)
        return 1
    return 0


def main() -> int:
    # NOTE: Asked of every run rather than behind a flag. A negative control
    # NOTE: nobody remembers to ask for is a negative control that stops
    # NOTE: happening, and this one costs nothing.
    self_tests = (
        self_test_shell_rule,
        self_test_lint_rule,
        self_test_pipefail_rule,
        self_test_shell_name_rule,
    )
    if any(self_test() != 0 for self_test in self_tests):
        return 1
    failures = []
    seen = set()
    for path in AUTOMATION:
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()
        for line_number, line in enumerate(lines, start=1):
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
            # NOTE: Beside the line or on the line above it. This repository's
            # NOTE: own `[policy.allow] trailing = false` forbids the first
            # NOTE: spelling, so the annotation moved; what has to hold is that
            # NOTE: the pin carries the note, not where the note sits.
            above = lines[line_number - 2].strip() if line_number >= 2 else ""
            annotation = comment or (above[1:].strip() if above.startswith("#") else "")
            if expected[1] not in annotation:
                failures.append(
                    f"{path.relative_to(ROOT)}:{line_number}: {action} needs version comment {expected[1]!r}"
                )
    unused = sorted(set(PINS) - seen)
    if unused:
        failures.append(f"reviewed action pin table has unused entries: {', '.join(unused)}")

    # NOTE: Every chapter the book lists has to be a file Git tracks. A page
    # NOTE: that exists only in a working tree builds here and fails in CI,
    # NOTE: which is what happened: a global ignore hid `docs/agents.md` --
    # NOTE: most repositories keep an agent instruction file as private
    # NOTE: scratch -- so `git add` never saw it and `mdbook build` could not
    # NOTE: read the chapter. `.gitignore` un-ignores it now; this is what
    # NOTE: notices the next one before it is pushed.
    summary = (ROOT / "docs/SUMMARY.md").read_text(encoding="utf-8")
    tracked = set(
        subprocess.run(
            ["git", "ls-files", "docs"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.split()
    )
    for chapter in sorted(set(re.findall(r"\]\(([^)#]+\.md)\)", summary))):
        path = f"docs/{chapter}"
        if path not in tracked:
            failures.append(
                f"docs/SUMMARY.md lists {chapter}, which Git does not track"
                f" ({'it is not on disk either' if not (ROOT / path).is_file() else 'it is ignored or unstaged'})"
            )

    # NOTE: Every `tools/*.py` gate CI runs also runs in `cargo xtask preflight`.
    # NOTE: A push that has to wait eight minutes to hear about a stale manual
    # NOTE: page is not a review cycle, and the only way the local sweep stays
    # NOTE: worth trusting is if adding a gate to CI and not to it fails here.
    # NOTE: A job a laptop cannot run -- the OS matrices, Docker, CodeQL, npm --
    # NOTE: is named in `LOCALLY_UNREACHABLE` rather than silently skipped.
    LOCALLY_UNREACHABLE = frozenset({"tools/package_artifacts.py"})
    workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    preflight = (ROOT / "rust/xtask/src/main.rs").read_text(encoding="utf-8")
    for tool in sorted(set(re.findall(r"tools/[a-z_]+\.py", workflow))):
        if tool in LOCALLY_UNREACHABLE:
            continue
        if tool not in preflight:
            failures.append(
                f"{tool} runs in CI and not in `cargo xtask preflight`, so a push"
                " cannot be trusted to pass"
            )

    # NOTE: No standalone shell script but the one the release workflow runs.
    # NOTE: A task runner is code, and the code that decides what a gate does
    # NOTE: should be read and typed by the same toolchain as what it gates --
    # NOTE: and a shell step is the one thing here that does not survive the
    # NOTE: Windows job it stands in for. `cargo xtask` is where a new one goes.
    failures.extend(refuse_shell_scripts(shell_scripts_here()))
    failures.extend(members_missing_workspace_lints(member_manifests()))
    for workflow_path in sorted(ROOT.glob(".github/workflows/*.yml")) + [ROOT / "action.yml"]:
        body = workflow_path.read_text(encoding="utf-8")
        where = str(workflow_path.relative_to(ROOT))
        failures.extend(refuse_pipes_without_pipefail(run_blocks(body), where))
        failures.extend(refuse_a_shell_that_is_not_bash(body, where))

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
    if '"rust/ocomment/src/runtime/**"' not in ci:
        failures.append("strip CI does not protect the upstream-derived internal runtime")
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
    publish_crates = re.search(
        r"(?ms)^  publish-crates:\n(?P<body>.*?)(?=^  finalize:\n)", release
    )
    if publish_crates is None:
        failures.append("release workflow has no publish-crates job")
    else:
        for required in (
            "contents: read",
            "id-token: write",
            "id: crates-io-auth",
            "rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5",
            "CARGO_REGISTRY_TOKEN: ${{ steps.crates-io-auth.outputs.token }}",
        ):
            if required not in publish_crates.group("body"):
                failures.append(
                    f"crates.io Trusted Publishing is missing {required!r}"
                )
    if "secrets.CARGO_REGISTRY_TOKEN" in release:
        failures.append("release workflow must not use a static crates.io token")

    release_pr = (ROOT / ".github/workflows/release-pr.yml").read_text(encoding="utf-8")
    for required in (
        "release-plz release-pr",
        "--config release-plz.toml",
        "--manifest-path rust/Cargo.toml",
        "release-plz-v0.3.160",
        "2263c4f95eac1513da96a114a77fde20ea038742a8c8050f7514b8f93b828646",
        "pr_count=\"$(jq -er '.prs | length'",
        "pr_number=\"$(jq -er '.prs[0].number'",
        "branch=\"$(jq -er '.prs[0].head_branch'",
        "Expected one version-grouped Release PR",
        "python3 tools/sync_release_docs.py",
        "gh workflow run ci.yml",
        "gh workflow run docs.yml",
        "gh workflow run codeql.yml",
        "dispatch-checks:",
        "actions: write",
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
    print(
        f"{len(PINS)} reviewed action pins and CI/release contracts match"
        f" ({len(member_manifests())} workspace members inherit the lints, and"
        f" all {len(self_tests)} self-checking rules were watched refusing one)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
