# Contributing to OComment

Thank you for helping improve OComment. Bug reports, language fixtures,
documentation, performance data, and code changes are all welcome.

## Before opening a change

- Use [GitHub Discussions](https://github.com/P4suta/OComment/discussions) for
  design questions and support.
- Use an issue for confirmed bugs and scoped feature requests.
- Report vulnerabilities through the private process in [SECURITY.md](SECURITY.md).

Substantial scanner, policy, public API, Git, LSP, or plugin-contract changes
should have an agreed design before implementation. Small fixes can go directly
to a pull request.

## Development setup

The production workspace requires Rust 1.88 or newer. Differential verification
also requires OCaml 5.5, opam, Dune 3.24.2, Python 3, and the dependencies from
`ocaml/ocomment-ref.opam`.

```sh
opam install ./ocaml/ocomment-ref.opam --deps-only --with-test
cargo build --manifest-path rust/Cargo.toml --workspace --locked
lefthook install
```

`lefthook install` wires up `lefthook.yml`, whose `pre-commit` hook runs
`ocomment check --staged` and `cargo fmt --check`. The hook reads the staged
blobs rather than the working tree, so a partially staged file is judged by the
bytes the commit will carry, and it reports rather than rewrites: `fix --staged`
under Lefthook would need `stage_fixed`, which stages the whole working-tree
file and destroys partial staging. It prefers an `ocomment` on `PATH` and falls
back to the workspace copy, so a fresh clone needs no `cargo install` first.

The repository is intentionally split into independent implementations:

- `spec/` contains shared contracts and fixtures.
- `rust/` contains the product, public library, LSP server, and plugin host.
- `ocaml/` contains the independent reference implementation.

Do not share scanner code between Rust and OCaml. Matching normalized outputs
are the cross-check.

## Required checks

Run the checks relevant to your change; scanner or policy changes should run all
of them.

```sh
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo test --manifest-path rust/Cargo.toml --workspace --all-targets --locked
opam exec -- dune runtest --root ocaml
opam exec -- ./tools/differential.sh
python3 tools/check_embedded_specs.py
python3 tools/check_hooks.py
python3 tools/check_directives.py
python3 tools/validate_schemas.py
./tools/package-list.sh
ocomment
actionlint
lefthook validate
```

A bare `ocomment` from the repository root is the gate CI runs; see
[comments carry a tag](#comments-carry-a-tag).

When behavior changes, add the smallest fixture that proves the lexical edge
case. Keep byte spans half-open, edits sorted and non-overlapping, and output
deterministic. Update both implementations and their differential expectations
when the shared contract changes.

## Comments carry a tag

OComment checks its own repository. `.ocomment.toml` runs the `legal` policy
with `doc-line` and `doc-block` kept, so documentation is never at risk, and it
keeps any comment whose first word is one of these tags:

| Tag | What it introduces |
| --- | --- |
| `NOTE` | Why the code is the way it is, where the code cannot say so itself. |
| `SAFETY` | Why an `unsafe` block upholds what the compiler cannot check. It is reserved for that Rust-wide meaning; the workspace has no `unsafe` today. |
| `INVARIANT` | A property the surrounding code must preserve for the next reader to be able to change it — why an operation cannot lose a user's bytes or be spoofed included. |
| `PERF` | A measurement or a hot path that explains a shape which would otherwise look convoluted. |
| `TODO` / `FIXME` / `HACK` | Work that is known to be left, in decreasing order of how deliberate it was. |

An untagged explanatory comment fails the `dogfood` CI job and the pre-commit
hook. The rule is not that comments are unwelcome — it is that a comment worth
keeping is worth saying *why* it is there, and a comment that cannot be given
one of these tags is usually restating what the line below it already says.

The tag is matched against the head of a single comment token, so a rationale
that runs past one line is one block comment rather than a run of `//` lines:

```rust
/* INVARIANT: a Rust string literal carries a bare newline as content, unlike
 * its C, Go, and Java cousins, so only the closing quote or the end of the
 * file ends one. */
```

Keep the continuation lines on ` * `; `rustfmt` reflows the other block-comment
shapes. In OCaml the same rationale is `(* INVARIANT: ... *)`.

A language whose only comment is `#` — shell, Python, the Dockerfile — has no
block form to run a rationale through, and every `#` line is a comment token of
its own, so every one of them carries the tag:

```dockerfile
# NOTE: musl-dev is deliberately unpinned: the version that matters is the one
# NOTE: the pinned `rust:1.88-alpine` tag resolves to, and pinning a package
# NOTE: version on top of that only breaks the build when the base image moves.
```

Prose that documents a Python object belongs in its docstring instead, which is
not a comment at all.

`ocomment config explain` names the setting behind each of these rules, and
`ocomment --explain` names the rule that decided any one comment.

## CLI output conventions

Findings, patches, generated files, and every machine format are written to
standard output; run summaries, progress, and notes are written to standard
error, so `ocomment diff > fix.patch` and `--format json | jq` stay clean. A
machine format writes nothing to standard error but errors and diagnostics.
Every write to standard output goes through `output::wrote(...)`, which tags a
lost reader as `OutputPipeClosed` so the run ends quietly instead of reporting
an unexplained broken pipe; `rust/ocomment/tests/source_guards.rs` enforces
that. Name a language, dialect, comment kind, policy, layout, or disposition
through its `as_str()` and never through `Debug`: the canonical spellings are
kebab-case (`doc-block`, `html-comment`) and are shared with the human, JSON,
JSONL, SARIF, and GitHub output. All user-facing text is English.

## Pull requests

- Keep each pull request focused and explain compatibility or safety effects.
- Add tests for observable behavior and update user-facing documentation.
- Regenerate checked-in schemas, WIT, man pages, or completions when their source
  changes; `tools/check_embedded_specs.py` checks shared embedded assets.
- Adding a language to `spec/languages.toml` also changes the published
  pre-commit hooks; `tools/check_hooks.py --print-pattern` regenerates the
  `files:` regex that `.pre-commit-hooks.yaml` must carry. It changes what this
  repository checks about itself as well: a file the new scanner now reads is a
  file whose comments have to carry a tag, so run a bare `ocomment` before
  opening the change. Two more things count the languages rather than reading
  the table: `MINIMUM_CASES` in `tools/differential.py`, the floor that stops a
  later change from quietly dropping the fixtures the language brings, and the
  editor clients — `editors/vscode/package.json` lists the identifiers the
  extension attaches to, and `docs/editors.md` names and counts them.
- Adding a marker to `spec/directives.toml` needs two samples: one in
  `tools/check_directives.py`, which proves the scanner protects it — and,
  through a near-miss derived from the name, that it protects nothing more —
  and one in `PROTECTED_SAMPLES` of `tools/gen_docs.py`, which is the row the
  published table of protected markers is generated from. Each is checked
  against `spec/directives.toml` on its own, so a marker with only one of the
  two fails the run that needs the other. Both run the binary where this
  repository's own `.ocomment.toml` applies, so write the near-miss as a comment
  kind that configuration does not keep: a `doc-line` one comes back kept for a
  reason that has nothing to do with the marker under test.
- A new CI job must be added to `.github/rulesets/main.json` in the same change,
  and every `uses:` must be SHA-pinned with a version comment.
- Do not include build output, credentials, or unrelated formatting changes.

The repository uses squash merges. By submitting a contribution, you agree that
it is licensed under either MIT or Apache-2.0, at the user's option.
