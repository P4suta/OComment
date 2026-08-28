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
python3 tools/check_editor_ids.py
python3 tools/check_ci_contracts.py
python3 -m unittest tools/test_release_metadata.py tools/test_publish_crates.py
python3 tools/check_directives.py
python3 tools/validate_schemas.py
python3 tools/yaml_roundtrip.py
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

### The YAML round trip

Every other language lets a removal be judged by the bytes it leaves behind.
YAML does not: a block scalar decides where its body ends from the lines
*below* it, so the hole a removal leaves on a comment line can be read back as
content of the scalar above it. That is a property of the *parsed value*, and
no byte-level fixture can state it.

`tools/yaml_roundtrip.py` states it. Its documents come from four places —
every YAML case in `spec/fixtures/v1`; a systematic sweep of every block scalar
header crossed with every short arrangement of blank, comment, and directive
lines under one; a second sweep of the same headers over trails whose comments
sit *below* the body's own indentation, where a surviving comment is what the
body would swallow and the comment above it is the only thing holding it out;
and a few thousand generated documents of nested mappings, sequences, and block
scalars with comments in every position, in LF and in CRLF. It strips every one
of them under all three layouts and all three policies — `safe`, `legal` and
`all`, because each keeps a different comment and only a survivor makes the
hazard reachable — and asserts that PyYAML reads the same value out of it
afterwards. A document PyYAML rejects *before* the removal is skipped: YAML has
shapes a lexer cannot rule out and a parser will not take.

```sh
python3 -m pip install pyyaml
cargo build --manifest-path rust/Cargo.toml --locked -p ocomment
python3 tools/yaml_roundtrip.py                          # NOTE: the full sweep
python3 tools/yaml_roundtrip.py --cases 200              # NOTE: what CI runs
python3 tools/yaml_roundtrip.py --cases 20000 --seed 7   # NOTE: a longer sweep
```

CI runs `python3 tools/yaml_roundtrip.py --cases 200` in the `dogfood` job: the
corpus and both enumerated sweeps run in full there — they are where the hazard
lives and they are the same documents on every run — and only the pseudo-random
set is cut, because its cost is linear and its value is not. The bare run above
is the fuller one — around 5,900 documents against CI's 3,700 — and `--seed`
moves the generated set. Unlike the fuzz below it is deterministic: the seed is
fixed, so a red run reproduces. Anything it finds belongs in
`spec/fixtures/v1/hazards.json` as a named case per layout, the same as a fuzz
finding.

### On demand: the differential fuzz

`tools/differential.py` asks the two implementations the questions
`spec/fixtures/v1` already knows to ask. `tools/fuzz_differential.py` asks them
questions nobody thought of — random sources built from the delimiters,
escapes, quotes and directive words the built-in scanners care about, across
every language, dialect, policy and layout — and reports each way the answers
differed once, with a shrunken source that still shows it.

```sh
cargo build --manifest-path rust/Cargo.toml -p ocomment-core --example ref_driver --locked
opam exec -- dune build --root ocaml bin/main.exe
python3 tools/fuzz_differential.py --seed 1 --seed 2      # NOTE: ~2 minutes
python3 tools/fuzz_differential.py --cases 200            # NOTE: a quicker sweep
```

The pool it draws from is one pool for every language, so a scanner meets the
delimiters it does not own — but it is a pool of *tokens*, and a lexical state
that only a whole word opens is never reached by a per-byte draw. That is why
the pool carries a named group for each language whose states are spelled that
way: a YAML block scalar header, a `<?php` tag, a Ruby `=begin` or `<<~EOS`.
Adding a language means adding its own group, or the sweep runs its scanner over
sources that never leave the top level.

It is not wired into CI and is not meant to be: it is random, so a green run is
weaker evidence than a corpus case and a red one is not reproducible from the
pipeline alone. Run it after a scanner change, and turn whatever it finds into
a named case in `spec/fixtures/v1/hazards.json` — that is what makes the
finding a permanent gate rather than a run someone remembers.

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

A comment that a machine reads and rewrites is the one thing this rule has no
tag for, and `[policy] keep_regex` in `.ocomment.toml` is where such a shape is
named instead: the version beside a SHA-pinned action is required by this
document, is maintained by Dependabot rather than by a reader, and is kept by a
pattern that matches the whole comment. Adding a language can bring more files
under the gate and so more such shapes; a bare `ocomment` is what finds them.

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
- Use a Conventional Commit subject for the pull request's squash commit:
  `fix:`, `feat:`, and a `!` or `BREAKING CHANGE:` footer are the release-plz
  signals for the next SemVer version and changelog. Use a scope when it makes
  the subject clearer, such as `feat(languages): ...`.
- Add tests for observable behavior and update user-facing documentation.
- Regenerate checked-in schemas, WIT, man pages, or completions when their source
  changes; `tools/check_embedded_specs.py` checks shared embedded assets.
- Adding a language to `spec/languages.toml` does not require a second
  pre-commit extension list: `.pre-commit-hooks.yaml` sends every text file to
  the CLI detector, and `tools/check_hooks.py` rejects a filter that would hide
  reserved names or extensionless shebang scripts. It changes what this
  repository checks about itself as well: a file the new scanner now reads is a
  file whose comments have to carry a tag, so run a bare `ocomment` before
  opening the change. Two more things count the languages rather than reading
  the table: `spec/fixtures/v1/floor.txt`, the floors that stop a later change
  from quietly dropping the fixtures the language brings — `cases` and
  `expectations`, both read by `tools/differential.py` and by
  `rust/ocomment-core/tests/spec_fixtures.rs`, and both raised by the number of
  fixtures the language adds in the same commit that adds them — and the
  editor clients: `editors/vscode/package.json` lists the identifiers the
  extension attaches to in both `activationEvents` and the `ocomment.languages`
  default, and `docs/editors.md` names and counts them. An editor identifier is
  written in the editor's vocabulary rather than in this one, so it also has to
  *reach* the language: `language_from_lsp` in `rust/ocomment/src/lsp.rs` parses
  it as a `Language` and carries an arm for each one that does not agree —
  `objective-c`, `cuda-cpp`, `javascriptreact`, `shellscript`. Nothing else
  notices when a new identifier agrees with nothing: the extension activates,
  the server opens the document, and the scan comes back with the
  `unknown-language` diagnostic and no comments, which reads like a language
  that was never added at all.
  `every_editor_language_identifier_reaches_a_built_in_language` in that file
  reads the selector and fails instead. The two
  published JSON schemas carry the vocabulary rather than deriving it:
  `spec/config.schema.json` enumerates the languages a configuration may name
  and `spec/result.schema.json` the ones a report may carry, which is the same
  list plus `unknown`. `the_schemas_enumerate_the_same_vocabulary` compares both
  against the table, so a language added to one file alone fails the build. Every
  written-out count of languages or of editor language identifiers is checked
  against `Language::ALL` and against that selector by
  `every_written_language_count_matches_what_it_counts` in
  `rust/ocomment/tests/spec_languages.rs`, so the sentences fail the build
  rather than drifting. It reads six files, and the whole set is worth having in
  front of you rather than discovering one failure at a time: the coverage row
  of `docs/comparison.md`, the `description` of
  `editors/vscode/package.json`, `editors/vscode/README.md` and
  `editors/vscode/CHANGELOG.md` — the extension carries its own two counts
  besides the ones in `docs/` — `docs/editors.md`, and `CHANGELOG.md`, which is
  counted twice, once for the languages and once for the editor identifiers.
  The *names* beside those counts are still yours to extend: `docs/editors.md`,
  the language lists in `README.md` and `editors/vscode/README.md`, and the
  `Added` entry in `CHANGELOG.md`. Two more
  places name the language rather than counting it: `Language::ALL.len()` is
  asserted outright by `language_names_are_stable` in
  `rust/ocomment-core/tests/names.rs`, and every language carries one line of
  per-value help in `rust/ocomment/src/values.rs`. Every spelling the language
  answers to goes in that same test: `language_aliases_are_pinned` holds a row
  for the canonical name and for each entry of `Language::aliases`, and it
  checks that table *against* `Language::aliases` in both directions, so an
  alias added to the one and not the other fails there rather than shipping
  unpinned. That help is `--help` text,
  so adding it makes the checked-in manual page and the shell completions
  stale: regenerate them with `python3 tools/release_extras.py --binary
  rust/target/debug/ocomment`, copy `release-extras/ocomment.1` to `docs/`, and
  run `python3 tools/gen_docs.py --binary rust/target/debug/ocomment` for the
  generated pages.
- An interpreter name a `#!` line is read for is searched for as a *substring*
  of that line, because an interpreter arrives written a dozen ways: as a path,
  with a version, or behind `env` with options. The order of `SHEBANGS` in
  `rust/ocomment-core/src/detect.rs` is therefore part of the rule and not an
  accident of listing — a name another name *contains* has to be met first, or
  every Bash script on disk would be read as POSIX `sh`. A name too short to be
  looked for that way is what the table carries a `Spelling` for: `r`, the front
  end littler installs, is one letter, and `/usr/` alone carries one, so it is
  compared against the whole words of the line and is listed last. Publish every
  name in `spec/languages.toml` in the same change:
  `the_detector_knows_no_unrecorded_shebang` compares that list against
  `ocomment_core::shebang_interpreters` in both directions, and
  `every_listed_shebang_detects_its_language` runs the detector over
  `#!/usr/bin/env <name>` for each one.
- A language whose lexical mode is document state rather than line state — PHP,
  where the same line means one thing under an unclosed `<?php` and another
  without it — must offer a safe checkpoint only in its outermost state. Emit
  none inside the state, and the incremental engine is sound without a rule of
  its own: it only ever restarts at an offset the previous full scan emitted and
  whose preceding bytes an edit has not touched. `RestartRules` is for the other
  shape, a construct whose *end* is decided by the bytes below a checkpoint;
  `first_yaml_block_scalar` is the one instance. A third shape lives there too:
  a rule that reads the *absolute* offset rather than the bytes around it. A
  `#!` line is a preamble only at the first byte, and a source-encoding
  declaration only inside the first two lines, so a language that declares one —
  Python and Ruby are the two — belongs in `the_preamble_permits_a_restart`,
  which is what refuses a restart at a line a full scan would have classified
  differently. The proptest
  `arbitrary_incremental_edits_match_full_scans_for_every_builtin` covers every
  language automatically, but only over the byte fragments its generator knows,
  so add the tokens that open the new state to `ocomment_core::lexical_pool` —
  `BYTES` for a single byte and `TOKENS` for a whole opener, with the reason
  written beside the list — and run the incremental properties five times over
  with `PROPTEST_CASES=5000`, all five green:

  ```sh
  for i in 1 2 3 4 5; do
    PROPTEST_CASES=5000 cargo test -p ocomment-core --locked -- \
      incremental arbitrary_incremental safe_checkpoints arbitrary_edits
  done
  ```

  Five runs rather than one because each draws a fresh seed: the counterexamples
  that matter here are two rare fragments meeting in one document, and a single
  pass of 2,000 cases was what let a character-literal lookahead read across a
  line terminator for as long as it did. The
  pool is one place on purpose: two generators draw from it, one in
  `rust/ocomment-core/src/incremental.rs` and one in
  `rust/ocomment-core/tests/properties.rs`, and
  `rust/ocomment-core/tests/source_guards.rs` reads both of them and fails a
  `prop_oneof!` arm that spells a literal of its own, so an opener added to one
  suite alone is refused rather than silently leaving the other blind to it.
- A language whose *lexer* ends a line on more than `\r` and `\n` still offers a
  checkpoint after those two alone. C# is the one that has more — ECMA-334 6.3.1
  counts U+0085, U+2028 and U+2029 besides, and Roslyn ends a `//` comment at all
  five, so the scanner has to as well or a removal would swallow the code behind
  one on the same physical line. The *restart* rules are a different question:
  `the_line_ending_permits_a_restart` and the preamble window are written in `\r`
  and `\n`, and the incremental engine consults them over the edited bytes, so a
  checkpoint after a U+2028 would be a line start only one of the two agreed on.
  Refusing it costs a rescan the chance to begin there and nothing else, which is
  the same trade JavaScript makes for its own two.
- A lookahead that can read past the byte the scan resumes at takes `&mut Reach`
  and records every byte it consults — the failed `get` at the end of the
  document included, because deciding that the document ends there is a decision
  about that byte just the same — and its caller folds the result in with
  `Scanner::consult`. That is what makes `Scanner::add_safe_checkpoint` a
  mechanism rather than an audit: it refuses outright any position an earlier
  decision already read through. A plain forward scan is not a lookahead and
  reports nothing — its index consumes every byte it reads, so a restart inside
  the region reaches the same answer for what is left of it. What has to be
  reported is the read the scan then *rewinds* behind: a delimiter parse that
  gives up and lexes the same bytes again, a search for a closing token that
  fails. Prefer a bound to a withdrawal. A search for a tag terminator reads the
  tag's own character class and one byte more — `ocaml_quoted_string` reads
  `[a-z_]*` and then requires `|`, `sql_dollar_quote_end` an identifier and then
  `$` — so an ordinary `{` or `$` in the code gives up where the class does,
  rather than pushing the watermark to the end of the file and costing every line
  under it its restart point. Assert either kind directly with
  `scanner::scan_checkpoint_watermarks`, which pairs each checkpoint with the
  watermark standing when it was offered;
  `safe_checkpoints_restart_every_builtin_scan_exactly` runs it over every
  language, and a reach that is merely unpinned still passes a rescan that
  happens to re-lex the same bytes, so pin it with a unit test on the lookahead
  itself.
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
