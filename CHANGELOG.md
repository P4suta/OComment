# Changelog

All notable changes to OComment will be documented here. The project follows
[Semantic Versioning](https://semver.org/) after the first public release.

## Unreleased

### Added

- Byte-oriented scanners and transformations for 17 built-in languages and the
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
- An official VS Code extension, `P4suta.ocomment`, under `editors/vscode`. It
  is a client only: it launches the separately installed `ocomment lsp`,
  attaches it to the twenty language identifiers OComment scans, and exposes
  the server's quick fixes, `source.fixAll.ocomment`, code lens, and pull
  diagnostics, plus `OComment: Remove comments in file`, `... in workspace`,
  `OComment: Restart server`, `OComment: Show output`, and a status bar count.
  `ocomment.path` resolves a relative path against the workspace and expands a
  leading `~`; a missing binary is a notification pointing at the install
  instructions rather than a silent failure. The extension is disabled in
  untrusted workspaces, because that setting names an executable it launches.
  The extension version is the crate version, checked by the extension's own
  suite on every pull request and against the tag before `publish-vscode` can
  upload anything. A `vscode` CI job lints, compiles, builds the binary the
  extension launches, and drives a real VS Code under `xvfb-run`;
  `publish-vscode` signs the `.vsix` with cosign, attaches it to the release,
  and publishes to the Marketplace and Open VSX. The extension holds one file
  system watcher for its lifetime rather than one per start, and every start,
  stop, and restart is queued behind the last, so a settings change during a
  restart cannot leave a second server running with nothing holding it.
- Item-by-item documentation for `ocomment-core` and `ocomment-plugin-sdk`,
  and a gate that keeps it. `missing_docs` is denied through
  `[workspace.lints]` for both library crates, so a public type, field,
  variant, or method added without a doc comment fails `cargo clippy`. The
  crate documentation states what byte-preserving means, that spans are
  half-open and edits sorted and non-overlapping, and gives the policy
  against comment-kind table; `scan`, `transform`, `transform_spans`,
  `apply_edits`, `detect_language`, `explain_disposition`,
  `DeclarativeProfile`, `SourceMap` and `IncrementalDocument` each carry a
  runnable example, and `# Errors` and `# Panics` sections say what a call
  refuses and what it asserts. CI now runs `cargo test --doc` — which
  `--all-targets` silently skips — and `cargo doc` with `-D warnings`, so an
  example that stops compiling or a broken intra-doc link fails the build.
  `IncrementalError`, `LineDelimiter`, `BlockDelimiter`, `StringDelimiter`
  and `ProtectedPattern` are exported from the crate root: each appeared in a
  public signature that no downstream caller could name. Four runnable
  examples under `rust/ocomment-core/examples` — `strip`, `external_spans`,
  `incremental`, `profile` — and both library crates carry
  `[package.metadata.docs.rs]`.
- `ocomment languages` is generated from `spec/languages.toml`, which the
  binary now embeds, and `--format json` writes that table as an array of
  objects: `name`, `extensions`, `dialects`, and, where a row has them,
  `extension_dialects`, `reserved_names`, `shebangs`, and `notes`. The shared
  table now records the dialect an extension selects — `.m` is Objective-C,
  `.mm` Objective-C++, `.cu` CUDA — the whole file names that carry no
  extension at all (`Dockerfile`, `Containerfile`, `Makefile`, `GNUmakefile`,
  `.profile`, `.bashrc`, `.zshrc`, `tsconfig.json`, `jsconfig.json`), and the
  interpreter names a `#!` line is read for, and `docs/languages.md` is
  generated with them. `tools/check_embedded_specs.py` holds the embedded copy
  to the canonical file, and `rust/ocomment/tests/spec_languages.rs` checks
  every claim the table makes against the code that has to honour it: each
  extension, reserved name, and shebang against `detect_language`, each row of
  dialects against the list the binary prints when it refuses one, the schema
  enumerations against the same vocabulary, and both listings against the table
  itself.
- TOML is a built-in language, scanned by a lexer of its own rather than by the
  profile engine. `#` opens the only comment form there is, and every string
  form hides one: basic and literal strings, the multi-line forms of both —
  where the closing delimiter is the last three of a run of up to five quotes —
  and the quoted keys written in either. `.toml` selects it, as do the lock
  files written in TOML that carry no extension of their own (`Cargo.lock`,
  `Pipfile`, `poetry.lock`, `uv.lock`, `pdm.lock`; `Pipfile.lock` is JSON and
  is not among them). Taplo's `#:schema` and `# taplo:` lines are directives a
  removal keeps.
- Lua is a built-in language, scanned by a lexer of its own. `--` opens a short
  comment and a long bracket after it — `--[[`, `--[==[` — a long one, which
  ends only at the closing bracket of its own level; the same brackets without
  the `--` are long strings, and `a[b[1]]` is neither, because a long bracket
  needs its second `[`. Short strings carry `\z`, which swallows the whitespace
  and newlines after it, and a backslash before a line ending, which carries it
  into the string. `---` is the documentation comment of LDoc and the Lua
  language server, a fourth dash makes an ordinary divider, and `---@diagnostic`
  is a directive where the other annotations are documentation, alongside the
  `-- luacheck:`, `-- selene:`, `-- stylua:` and `-- luacov:` lines. `.lua` and
  `.rockspec` select it, as does a `lua` or `luajit` `#!` line — which, like any
  first line that opens with `#`, the loader skips.

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
- `layout = "compact"` is a layout of its own. A line that held nothing but a
  removed comment now goes away with it, terminator included, and the
  whitespace a removal would leave at the end of a line is trimmed, so a run of
  whole-line comments disappears instead of becoming a run of blank lines;
  until now `compact` produced exactly what `lines` produces, on every input.
  Code keeps its own lines: a line that code survives on keeps its terminator
  and its CRLF or LF style, a comment running across several lines with code
  before or after it closes up to a single line rather than joining two
  statements, and a surviving line keeps the ending it had in the source — the
  same LF or CRLF, from inside the comment if that is where it was, or none at
  all if the file stopped there without one. Being alone on a line is judged
  from the original bytes, so a line holding two comments and nothing else
  keeps its terminator. `lines` and `columns` are unchanged byte for byte, and
  fourteen `compact-*` cases in the shared fixture corpus pin the new bytes in
  both implementations.

### Fixed

- `ocomment languages` lists every extension the detector knows. The listing was
  a table written by hand beside the detector rather than generated from the
  shared spec, so `.m`, `.mm`, `.cu`, and `.xhtml` were scanned but never
  listed, and `--format json` was accepted and quietly answered with the human
  table; the machine formats that have nowhere to put a language table are now
  refused. `spec/languages.toml` was itself missing `standard` from the shell
  dialects, and listed C's and C++'s in an order the binary does not use, so a
  dialect the binary accepts read as unsupported.
- The LSP server places the `shellscript` and `cuda-cpp` language identifiers.
  Neither parses as an OComment language name, so a buffer the editor called
  either of them fell back to detection by path and bytes, and one that carried
  no telling extension — a shell hook with no suffix, a CUDA scratch file — was
  left `unknown` and answered with `a language is required` instead of its
  comments. `shellscript` takes the dialect from the path when the path agrees
  it is a shell script, because that one identifier covers sh, Bash, and zsh
  alike and `$'...'` is an ANSI-C quoted string in only the last two.
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
  the second turns a linter rule back on. `hadolint` and `shellcheck` are whole
  words, so each ends at a boundary rather than at one particular byte:
  `# hadolint\tignore=DL3018` is the directive written with a tab and is kept,
  while `# hadolintish note` and `# shellcheckish note` are prose about the
  tool and stay removable. The OCaml reference agrees, and the differential
  harness carries the case.
- `tools/check_directives.py` gives every marker in `spec/directives.toml` a
  near-miss the scanner has to remove. The check proved that each marker is
  protected; nothing proved it protects no more than itself. The near-miss is
  written from the marker's own text — `# hadolint ignore=DL3018` against
  `# hadolintish note` — rather than from the name the spec files it under,
  which for seven of the fifteen was a word appearing nowhere in the marker and
  so tested nothing about it. It is also scanned in the marker's own place
  rather than appended below it, because a shebang is a shebang only at the
  first byte of the first line and an Oracle hint only when its `+` touches the
  `/*`: a near-miss further down the file could never have been protected
  whatever it said. Feeding each marker back in as its own near-miss now fails
  for all fifteen, where five of them used to pass. It also runs from
  `tools/release-check.sh` now, against the release binary.
- A staged path the caller names is checked whatever `files.hidden` and
  `files.max_size` say about it, the way a named path is on a walk.
  `ocomment check --staged .hidden/x.rs` answered about zero files, which reads
  as a clean file rather than as a path outside the project's bounds; a path
  nobody named is still bounded by both.
- A staged pathspec is put to `git` rather than compared as text, so it names
  the paths it covers however it is written. An absolute path and a wildcard
  `git` expands matched nothing against the root-relative path
  `git diff --cached` answers with, so `ocomment check --staged .hidden/*.rs`
  was read as naming no path at all and the file it named stayed bounded by the
  limits a named path lifts. A relative pathspec is also resolved where it was
  typed, so `--staged .` from `src/` means that subtree, as `ocomment check .`
  does — it reached the whole repository. The one pathspec that names nothing
  in particular is the one that covers everything: `--staged .` from the top is
  the bare run it looks like, `[files]` limits included, where it used to lift
  `hidden` and `max_size` from the whole tree at once.
- A staged blob with no built-in language, and one that turns out to be binary,
  are counted in the end-of-run summary — `2 files skipped (binary: 1, unknown
  language: 1)` — and listed by `-v`, exactly as a walk reports them. A hook
  that staged a PNG beside its source passed both over without a word. One the
  caller named is answered on its own line instead, the way a walk answers a
  named path: `ocomment check --staged notes.md` that says only "nothing to
  check" reads as a clean file rather than as a file nothing could read.
- An invalid `.ocomment.toml` is reported on one line and in full. `toml`
  quotes the line it stopped on, with a caret under the byte that is wrong with
  it, so a control character in a project file reached the terminal verbatim
  over four lines of diagram; the verdict is folded onto one line and every
  byte of it is printable, as an invalid `[policy]` regex already was. The path
  in front of the colon is held to the same rule and for the same reason: it
  names a directory the project chose, so a `\x07` in that name rang the
  terminal's bell on the way past.
- Every example on the library page is compiled and run. `docs/library.md` says
  it is, but the page is hand-written prose and `cargo test --doc` reads only
  what is in the crate sources, so nothing had checked it since it was written;
  CI hands the page to `rustdoc --test` against the built `ocomment-core`. The
  docs job also pins mdBook, so the published HTML changes only when a commit
  changes it.
- The README links to the Markdown under `docs/`, which GitHub renders, rather
  than to a Pages site that is not published yet; one line names the site and
  says so. `docs/verify.md` says which version its examples pin, the way
  `docs/installation.md` does — the tag inside a signing identity is part of
  what the check proves.
- `--format github` folds a walked skip away unless `-v` asks for it, the way
  the human report already did. A run over a repository annotated every file it
  had no scanner for, so the checks tab filled with notices about Markdown and
  YAML. An I/O error and a path the caller named are still always annotated,
  `-q` included: `-q` trims the human report, and an annotation is the product
  of a machine format rather than commentary about it.
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
