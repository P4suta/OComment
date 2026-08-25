# OComment

OComment is a fast, byte-preserving comment checker and remover. The production
tool is the Rust `ocomment` binary and the public `ocomment-core` library.
`ocomment-ref` is an independent OCaml implementation used to check the scanner,
classification, diagnostics, edits, transformed bytes, and source maps.

OComment supports Rust, OCaml, C, C++, Go, Java, JavaScript, TypeScript, Python,
Shell, HTML, CSS, JSONC, SQL, and Kotlin. JSX/TSX, Objective-C/C++, GNU C/C++,
CUDA, POSIX sh, Bash 5.3, zsh, PostgreSQL, MySQL, SQLite, T-SQL, and Oracle are
explicit dialects. HTML `<script>` and `<style>` contents are scanned
recursively.

## Quick start

```sh
cargo install ocomment --locked

ocomment                 # check the current repository
ocomment check src tests
ocomment diff src
ocomment fix src
printf '%s\n' 'let x = 1; // remove' | ocomment strip --language rust
```

`check` exits 0 when clean, 1 when removable comments exist, and 2 for an
invalid source, configuration, plugin, or I/O failure. `diff` exits 1 when it
prints a change. Successful `fix` and `strip` operations exit 0. JSON, JSONL,
SARIF, and GitHub annotation output are available through `--format`.

The default `safe` policy removes ordinary and documentation comments while
keeping source preambles and tool/language directives. `legal` additionally
keeps license and copyright comments. `all` removes every comment token, but
still needs `--force-protected` before touching a shebang or encoding preamble.
HTML comments are kept unless `all` or `--remove-kind html-comment` is explicit.

```toml
version = 1

[policy]
mode = "legal"
layout = "lines"

[[overrides]]
paths = ["generated/**"]
policy = "all"
```

Run `ocomment init config` for the complete default file or `ocomment config
schema` for its JSON Schema. See [configuration](docs/configuration.md),
[editor/LSP setup](docs/editors.md), and [plugins](docs/plugins.md).

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
[docs/releasing.md](docs/releasing.md). OComment is dual-licensed under MIT or
Apache-2.0.
