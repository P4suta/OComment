# Changelog

All notable changes to OComment will be documented here. The project follows
[Semantic Versioning](https://semver.org/) after the first public release.

## Unreleased

### Added

- Byte-oriented scanners and transformations for 20 built-in languages and the
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
  attaches it to the twenty-five language identifiers OComment scans, and
  exposes the server's quick fixes, `source.fixAll.ocomment`, code lens, and
  pull diagnostics, plus `OComment: Remove comments in file`, `... in
  workspace`, `OComment: Restart server`, `OComment: Show output`, and a status
  bar count. `ocomment.path` resolves a relative path against the workspace and
  expands a leading `~`; a missing binary is a notification pointing at the
  install instructions rather than a silent failure. The extension is disabled
  in untrusted workspaces, because that setting names an executable it
  launches. The extension version is the crate version, checked by the
  extension's own suite on every pull request and against the tag before
  `publish-vscode` can upload anything. A `vscode` CI job lints, compiles,
  builds the binary the extension launches, and drives a real VS Code under
  `xvfb-run`; `publish-vscode` signs the `.vsix` with cosign, attaches it to
  the release, and publishes to the Marketplace and Open VSX. The extension
  holds one file system watcher for its lifetime rather than one per start, and
  every start, stop, and restart is queued behind the last, so a settings
  change during a restart cannot leave a second server running with nothing
  holding it.
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
- YAML is a built-in language, scanned by a lexer of its own. `#` opens the
  only comment form there is, and only where white space separates it from the
  token in front of it, so the `#` of a URL fragment and the one inside a plain
  `a#b` are content. Both quoted styles hide it, over a line break included,
  and so does a block scalar: `|` and `>` with their indentation and chomping
  indicators take a comment on the header line and then swallow every following
  line more indented than the node they hang off, empty lines and document
  markers decided in column zero. `.yml` and `.yaml` select it, as do the
  configuration files written in YAML that carry no extension of their own
  (`.clang-format`, `.clang-tidy`, `.yamllint`). The `# yaml-language-server:`,
  `# yamllint`, `# renovate:`, `# checkov:skip`, `# trivy:ignore`, `# nosec`,
  `# kics-scan` and `# @schema` lines are directives a removal keeps.
- PHP is a built-in language, scanned by a lexer of its own. A PHP file is two
  languages at once: it opens in inline HTML, where every byte is output
  verbatim and nothing is a comment, and `<?php` with white space behind it or
  the short echo tag `<?=` enters PHP mode, which `?>` leaves again — carrying
  one line break away with it. A bare `<?` opens nothing, because
  `short_open_tag` is off by default, so an XML declaration stays inline text.
  In code, `//` and `#` open a one-line comment that ends at the line break or
  at the closing tag, whichever comes first; `#[` is an attribute rather than a
  comment; `/* */` is a block comment and `/**` with white space behind it is a
  PHPDoc block, as the tokenizer decides it. Single-quoted, double-quoted and
  backtick strings, heredocs and nowdocs hide every comment opener inside them,
  including a closing tag, and the braces of `{$...}` are opaque. `.php`,
  `.phtml` and `.phpt` select it, as does a `php` `#!` line — which the CLI
  strips, so it is a preamble a removal keeps. The `phpcs:`,
  `@phpstan-ignore`, `@psalm-suppress` and `@codeCoverageIgnore` markers are
  directives a removal keeps. Inline HTML is opaque in v1: an HTML
  `<!-- ... -->` comment in a PHP file is not reported, which
  `docs/languages.md` says out loud. It is also the only place an editor may
  restart a scan from, because which mode a byte sits in is decided by
  everything above it, so a file that is all PHP is rescanned from the top.
- Ruby is a built-in language, scanned by a lexer of its own. Four of Ruby's
  tokens are spelled with a byte that is also an operator, and only where the
  token stands decides which, so the scanner keeps the three states Ruby's own
  lexer answers those questions from: `/` is a regular expression where a value
  is expected and division after an operand, `%` opens `%q %Q %w %W %i %I %s %r
  %x` and the bare `%(...)` in the first place and is modulo in the second, `?`
  is a one-character string or the ternary operator, and `<<` opens a here
  document or appends — with a bare word between the two, where white space in
  front and none behind makes `puts /x/` a pattern and `a <<EOS` a here
  document. `#` opens the only one-line form; `=begin` and `=end` at column zero
  delimit an embedded document; a `__END__` alone on its line ends the source
  and leaves the DATA section behind it opaque. Single-quoted, backtick,
  symbol, character, percent, regular-expression and here-document literals all
  hide a `#`, and the interpolating ones read `#{ ... }` as code, so a comment
  written inside one is a comment. A `#!` line at the first byte is a preamble
  and a `coding:` declaration in the first two lines is an encoding, the same
  two positional rules Python has; `# frozen_string_literal:`, `# warn_indent:`,
  `# shareable_constant_value:`, `# rubocop:`, `# standard:` and `# typed:` are
  directives a removal keeps. `.rb` and its eight siblings select it, as do the
  fourteen project files named after the tool that reads them — `Gemfile`,
  `Rakefile`, `Vagrantfile` and the rest — and a `ruby`, `jruby` or
  `truffleruby` `#!` line.
- `tools/fuzz_differential.py`, an on-demand cross-implementation fuzz. It
  builds random sources out of the delimiters, escapes, quotes and directive
  words the built-in scanners care about, asks both implementations across
  every language, dialect, policy and layout, and collapses whatever disagreed
  into one line per *kind* of disagreement — the fields that differ, not the
  source that reached them — with a shrunken repro beside each. `--seed` and
  `--cases` size the sweep; the language list comes from `spec/languages.toml`,
  so it cannot fall behind a language someone adds. It is not a CI gate and
  `CONTRIBUTING.md` says why: what it finds belongs in
  `spec/fixtures/v1/hazards.json` as a named case, which is what turns a run
  someone remembers into a gate.
- Every written-out count of languages, and of the editor language identifiers
  the VS Code extension attaches to, is checked against `Language::ALL` and
  against the extension's own selector. Six sentences across the documentation,
  the changelogs, and the Marketplace description said a number nothing
  verified; three of them were already wrong. The claims are matched with the
  file's line wrapping collapsed, so re-flowing a paragraph is not a failure
  and changing what it says is. The extension's `activationEvents` are checked
  against the languages it attaches the server to as well.

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
- An apostrophe opens a string in `jsonc`. That language is documented as `JSON
  with comments, including JSON5` and owns `.json5`, and JSON5 4.4 writes a
  string with either quote, so `{ 'note': '// not a comment' }` holds no
  comment. An apostrophe is already invalid in the stricter dialect, so the
  only thing the change hides is a `//` that dialect could not have meant as a
  comment.
- A directive named after the tool that reads it survives a missing argument.
  The keyword ends at a boundary so that prose merely opening with those
  letters is not protected, and the end of the comment is now such a boundary:
  a bare `#:schema`, `# shellcheck`, or `# hadolint` is the instruction it is
  about to be. The comment text arrives trimmed, so refusing the empty
  remainder protected the directive or not depending on a trailing space.
  `#:schemata are plural` is still prose.
- A UTF-8 byte order mark no longer hides the first line. CPython's `check_bom`
  and Lua's `skipBOM` both run before the first line is read, so a `#!` line
  behind a mark is still a preamble, and Lua's loader still skips a first line
  opening with `#`. `is_python_encoding_declaration` had always skipped the same
  three bytes; now the shebang rule does too. A shell is deliberately not
  included: `#` opens a comment only where no word has begun, and the mark's
  bytes begin one — `bash` reads the whole line as a command name, and the
  kernel does not honour the shebang either.
- A here-document delimiter ends at `>`. POSIX Shell Command Language 2.7.4
  makes the delimiter a word, and a word ends at an unquoted operator
  character, so `cat <<EOF>out` is a here-document named `EOF` and a
  redirection. It read as a delimiter `EOF>out` that no line ever matches,
  which swallowed the rest of the file.
- The vertical tab is whitespace to JavaScript and is not whitespace to the C++
  raw string delimiter. ECMA-262 12.2 lists <VT> as `WhiteSpace`, so
  `a\u{b}<div>` is a comparison rather than the start of a JSX element that
  would swallow the file; C++ [lex.string] excludes it from the d-chars, so
  `R"a\u{b}b(...)a\u{b}b"` is no raw string at all. Both had been asked of
  `u8::is_ascii_whitespace`, which answers neither question: it is the set that
  leaves the vertical tab out.

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
- Eleven ways the OCaml reference and the Rust engine disagreed, each now a
  named case in `spec/fixtures/v1/hazards.json`. The reference gave one generic
  `unterminated literal` message where the engine names the construct per
  language; read every `'` as a literal opener, where Rust Reference,
  *Lifetimes and loop labels* makes `'a` a lifetime that opens nothing;
  consumed an unterminated `r"` raw string to the end of the file in silence;
  took a vertical tab for layout whitespace, where `u8::is_ascii_whitespace`
  does not; trimmed only ASCII whitespace before looking for a directive word,
  so a `region` marker behind U+00A0 or U+2028 was removed rather than kept;
  read a C++ `R"` in the middle of an identifier as a raw string opener; gave
  up on a whole OCaml file at the first unterminated character literal; called
  every byte below space a shell blank, an HTML tag-name terminator, and a
  profile boundary; and read a `#!` at the start of an embedded `<script>` as
  the page's preamble. `tools/fuzz_differential.py` found the last seven; the
  first four came from the fuzz the Lua and TOML work ran.
- Two more, from the fuzz the YAML work ran. A `-->` behind a byte order mark
  closed an HTML-like comment in one implementation and not in the other, where
  ECMA-262 12.2 lists U+FEFF among `WhiteSpace` wherever it sits: both now walk
  the line in front of a `-->` treating the mark as the white space it is, so a
  space before it no longer hides the comment either. And the reference read
  `'\c` inside an OCaml comment as a character literal, where the manual makes
  one only of an apostrophe, a character or an escape sequence, and a closing
  apostrophe: the string that followed it was swallowed rather than left
  unterminated, and a comment that never closes came back valid. The comment
  scan now asks the same question the top-level scan already asked.
- A YAML block scalar body is measured from the node it hangs off, never from
  the column its own `|` or `>` sits in (YAML 1.2.2, 8.1.1.1). The header may
  stand anywhere past that owner: `key: !!str |` and `key: &x |` put node
  properties (6.9) in front of it, and `key:` may leave the indicator for the
  line below, indented as deeply as it likes. Reading the header's own column
  as the floor took a body indented less than the header for the end of the
  scalar, so every `#` line in it was reported as a comment and `ocomment fix`
  deleted the body under the default policy — and a `|` behind a tag or an
  anchor was not read as a header at all, which lost the whole body. Both
  implementations now carry the owner's indentation across the line break and
  read a property as the node property it is. A restart at the start of a line
  under a live carry is refused, because that owner is the one thing such a
  line does not say about itself. Eleven cases in
  `spec/fixtures/v1/hazards.json` pin the shapes.
- Removing comments from a YAML document never changes what it parses to, under
  any layout. A block scalar decides where its body ends from the lines *below*
  it (YAML 1.2.2, 8.1.1), which makes the lines under a body the one place in
  any language where the hole a removal leaves carries meaning — and it carried
  it in three ways. A line of spaces as wide as the comment, which `columns`
  writes, is indented at least as deep as the body it was terminating and is
  read back as content of it, whatever the header chomps. An empty line, which
  `lines` writes, is content under `|+` and `>+`, which keep the empty lines
  trailing a body (8.1.1.2). And the empty lines *between* a removed comment
  and the next line are `l-comment` only while the comment shelters them: once
  it is gone the `+` claims them too. So a whole-line comment sitting in the run
  of blank and comment lines under a block scalar body now goes with its whole
  line, terminator and all, under every layout and every chomping indicator, and
  under `|+`/`>+` it takes the blank run it was sheltering with it — never the
  blank lines above it, which were content already. `lines` gives up those
  lines' numbers and `columns` their columns rather than give up the value; it
  is the one exception either layout makes, and `docs/languages.md` states it.
  A phantom header is gone with it: `key: a |+` ends a plain scalar, and the
  trail is now hung off the headers the scanner itself recognises rather than
  off any `|` or `>` that ends a line. Sixteen cases in
  `spec/fixtures/v1/hazards.json` pin the shapes, and `tools/yaml_roundtrip.py`
  holds the invariant to PyYAML over thousands of generated documents.
- The other half of that invariant: the trail comment a block scalar leans on is
  now *kept*, because no removal there preserves the value. The line a body ends
  at is a comment shallower than the body's content (YAML 1.2.2, 8.1.1), and
  taking it — whole line and all, which is the least a removal can take — hands
  the lines under it back to the body. When one of those is a comment the run
  keeps and it is indented to the content depth, the body swallows it and the
  value grows a line; blanking the line instead is no better, since an empty
  line is content of the scalar whatever its indentation (8.1.1.2). Both
  implementations now weigh a block scalar's trail after the scan and give that
  one comment a sixth frozen keep reason, `structural in a YAML block scalar
  trail`, which `--explain` renders as a sentence naming the block scalar under
  the comment and the line that has to go before this one can. It is the one
  keep no setting reaches: `--policy all` removes the comment below it and the
  question with it, but a `keep_regex` that spares that comment leaves this one
  load-bearing. The depth is the body's *content* indentation — the
  explicit indentation indicator, or the first non-empty line's column
  (8.1.1.1) — not the floor a body line has to clear, so a surviving comment
  shallower than the content keeps nothing above it and the comment above it is
  removed as before. `DispositionExplanation::KeptStructural` and
  `explain_comment` name the rule for a library caller. Nine cases in
  `spec/fixtures/v1/hazards.json` pin the shapes, and `tools/yaml_roundtrip.py`
  gained a sweep of trails written on both sides of the body's own indentation —
  the old generator capped a trail comment one column short of it, so this whole
  half of the hazard was ungenerated.
- `tools/yaml_roundtrip.py` runs its layout and policy passes together instead
  of one after another. A pass costs one `fsync` per rewritten file, which is
  latency rather than work, so overlapping them is what pays: the full sweep now
  covers three policies rather than two and a third more documents in under half
  the wall time. CI runs the corpus and both enumerated sweeps in full and cuts
  only the pseudo-random set, whose cost is linear and whose value is not; the
  whole 2400 is one command away.
- A Ruby here document opened inside an interpolation takes the lines under the
  line it was written on. `puts "#{ <<EOS }"` queued the body on the
  interpolation's own scan, which the `}` returned from and dropped, so the body
  was read as code and a `#` line inside it was reported as a comment and
  removed. The queue now belongs to the physical line rather than to the scan
  reading it: openers are consumed left to right across the whole line whether
  they stand before an interpolation, inside one, inside a nested one, or inside
  an interpolation on another here document's body line — the order Ruby 3.3.12
  `Ripper.lex` gives — and it is drained wherever the lexer meets the line
  break, so a break inside an interpolation drains the openers written before it
  too. Eight cases in `spec/fixtures/v1/hazards.json` pin the shapes, each with
  the `Ripper.lex` output it was recorded against. The OCaml reference agrees.
- The two corpus runners hold `spec/fixtures/v1` to the same floors. Each
  carried its own copy of the numbers and they had drifted apart —
  `tools/differential.py` required 308 cases and
  `rust/ocomment-core/tests/spec_fixtures.rs` 306 — so two cases could have gone
  missing without either runner saying so. Both now read
  `spec/fixtures/v1/floor.txt`, which is the only place the numbers are written.
- The randomised property suites draw from one alphabet. `src/incremental.rs`
  said its pool was the one in `tests/properties.rs` and that the two were meant
  to stay the same list; they were not, and the incremental suite was missing
  sixteen of the multi-byte openers the whole-file suite had — every Lua long
  bracket, every triple-quoted string, and every PHP fragment among them, which
  are exactly the states a restart must never land inside. Both now draw from
  `ocomment_core::lexical_pool`, which also carries the Ruby interpolation
  boundary a here document header may be written across.
