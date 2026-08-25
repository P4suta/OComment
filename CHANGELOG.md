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
  flag rather than ignoring it, and so does every command that writes no report
  of comments for it to annotate.
- The repository checks itself. `.ocomment.toml` runs the `legal` policy with
  `doc-line` and `doc-block` kept and protects any comment headed `NOTE`,
  `SAFETY`, `INVARIANT`, `PERF`, `TODO`, `FIXME`, or `HACK`, so an explanatory
  comment that says why it is there survives and one that only restates the
  line below it does not. Every such comment in `rust/` and `ocaml/` carries
  its tag, as does every one in the Python and shell tooling and in the
  `Dockerfile`; the only paths left out of the gate are vendored crates,
  fixture bytes, and packaging and benchmark scratch. `SAFETY` is reserved for
  its Rust-wide meaning — justifying an `unsafe` block — and a rationale about
  bytes or spoofing is an `INVARIANT`. `lefthook.yml` runs `ocomment check
  --staged` before each commit, and the `dogfood` CI job runs a bare `ocomment`
  over the tree, reports the environment through `doctor` and `config explain`,
  and then strips every comment out of a copy of the sources with `fix --policy
  all --force-protected` and rebuilds it: the Rust workspace builds and
  `ocomment-core` still passes its tests, and the `reference` job does the same
  for the OCaml reference. `CONTRIBUTING.md` documents the tags.

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
- `--format sarif` describes the rules it reports. `tool.driver` names the
  version that produced the run and carries a `rules` array: one entry for
  every comment kind — a title, a sentence, a link, and a default level — plus
  an entry for each scan diagnostic, skipped file, and unreadable file the run
  actually met. Every result points at its own entry through `ruleIndex`. A
  code-scanning UI titles a finding, describes it, and links out of it through
  that entry, so a finding used to arrive as a bare rule id and nothing else.

### Fixed

- `--staged` honours `files.include` and `files.exclude`. It read every path
  `git diff --cached` named, so a commit that touched an excluded tree — a
  vendored crate, generated tooling — was reported by the pre-commit hook, and
  `fix --staged` rewrote its index blob. A staged path is a walked path rather
  than a named one: it is measured against the project root exactly as a walk
  measures one, from whichever directory the command was typed in.
- A Rust string or byte-string literal may carry a bare newline, so a scan no
  longer ends one at the end of its line. `ocomment` reported its own
  `rust/ocomment/src/cli.rs` as invalid — two `unterminated-string`
  diagnostics for a multi-line `&str` constant — and then read the rest of the
  literal as source, finding comments inside it and refusing to write anything
  for the file. A Rust character literal still ends at the line, which is what
  keeps a lifetime from swallowing the rest of the source, and C, C++, Go, and
  Java literals are unchanged. The OCaml reference agrees.
- A walk never descends into `.git`, whatever lifted the hidden-file rule.
  Naming a directory does lift it, and so does `files.hidden`, so `ocomment fix
  .` in a fresh repository used to rewrite the sample hooks git had just
  written into `.git/hooks`. The exclusion covers the `.git` *file* a submodule
  or a linked worktree keeps in place of the directory. A path named inside
  `.git` is still a request and is still answered.
- SARIF and GitHub annotations spell a reported path the way the checkout
  spells it: forward slashes on every platform, and none of the `.` segments a
  typed target leaves behind — `ocomment check sub/./doc.rs` reported
  `sub/./doc.rs`, which matches no file in any repository, so the annotation
  landed on nothing and the SARIF result located nothing. A relative
  `artifactLocation` now also carries `uriBaseId: "%SRCROOT%"`; a SARIF reader
  given no base id has nothing to resolve the path against. An absolute path, a
  path that climbs out of the tree through `..`, and the `<stdin>` pseudo-path
  carry no base id, because none of them is under the source root.
- `# syntax=` and `# hadolint ignore=` are directives. A Dockerfile is scanned
  as shell, and both lines are read by a tool rather than by a person: removing
  the first changes which Dockerfile frontend builds the image, and removing
  the second turns a linter rule back on. The OCaml reference agrees, and the
  differential harness carries the case.
- `--format github` folds a walked skip away unless `-v` asks for it, the way
  the human report already did. A run over a repository annotated every file it
  had no scanner for, so the checks tab filled with notices about Markdown and
  YAML. An I/O error and a path the caller named are still always annotated.
- An invalid `[policy]` regex is reported on one line and in full. The `regex`
  crate writes a parse error over four lines with a caret under the byte it
  stopped at; the report replaced the newlines with U+FFFD instead of folding
  them, so a single failure arrived as one unreadable line of replacement
  characters.
- `tools/release_manifests.py` defaults `--repository` to `P4suta/OComment`.
  The release workflow passes `$GITHUB_REPOSITORY`, so the old default only
  ever reached someone generating the definitions by hand — and pointed the
  Homebrew formula, the Scoop manifest, and the WinGet manifest it wrote at a
  repository that is not this one.
