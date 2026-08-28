# Hooks and CI

OComment ships two integrations: a [pre-commit](https://pre-commit.com) hook
manifest at `.pre-commit-hooks.yaml`, and a composite GitHub Action at
`action.yml`. Both drive the same CLI and the same exit codes: `0` clean, `1`
removable comments exist, `2` an invalid source, configuration, plugin, or I/O
failure.

## pre-commit

### Install the CLI first

The hooks declare `language: system`, so `ocomment` must already be on `PATH`
when pre-commit runs them. pre-commit's `language: rust` runs
`cargo install --path .` at the checkout root, and this repository's manifest
lives in `rust/`, so it cannot build these hooks. Install the CLI once per
machine and CI image:

```sh
cargo install ocomment --locked
```

A future release may publish a wheel so `language: python` can install the
binary itself. Until then, a missing `ocomment` fails the hook with a "command
not found" error rather than silently passing.

### Recommended configuration

```yaml
repos:
  - repo: https://github.com/P4suta/OComment
    rev: v0.1.0
    hooks:
      - id: ocomment-check
```

`ocomment-check` reports removable comments in the staged source files and
exits 1, which blocks the commit and leaves the fix to you. That is the safe
default: nothing is rewritten behind your back.

To rewrite instead of reporting, use `ocomment-fix`. Run it *before*
`ocomment-check` so the check confirms the result:

```yaml
repos:
  - repo: https://github.com/P4suta/OComment
    rev: v0.1.0
    hooks:
      - id: ocomment-fix
      - id: ocomment-check
```

Both hooks accept the full CLI surface through `args`, for example
`args: ["--policy", "legal"]` or `args: ["--config", "ci/.ocomment.toml"]`.

### Judging the commit rather than the disk

pre-commit passes the staged file names to the hook and stashes unstaged
changes before running it, so by default OComment reads the working tree that
pre-commit has already reduced to the staged content. Add `--staged` to read
the Git index blobs directly — the exact bytes the commit will contain:

```yaml
      - id: ocomment-check
        args: ["--staged"]
```

For a partially staged file the difference is visible: the working tree shows
every comment, the index shows only the ones being committed.

```console
$ ocomment check a.rs
a.rs:2:16: removable line comment: // staged comment
a.rs:3:16: removable line comment: // unstaged comment
Found 2 removable comments in 1 file (1 file scanned). Run `ocomment fix` to remove them.

$ ocomment check --staged a.rs
a.rs:2:16: removable line comment: // staged comment
Found 1 removable comment in 1 file (1 file scanned). Run `ocomment fix` to remove it.
```

Two caveats come with `--staged`, and both are worth knowing before you enable
it.

**`fix --staged` rewrites the index and the working tree together, so
pre-commit does not notice.** pre-commit decides that "files were modified by
this hook" by comparing the unstaged diff before and after the hook. After
pre-commit's stash the working tree already equals the index, and
`ocomment fix --staged` moves both sides by the same edits, so the unstaged
diff is empty both before and after:

```console
$ git status --short
M  a.rs                     # staged, working tree clean
```

The detection therefore does not fire, and the commit proceeds with the
removals already staged. If you want the commit stopped so you can look at the
result, keep `ocomment-fix` without `--staged` — that rewrites only the working
tree, leaves an unstaged diff, and pre-commit fails the commit — or follow it
with `ocomment-check --staged`.

Outside pre-commit, where a file really is partially staged, `fix --staged`
refuses rather than guessing:

```console
$ ocomment fix --staged a.rs
ocomment: unstaged changes in a.rs make the staged fix ambiguous; no files were
modified (use --index-only): edit context does not have one unique working-tree
mapping
```

**`--staged` sees nothing outside the `pre-commit` stage.** Under
`pre-commit run --all-files`, or in a `pre-push` or `manual` stage, there is no
staged change set, so the run scans zero files and exits 0:

```console
$ ocomment check --staged
No removable comments in 0 files.
```

That is a hook which always passes, not a hook which found nothing. Use a
separate entry without `--staged` for those stages, or gate the `--staged`
entry with `stages: [pre-commit]`.

### Keeping the hook manifest honest

The `files:` pattern in `.pre-commit-hooks.yaml` is generated from the
extensions in `spec/languages.toml`, so a new language cannot leave the hooks
scanning the old file set. `tools/check_hooks.py` regenerates the pattern and
fails on any drift; it also rejects a manifest key pre-commit does not define
and a hook missing `id`, `name`, `entry`, or `language`, because pre-commit
itself only complains when a consumer runs the hook. CI runs it next to
`tools/check_embedded_specs.py`.

```sh
python3 tools/check_hooks.py                  # NOTE: fail on drift
python3 tools/check_hooks.py --print-pattern  # NOTE: the regex the hooks must carry
```

## GitHub Action

`action.yml` at the repository root is a composite action. It resolves a
release, downloads the archive for the runner, verifies its SHA-256 and its
build provenance, runs `ocomment check` or `ocomment diff`, and turns the exit
code into a verdict.

### Annotate a pull request

`format: github` is the default and writes `::notice` annotations that GitHub
renders on the changed lines.

```yaml
name: Comments
on: [pull_request]

permissions:
  contents: read

jobs:
  ocomment:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: P4suta/OComment@v0.1.0
        with:
          paths: src tests
```

### Upload SARIF to code scanning

```yaml
jobs:
  ocomment:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write # NOTE: Upload the SARIF file to code scanning.
    steps:
      - uses: actions/checkout@v7
      - uses: P4suta/OComment@v0.1.0
        with:
          format: sarif
          upload-sarif: "true"
          fail-on-findings: "false" # NOTE: Let the code-scanning alerts carry the result.
```

`upload-sarif: "true"` requires `format: sarif`; any other format is a usage
error rather than a silent skip. The SARIF is uploaded under the `ocomment`
category, so it does not collide with other tools' results.

### Inputs

| Input | Default | Meaning |
| --- | --- | --- |
| `version` | `""` | Release tag to download. Empty uses the tag the action was referenced by when that looks like a version, and otherwise the latest release. |
| `command` | `check` | `check` or `diff`. |
| `paths` | `""` | Files or directories, split on whitespace. Empty processes the working directory. |
| `policy` | `""` | Value for `--policy`. Empty leaves the configured policy alone. |
| `format` | `github` | Value for `--format`. |
| `args` | `""` | Extra arguments, split on whitespace. |
| `fail-on-findings` | `"true"` | Fail the step on exit 1. Exit 2 always fails. |
| `upload-sarif` | `"false"` | Upload the SARIF file to code scanning. |
| `sarif-file` | `ocomment.sarif` | Where SARIF output is written. |
| `verify-attestation` | `"true"` | Run `gh attestation verify` on the archive when `gh` is available. |
| `binary-path` | `""` | Use an already-built binary and download nothing. |
| `working-directory` | `.` | Directory the command runs in. |
| `token` | `${{ github.token }}` | Used to resolve the latest release and verify attestations. |

`paths` and `args` are split on whitespace with globbing disabled; quoting
inside them is not interpreted, so a path containing a space needs a separate
run or a `--config` file.

### Outputs

| Output | Meaning |
| --- | --- |
| `exit-code` | `0` clean, `1` removable comments, `2` failure. |
| `version` | Release tag downloaded, or the version the supplied binary reported. |
| `sarif-file` | Absolute path of the SARIF file, empty when `format` is not `sarif`. |

`fail-on-findings: "false"` keeps the step green on exit 1 so a later step can
branch on `exit-code`:

```yaml
      - id: comments
        uses: P4suta/OComment@v0.1.0
        with:
          fail-on-findings: "false"
      - if: steps.comments.outputs.exit-code == '1'
        run: echo "Removable comments are present but not blocking."
```

### What the action verifies

Every downloaded archive is checked against the release `SHA256SUMS` before it
is unpacked, and the run stops with exit 2 on a mismatch or on an archive that
the checksum file does not list. With `verify-attestation: "true"` — the
default — the archive is also checked against its GitHub build-provenance
attestation with `gh attestation verify --repo P4suta/OComment`. Runners
without the `gh` CLI log a warning and continue; runners with it fail the step
when the attestation does not verify.

Runner platforms map to the published targets as follows. Linux uses the
statically linked musl archives, so no glibc version is required.

| Runner | Target | Archive |
| --- | --- | --- |
| Linux x64 | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Linux arm64 | `aarch64-unknown-linux-musl` | `.tar.gz` |
| macOS x64 | `x86_64-apple-darwin` | `.tar.gz` |
| macOS arm64 | `aarch64-apple-darwin` | `.tar.gz` |
| Windows x64 | `x86_64-pc-windows-msvc` | `.zip` |

Any other combination fails with exit 2 and points at `binary-path`.

### Runners without a published archive

`binary-path` skips resolution and download entirely and uses a binary you
already have. A missing path is retried with an `.exe` suffix, so one value
works across the runner matrix. This is how the repository's own
`action-smoke` job tests the action against a freshly built CLI:

```yaml
      - run: cargo build --manifest-path rust/Cargo.toml --locked -p ocomment
      - uses: ./
        with:
          binary-path: rust/target/debug/ocomment
          paths: action-fixture
```

### Pinning

Version tags are immutable under the repository's release-tag ruleset, so
`P4suta/OComment@v0.1.0` is a stable reference and there is no moving `v0` tag
to follow. Pin to a full version, or to a commit SHA with a version comment if
your policy requires it.

## Keeping the protected directives honest

`spec/directives.toml` publishes the markers that take a comment out of reach
of a `remove` policy — `# syntax=`, `//go:build`, `# hadolint ignore=`, and the
rest — so a consumer can read the contract without reading the scanner.
`tools/check_directives.py` is what keeps the two the same thing. It feeds
every name to the built binary as the comment a project would really write, and
the answer has to be a `keep` with the reason that says why; a name in the spec
with no sample fails, and so does a sample the spec does not list.

Each sample carries two comments the scanner has to remove: an ordinary one,
which catches a run that protected the whole file, and a near-miss derived from
the name — `hadolint` against `hadolintish note` — which catches a marker
matched so loosely that prose merely opening with those letters is protected
too.

```sh
cargo build --manifest-path rust/Cargo.toml --locked -p ocomment
python3 tools/check_directives.py
python3 tools/check_directives.py --binary rust/target/release/ocomment
```

The `rust` CI job runs it next to `tools/check_hooks.py` and
`tools/check_embedded_specs.py`, and `tools/release-check.sh` runs it again
against the release binary before a tag is pushed.
