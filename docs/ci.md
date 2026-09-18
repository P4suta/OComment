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
`args: ["--policy", "standard"]` or `args: ["--config", "ci/.ocomment.toml"]`.

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

Both published hooks use `types: [text]` and intentionally have no `files:`
regex. Pre-commit selects the text files and OComment's own detector decides
which ones it understands. That keeps reserved names such as `Dockerfile` and
extensionless shebang scripts on the same path as an ordinary CLI run.
`tools/check_hooks.py` rejects any manifest-level `files:` filter, an unknown
manifest key, or a hook missing `id`, `name`, `entry`, or `language`. CI runs it
next to `tools/check_embedded_specs.py`.

```sh
python3 tools/check_hooks.py
```

## GitHub Action

`action.yml` at the repository root is a composite action. It resolves a
release, downloads the archive for the runner, verifies its SHA-256 and its
build provenance, runs `ocomment check` or `ocomment diff`, and turns the exit
code into a verdict.

### Annotate a pull request

`format: github` is the default and writes annotations that GitHub renders on
the changed lines.

The level of each one is the level its run's exit status justifies: `check`
and `diff` answer a finding with exit 1, so what they report is an `::error`,
while `scan` and `fix` end at 0 whatever they find and report a `::notice`.
That way a job which fails on the 1 does not describe the comments it failed
over as though nothing had gone wrong — and GitHub folds a notice away where
it surfaces an error, so the annotation was easy to miss entirely.

A job that posts annotations without gating on them, or gates without wanting
the red, says so with `--annotation-level <error|warning|notice>` and is
believed. A diagnostic — a file that would not scan at all — stays an
`::error` whatever that flag says, because it is not a finding the run is
offering an opinion about.

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
| `verify-attestation` | `"true"` | Run `gh attestation verify`; the action fails closed when `gh` is unavailable. Set `false` only as an explicit opt-out. |
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
attestation with `gh attestation verify --repo P4suta/OComment`. A runner
without the `gh` CLI fails closed; `verify-attestation: "false"` is the only
explicit opt-out. The action also validates SARIF structure before invoking the
upload action, and reports a CLI exit 2 before any upload failure can obscure
it. Inputs and binary version output containing line breaks are rejected before
they can become workflow outputs.

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
`tools/check_embedded_specs.py`, and `cargo xtask release-check` runs it again
against the release binary before a tag is pushed.

## Putting this on a repository that already exists

A repository with eleven thousand comments cannot turn the rule it wants on
today. The order below tightens one axis at a time, and each step leaves a gate
that passes.

**1. Find out what is there.** `ocomment coverage` says which files were read
and which were passed over; a gate over 85% of a tree is not the gate you
think it is, so close that first with `[files]` and, where a format has no
built-in scanner, a `[profiles.<name>]` entry. `ocomment tags` says which tags
your comments already open with — that list, not a list you invent, is the one
to start `[policy.allow] tags` from.

**2. Protect what your own tools read.** `[policy] protected` names the markers
your build, your linter or your test runner reads. This is the step that has to
come before any removal, because it is the only one whose omission changes what
the code *does*. `ocomment scan --policy all --explain` over a directory you
know well is a quick way to find what you have been relying on.

**3. Gate the new work, not the old.** `ocomment check --base main` in a pull
request checks only what the branch changed. The tree stays as it is and
nothing new is added to it, which is most of the value and costs no cleanup at
all.

**4. Record the distance, and close it.** `[ratchet] ledger` counts what each
file holds today and fails when a file holds more — and when it holds fewer,
asking to be updated, so the number in the file is always the number in the
tree. See [the configuration guide](configuration.md#getting-to-a-rule-you-cannot-turn-on-today).

**5. Tighten one axis.** `max_lines`, then `trailing = false`, then deadlines on
the tags that are promises. Each is a separate number the ledger can carry to
zero. `ocomment check --format agent` is the report to hand somebody — or
something — that is going to do the editing.

**6. Drop the ledger.** When it reaches zero, delete it and make the bare run
the gate. A ledger that has nothing left to say is a file that describes a
repository that no longer exists.

## What the gate never looked at

```console
$ ocomment coverage
1 of 3 files scanned (33.3%)
2: hidden file or directory ([files] hidden = false)
       1  .yml
       1  .toml
```

The percentage is of the tree and not of the walk. A file the walk *reached*
and passed over is a skip and has always been reported; a file the walk's own
limits kept out was met by nothing, so nothing reported it — and `hidden =
false` is the default, which means every `.github/workflows/*.yml` a project
has. A run that read one of three files used to say `100.0%`, which was a true
sentence about the walk and a false assurance about the repository.

Three settings can keep a file out, and each is named with the line a reader
would change: `[files] hidden`, `[files] include`/`exclude`, and
`[files] max_size`. A file a `.gitignore` excludes is deliberately not counted
— that is build output, and a percentage taken over a hundred thousand object
files would mean nothing.

## Gating a branch on what it changed

```console
$ ocomment check --base main
```

Only the working-tree files that differ from `git merge-base HEAD main`. The
merge base and not the branch tip: on a branch several commits behind its
trunk, a plain diff against the trunk reports every file the trunk changed as
well, and a gate that reported those would be asking this branch to answer for
somebody else's work. A deleted file is dropped rather than reported — there is
nothing left to read, and failing on one would refuse the change that cleaned
it up.

A path named beside it narrows it further: `--base main src` is the files under
`src` that the branch changed. `--base` applies the ordinary walk limits, so a
generated file the branch touched is still passed over; a path typed on the
command line without `--base` is you saying *this one* and lifts them.

### A gate that examined nothing says so

```
--base main: no changed files to check, so nothing was examined.
--staged: nothing is staged, so nothing was examined. A runner that stages
nothing of its own -- `pre-commit run --all-files`, say -- needs a run without
--staged.
```

Both runs are correct and both exit 0, which reads exactly like a clean branch.
That is how `--staged` under `pre-commit run --all-files` becomes a gate that is
green forever. The run stays right; the silence goes.

## Numbers a later step can read

`--summary <FILE>` writes the end-of-run counts as one JSON object, whatever
`--format` the run wrote its product in:

```console
$ ocomment check --format sarif --summary counts.json > ocomment.sarif
$ jq .removable_comments counts.json
14
```

`spec/summary.schema.json` is the schema. The counts are the ones the run
already made, so there is no second scan to pay for and no parsing of the
product to get at them — and `comments_removed` is non-zero only for a `fix`
that reached the disk.

The GitHub Action uses it for its own outputs. `findings-count`,
`files-with-findings`, `files-scanned`, `removed-count` and `summary-file` are
available to later steps, and the job summary carries a table of the same
numbers unless `step-summary: false`:

```yaml
- uses: P4suta/OComment@v0
  id: comments
- if: steps.comments.outputs.findings-count != '0'
  run: echo "still ${{ steps.comments.outputs.findings-count }} to go"
```

A run that failed before it finished reports those outputs as **empty** rather
than as zero: "none found" and "never looked" are different answers, and a gate
downstream must not read the second as the first.

## Threads

`--jobs <N>` sets how many threads the run uses to walk, read and scan; `0`
chooses one per core, which is the default. The walk, the reads and the scans
all take it from the same place, so one flag is the whole knob. It was
previously settable only through `RAYON_NUM_THREADS`, which is an
implementation detail leaking as a user interface.

Output order does not depend on it. The candidates a walk finds are sorted
before any of them is opened, so two runs over the same tree write the same
bytes however many threads they used.

## The published pre-commit hooks

`.pre-commit-hooks.yaml` is what pre-commit reads when this repository is used
as a `repo:` entry. Both hooks deliberately receive every text file pre-commit
selects: OComment's detector, not a second extension list, decides which files
are supported, and that is what lets reserved names and extensionless shebang
scripts reach the same detector an ordinary CLI run uses.
`tools/check_hooks.py` rejects a manifest-level filter that would undo it.

`language: system` requires `ocomment` to already be on `PATH`. pre-commit's
`language: rust` runs `cargo install --path .` at the checkout root, and this
repository's manifest lives in `rust/`, so it cannot build these hooks.

## The YAML round trip

The one invariant no byte-level fixture can state: a YAML block scalar reads
the lines below it, so the hole a removal leaves on a comment's line can be
read back as part of a value. `tools/yaml_roundtrip.py` strips thousands of
generated documents under every layout and every policy and asks a real YAML
parser whether they still mean the same thing.

The corpus and both enumerated sweeps run in full in CI: they are where the
hazard lives, and they are the same documents on every run. Only the
pseudo-random set is cut there, because its cost is linear and its value is
not — `python3 tools/yaml_roundtrip.py` runs the whole 2400 on demand, and
`--seed` moves it. Every pass is one `fsync` per rewritten file, so the tool
overlaps them rather than waiting on them in turn.
