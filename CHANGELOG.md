# Changelog

All notable changes to OComment will be documented here. The project follows
[Semantic Versioning](https://semver.org/) after the first public release.

## Unreleased

### Added

- Byte-oriented scanners and transformations for 15 built-in languages and the
  documented dialects.
- CLI, staged Git fixes, LSP 3.18 server, declarative profiles, and sandboxed
  WASM component plugins.
- Independent OCaml reference implementation and byte-for-byte differential
  fixtures.
- Cross-platform CI, packaging definitions, and release verification gates.
- Full `--help` for every command and every possible value, an exit-status,
  files, and examples epilogue, and an `ocomment man` subcommand that renders
  the manual page.
- `-q`/`--quiet`, `-v`/`--verbose`, a `--progress` live scanning counter, and a
  one-line preview of the reported comment that `--no-preview` turns off.
- `-` as a target: `check`, `diff`, and `scan` read standard input under the
  `<stdin>` pseudo-path.
- `fix --dry-run`, which prints the patch `fix` would apply and writes nothing.
- `init --force` and `init --stdout`; `init` otherwise refuses to overwrite an
  existing file and notes a configuration that already applies to the directory.

### Changed

- Human output names comment kinds in their canonical kebab-case spelling
  (`doc-block`, `html-comment`) rather than leaking Rust `Debug` spellings.
- The manual page and the shell completions are generated from the binary, and
  the checked-in copies are verified against it.
- Run summaries and notes go to standard error, leaving standard output to the
  findings, patches, and machine formats alone.
- Failures say what to do next: how to add `version = 1`, which flag forces a
  language, how to clear a stale `.git/index.lock`, and which missing tool
  `ocomment doctor` diagnoses.
