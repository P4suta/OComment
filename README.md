# OComment

[![CI](https://github.com/P4suta/OComment/actions/workflows/ci.yml/badge.svg)](https://github.com/P4suta/OComment/actions/workflows/ci.yml)
[![CodeQL](https://github.com/P4suta/OComment/actions/workflows/codeql.yml/badge.svg)](https://github.com/P4suta/OComment/actions/workflows/codeql.yml)
[![MSRV 1.88](https://img.shields.io/badge/MSRV-1.88-93450a.svg)](rust/Cargo.toml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![VS Code extension](https://img.shields.io/visual-studio-marketplace/v/P4suta.ocomment?label=VS%20Code&color=0066b8)](https://marketplace.visualstudio.com/items?itemName=P4suta.ocomment)

OComment is a fast, byte-preserving comment checker and remover. The production
tool is the Rust `ocomment` binary and the public `ocomment-core` library.
`ocomment-ref` is an independent OCaml implementation used to check the scanner,
classification, diagnostics, edits, transformed bytes, and source maps.

OComment supports Rust, OCaml, C, C++, Go, Java, JavaScript, TypeScript, Python,
Shell, HTML, CSS, JSONC, SQL, Kotlin, TOML, Lua, YAML, PHP, Ruby, Zig, R, Dart,
Swift, C#, and Scala.
JSX/TSX,
Objective-C/C++,
GNU C/C++, CUDA, POSIX sh, Bash 5.3, zsh, PostgreSQL, MySQL, SQLite, T-SQL, and
Oracle are explicit dialects. HTML `<script>` and `<style>` contents are scanned
recursively, and a PHP file is scanned inside its `<?php ... ?>` tags.

The complete documentation is the Markdown in [`docs/`](docs/), which GitHub
renders as you read it, and every documentation link below points there. The
same pages are built into a book at <https://p4suta.github.io/OComment/> (once
published).

## Quick start

```sh
cargo install ocomment --locked

ocomment                 # check the current directory
ocomment check src tests
ocomment diff src
ocomment fix --dry-run src
ocomment fix src
printf '%s\n' 'let x = 1; // remove' | ocomment strip --language rust
```

Or install nothing: `docker run --rm -v "$PWD:/src"
ghcr.io/p4suta/ocomment:0.1.0 check` runs the same CLI from the
[container image](docs/docker.md).

A command that names no path checks the current directory, so running it from
a subdirectory checks that subdirectory; a bare `ocomment` run from the
repository root already checks the whole repository, under the ordinary walk
limits — `cd "$(git rev-parse --show-toplevel)"` gets there from anywhere
inside it. Naming a path explicitly (`ocomment .`, `ocomment src`) is a request
rather than a default, so it bypasses the hidden-file and size limits, as
[configuration](docs/configuration.md) describes.

A human run previews each removable comment and closes with a summary:

```console
$ ocomment check src
src/main.rs:2:5: removable line comment: // TODO: drop this
Found 1 removable comment in 1 file (1 file scanned). Run `ocomment fix` to remove it.
```

Findings, patches, and machine formats go to standard output; the summary and
every note go to standard error, so `ocomment diff src > fix.patch` keeps the
patch clean. `-q` drops the summary and leaves `check` to answer with its exit
code, while `diff` and `scan` still write the patch or listing they exist for;
`-v` traces what was scanned and counts every comment kind; `--no-preview`
drops the previewed text. A `-` target reads standard input under the `<stdin>`
pseudo-path, and `fix --dry-run` prints the patch `fix` would apply without
writing a file.

`fix -i` (`--interactive`) asks before each removal instead of applying them
all. Every comment `fix` would take out is shown where it starts, with three
lines of context either side and the line as the removal would leave it, above
a prompt: `y` removes it, `n` keeps it, `a` and `d` answer for the rest of the
file at once, `q` stops asking and applies what was accepted, `x` abandons the
run without writing anything, and `?` lists them again. A comment taller than
the window is shown as its first and last three lines with a marker for the
rest, so the question stays in view. The accepted removals are written through
the same rollback-backed transaction a plain `fix` uses.
Because it is a conversation, it needs a terminal on both standard input and
standard output and refuses `--staged`, `--dry-run`, `-q`, and every machine
`--format`; `ocomment diff` is the way to review the same changes without one.

### Why was this comment kept?

`--explain` lists every comment a human `check` or `scan` met — the kept ones
included — and puts the rule that decided each one, together with the setting
behind that rule, on the line under it:

```console
$ ocomment check --explain
gen/api.rs:1:1: kept block comment: /* generated */
    kept: matched keep_regex #0 `(?i)generated` ([[overrides]] #0, paths = ["gen/**"])
src/app.js:1:1: kept directive comment: // eslint-disable-next-line
    kept: tool or language directive `eslint`; use --remove-kind directive or --policy all to remove it
src/app.js:3:12: removable line comment: // TODO
    removed: policy `safe` removes ordinary comments ([policy] in .ocomment.toml)
```

A setting is named where it was written: the `[policy]` table of a file, a
`[languages.<name>]` table, the `[[overrides]]` entry whose globs matched the
path, or the flag on the command line. A comment no setting decided is left
with the flag that would overrule the built-in rule instead. `--explain`
annotates a report of comments, so it belongs to the two commands that write
one and to the one format with room for prose: asking for it with `--format
json`, or any other machine format, or with any command that writes no report
of comments, is a usage error rather than a flag that quietly does nothing,
and `-q` silences `check` altogether, explanations included.

`check` exits 0 when clean, 1 when removable comments exist, and 2 for an
invalid source, configuration, plugin, or I/O failure. `diff` and
`fix --dry-run` exit 1 when they print a change. Successful `fix` and `strip`
operations exit 0. JSON, JSONL, SARIF, and GitHub annotation output are
available through `--format`. Run `ocomment --help` for every option and
`ocomment man` for the manual page.

The default `safe` policy removes ordinary and documentation comments while
keeping source preambles and tool/language directives. `legal` additionally
keeps license and copyright comments. `all` removes every comment token, but
still needs `--force-protected` before touching a shebang or encoding preamble.
HTML comments are kept unless `all` or `--remove-kind html-comment` is explicit.

The `lines` layout keeps every line where it was, `columns` keeps every column
as well, and `compact` drops the lines a removed comment had to itself.

```toml
version = 1

[policy]
mode = "legal"
layout = "lines"

[[overrides]]
paths = ["generated/**"]
policy = "all"
```

`files.include`, `files.exclude`, and every `[[overrides]].paths` glob is
relative to the project root — the directory holding `.ocomment.toml`, or the
repository above it — however deep in the tree the command is run from.

Run `ocomment init config` for the complete default file or `ocomment config
schema` for its JSON Schema. See [configuration](docs/configuration.md),
[editor/LSP setup](docs/editors.md), [plugins](docs/plugins.md), and
[hooks and CI](docs/ci.md).

The official VS Code extension is `P4suta.ocomment`; it launches this binary,
so install both. Its source is in [`editors/vscode`](editors/vscode/README.md).

## Partially staged changes

`ocomment fix --staged` reads and rewrites Git index blobs, then maps only those
edits to the working tree when the mapping is unique. It never stages unrelated
working-tree changes. Use `--index-only` when a working-tree mapping is
ambiguous.

```sh
ocomment init lefthook --fix
lefthook install
```

The generated hook deliberately does not use Lefthook `stage_fixed`, because
that setting would add the complete working-tree file and destroy partial
staging.

## Hooks and CI

`.pre-commit-hooks.yaml` publishes `ocomment-check` and `ocomment-fix` for
[pre-commit](https://pre-commit.com). The hooks are `language: system`, so
install the CLI first, then point a `.pre-commit-config.yaml` at this
repository:

```yaml
repos:
  - repo: https://github.com/P4suta/OComment
    rev: v0.1.0
    hooks:
      - id: ocomment-check
```

`ocomment-check` exits 1 and blocks the commit while a staged file still has a
removable comment. `args: ["--staged"]` judges the index blobs rather than the
working tree, which is what a partially staged file needs.

`action.yml` is a composite GitHub Action. It downloads the release archive for
the runner, verifies its SHA-256 and its build-provenance attestation, and
annotates the pull request:

```yaml
      - uses: P4suta/OComment@v0.1.0
        with:
          paths: src tests
```

`format: sarif` with `upload-sarif: "true"` sends the findings to code scanning
instead, and `fail-on-findings: "false"` leaves the verdict to a later step
reading the `exit-code` output. [CI and hooks](docs/ci.md) documents every input
and output, the `--staged` caveats, and how to run the action where no release
archive is published.

## Library

```rust
use ocomment_core::{scan, transform, Language, ScanOptions, TransformOptions};

let report = scan(b"let x = 1; // note\n", Language::Rust, ScanOptions::default());
assert_eq!(report.comments.len(), 1);

let result = transform(
    b"let x = 1; // note\n",
    Language::Rust,
    TransformOptions::default(),
);
assert_eq!(result.output, b"let x = 1; \n");
```

Spans are half-open byte ranges. Edits are sorted and non-overlapping. The
engine never requires the complete source to be UTF-8, so BOMs, CRLF, trailing
newlines, and non-UTF-8 bytes outside edited spans are preserved.

## Repository and verification

- `spec/` contains the shared schemas, language table, WIT interface, and
  differential fixtures.
- `rust/` contains the library, CLI, Git integration, LSP server, WASM host, and
  plugin SDK.
- `ocaml/` contains the pure reference library and JSONL verification CLI.

```sh
cargo test --manifest-path rust/Cargo.toml --workspace --all-targets --locked
dune runtest --root ocaml
./tools/differential.sh
python3 tools/check_embedded_specs.py
```

The release process and performance gates are documented in
[Releasing](docs/releasing.md).

## Contributing and support

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Use
[Discussions](https://github.com/P4suta/OComment/discussions) for support and
design questions, and follow [SECURITY.md](SECURITY.md) for private vulnerability
reports.

## License

OComment is available under either the [MIT license](LICENSE-MIT) or the
[Apache License 2.0](LICENSE-APACHE), at your option.
