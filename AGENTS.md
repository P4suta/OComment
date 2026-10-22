# Working on OComment

For an agent *using* OComment, see [`docs/agents.md`](docs/agents.md).
This file is for an agent changing it.

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) — it is the long form of all of this,
and the synchronisation checklists in it are exhaustive where this page is short.
What follows is the shape of the repository and the handful of rules that are easy to break without noticing.

## The shape

| Path | What it is |
| --- | --- |
| `spec/` | The contract. Schemas, the language table, the directive catalogue, the WIT interface, and 501 differential fixtures. Everything else follows it. |
| `rust/ocomment-core/` | The engine. Performs **no I/O** and has no clock; it turns bytes into a report. |
| `rust/ocomment/` | The CLI. Files, Git, plugins, output, hooks — everything with a side effect. |
| `rust/ocomment-plugin-sdk/` | The guest side of the WASM scanner interface. |
| `ocaml/` | An independent reference implementation of the same specification. |
| `tools/` | The checks that hold the above to each other. |
| `editors/`, `action.yml`, `Dockerfile` | Integrations. Each speaks somebody else's protocol and none reaches into the scanner. |

## Five rules

**1. `spec/` is the source of truth, and the copies are checked.** `rust/ocomment/assets/` holds embedded copies of several `spec/` files.
`tools/check_embedded_specs.py` fails when they drift.
Change the canonical file, copy it across, and run the tool.

**2. The Rust and OCaml implementations do not share code.** That is the point: the cross-check is worth something only because the two were written separately.
`cargo xtask differential` runs every fixture through both and requires byte-identical normalised output.
A change to a scanning rule is a change to both, in the same commit.

**3. `ocomment-core` performs no I/O.** Not files, not processes, not the clock.
A rule that needs any of those is decided in the CLI — see `rust/ocomment/src/deadline.rs`, which measures how old a line is and reaches a verdict the core *owns the vocabulary for* (`ShapeRule::Expired`) and never produces.
Keeping the words in one place is what stops the two halves from explaining the same verdict differently.

**4. Standard output carries the product; standard error carries everything else.** `check` writes findings to stdout, the summary to stderr, and `-q` drops the second.
This is a mechanism rather than a convention: `output::Verbosity` is opaque and has no `PartialEq`, so nothing can ask whether a run is quiet — a caller says whether a line is `Detail::Normal` or `Detail::Verbose` and `output::note` decides.
`rust/ocomment/tests/source_guards.rs` reads the crate's own source to keep both halves true, and its file list checks itself against `src/`.

**5. Exhaustive matches, and lists that check themselves.** `Policy::keeps`, `CommentKind::protection`, `subject_to_shape` and every `DispositionExplanation` match are written out in full so that adding a variant fails to compile until somebody classifies it.
Where a test has to hold a list —
the fixture option sweep in `rust/ocomment-core/tests/explain.rs`, the source list in `source_guards.rs` — the list is checked against the thing it is a list of, in both directions.
A list that only grows by hand is a gate that quietly stops covering what it was written for; that has happened here, and it is what `every_option_is_classified` exists to prevent.

## Before you open a change

```sh
cargo test --manifest-path rust/Cargo.toml --workspace
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets -- -D warnings
cargo fmt --all --manifest-path rust/Cargo.toml -- --check
cargo xtask differential
python3 tools/validate_schemas.py
python3 tools/check_embedded_specs.py
python3 tools/check_directives.py --binary rust/target/debug/ocomment
python3 tools/gen_docs.py --binary rust/target/debug/ocomment --check
ocomment                                   # NOTE: this repository under its own gate
```

`lefthook install` wires the last one into `pre-commit`, built from this workspace rather than taken from `PATH` — a tool that gates its own repository has to be the version in that repository.

Changing `--help` text makes the checked-in manual page and the shell completions stale:

```sh
python3 tools/release_extras.py --binary rust/target/debug/ocomment
cp release-extras/ocomment.1 docs/
python3 tools/gen_docs.py --binary rust/target/debug/ocomment
```

## This repository is under its own gate, at zero

`.ocomment.toml` sets a tag list, `max_lines = 8`, `trailing = false`, and deadlines on `TODO`, `FIXME` and `HACK`.
A comment you add has to carry a tag,
fit in a paragraph, and sit above the code it is about; a promise you leave has a fortnight or a month before it becomes a finding.
A bare `ocomment` over this tree exits 0, and the CI job that runs it is a gate rather than a report.

There is no ledger here and no exemption for the tool's own source.
Both would be the same dodge: a tool whose own repository cannot pass its own rules is arguing that the rules are unreasonable.

The length rule is not "explain less".
Documentation is exempt from it because it is documentation — a `///`, an OCaml `(**`, a Python module docstring, a page under `docs/`.
Reaching zero here meant moving long rationale into those,
which is where a reader finds it anyway, and compressing the rest.
If your change needs more than a paragraph of prose, that is where it goes.

## Adding a language

The single most synchronisation-heavy change in the repository, and `CONTRIBUTING.md` lists every place it touches — a dozen files, several of them counting languages in prose.
Read that list before starting rather than discovering it one failing test at a time.
The tests are written to fail rather than to let a half-added language ship, so the build is on your side here.
