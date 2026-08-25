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
  Skipped paths are reported on standard error, so its standard output stays a
  patch that `git apply` accepts.
- `fix -i`/`--interactive`, which asks about each removable comment in turn —
  showing it with three lines of context either side and the line the removal
  would leave behind, capped at its first and last three lines so a tall comment
  cannot push the question off the screen — and writes only the accepted ones,
  through the same rollback-backed transaction a plain `fix` uses. `y`, `n`,
  `a` (the rest of this file), `d` (keep the rest of this file), `q` (stop
  asking and apply), `x` (abort and write nothing) and `?`. It needs a terminal
  on standard input and standard output, and refuses `--staged`, `--dry-run`,
  `-q`, and the machine formats rather than quietly ignoring one of the two
  flags.
- `ocomment doctor` probes the optional tools OComment shells out to — `curl`,
  `gh`, `oras`, and `cosign`, alongside `git` — and reports the environment it
  resolved: the working directory, the root, the configuration files it merged,
  and whether its output is a terminal. A missing tool is a row in the report
  naming what needs it, never a failing run.
- `init --force` and `init --stdout`; `init` otherwise refuses to overwrite an
  existing file and notes a configuration that already applies to the directory.
- `--explain`, which lists every comment a human `check` or `scan` met, kept
  ones included, and names the rule that decided each one together with the
  setting behind it: the `[policy]` table of a named file, a
  `[languages.<name>]` table, the `[[overrides]]` entry whose globs matched, the
  command-line flag, or the built-in default. A comment a built-in rule decided
  is left with the flag that would overrule it. The machine formats refuse the
  flag rather than ignoring it, and so do `fix`, `diff`, and `strip`, which
  write no report for it to annotate.

### Changed

- A command that names no path now checks the current directory rather than the
  repository or configuration root, matching every other file-walking developer
  tool. `ocomment fix` run from a subdirectory rewrites that subdirectory, and a
  stray `.git` above the tree no longer widens a run to everything under it. To
  check a whole repository, run a bare `ocomment` from its root: the directory a
  run with no PATH stands in for is walked with the ordinary hidden-file and
  size limits, while a path named explicitly (`ocomment .`, `ocomment src`)
  still bypasses both. `-v` names both the root and the target, and a bare `fix`
  below the root says which directory it is writing to.
- `files.include`, `files.exclude`, and `[[overrides]].paths` globs are matched
  against the path relative to the project root from any working directory.
  They were previously matched against the path as typed, so an override or an
  exclusion silently stopped applying whenever the command was run from
  anywhere but the root.
- Human output names comment kinds in their canonical kebab-case spelling
  (`doc-block`, `html-comment`) rather than leaking Rust `Debug` spellings.
- The manual page and the shell completions are generated from the binary, and
  the checked-in copies are verified against it.
- Run summaries and notes go to standard error, leaving standard output to the
  findings, patches, and machine formats alone.
- Failures say what to do next: how to add `version = 1`, which flag forces a
  language, how to clear a stale `.git/index.lock`, and which missing tool
  `ocomment doctor` diagnoses.
