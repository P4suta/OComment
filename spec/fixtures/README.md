# Shared fixtures

`v1/*.json` is the fixture corpus, and it is the single source of truth for
what OComment does to a hazardous source. Nothing about a case lives anywhere
else: no fixture bytes are hardcoded in `tools/differential.py`, in the Rust
test suite, or in the OCaml reference.

Two consumers read it, and both must pass:

- `tools/differential.py` turns every case into one request of the
  [differential protocol](../differential-protocol.md), feeds the corpus to the
  Rust engine and to the OCaml reference, and requires the two responses to be
  equal byte for byte. That says the pair agree.
- `rust/ocomment-core/tests/spec_fixtures.rs` runs every case against the Rust
  engine alone, so the corpus still holds on a machine with no OCaml toolchain.
  That says *what* they agree on.

Both also check a case's `expect` block, and both refuse to run a corpus that
has shrunk. The floors live in `v1/floor.txt` and nowhere else, so the two
runners cannot drift apart: `cases` is the least number of cases the corpus may
hold, and `expectations` the least number of those that must carry a recorded
expectation. `differential.py` enforces `cases` and requires `expectations` to
be present but does not enforce it — it is also the runner that *records* a
missing block, and a floor it enforced would refuse to run on the way to
putting one back. The Rust test, which never records, enforces both.

Two documents are in `v1/` today. The split is editorial, not structural — the
loaders concatenate every `*.json` in file-name order and require ids to be
unique across all of them. `floor.txt` sits beside them and is not a document:
both loaders read only `*.json`.

- `builtins.json` — one small source per built-in language, under both the
  `safe` and the `all` policy.
- `hazards.json` — the lexical hazards: raw strings, nested comments,
  heredocs, regex-versus-division, translation-phase escapes, dialect
  differences, malformed input, layout arithmetic, external spans, and
  declarative profiles.

## Case schema

```json
{
  "version": 1,
  "cases": [
    {
      "id": "sql-mysql-dash-boundary",
      "language": "sql",
      "dialect": "mysql",
      "operation": "transform",
      "options": { "policy": "safe", "layout": "lines" },
      "source_utf8": "select 1--2; -- remove\n",
      "note": "MySQL comment syntax requires `--` to be followed by whitespace...",
      "expect": {
        "valid": true,
        "comments": [{ "start": 13, "end": 22, "kind": "line", "action": "remove" }],
        "diagnostics": [],
        "output_utf8": "select 1--2; \n"
      }
    }
  ]
}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Unique across the whole corpus, and what a failure names. Meaningful, kebab-case, and stable: renaming one rewrites history for no gain. |
| `language` | yes | A built-in language name. `apply_edits` and the profile operations still need one; it is not read there. |
| `dialect` | no | The vendor rules to lex with. It sits beside `language` because the two together choose the grammar, while `options` holds the policy knobs. |
| `operation` | no | `transform` when absent. One of the protocol operations: `scan`, `transform`, `apply_edits`, `transform-spans`, `scan-profile`, `transform-profile`. |
| `options` | no | `ScanOptions` fields under their frozen serde names — `policy`, `force_invalid`, `force_protected`, `keep_kinds`, `remove_kinds`, `keep_regex`, `remove_regex` — plus `layout`. `policy` defaults to `safe` and `layout` to `lines`; everything else defaults as `ScanOptions::default` does. An unknown key is an error, not a no-op. |
| `source_utf8` / `source_base64` | exactly one | The source bytes. |
| `spans` | `transform-spans` | Ordered `{start, end, kind}` comment spans an external scanner is pretending to have found. |
| `edits` | `apply_edits` | Ordered `{span: {start, end}, replacement_base64}` edits. |
| `profile` | `*-profile` | A declarative profile object. |
| `note` | yes | The official lexical specification the case comes from, and what is supposed to happen. |
| `expect` | no, but see below | The recorded result. |

### Source encoding

Use `source_utf8` when the bytes are valid UTF-8 and every character is
ordinary text — newline, carriage return and tab are fine, and so are CJK
characters and emoji. Use `source_base64` otherwise: bytes that are not UTF-8
at all, C0 controls, U+2028 and U+2029, and format, surrogate, private-use or
unassigned characters. Those are exactly the characters a JSON tool, an editor,
or a terminal is liable to normalise, and a fixture whose bytes drift is worse
than no fixture. The same rule governs `expect.output_utf8` against
`expect.output_base64`.

A case whose source is *generated* — a sweep over a range of scalar values, a
long repeated tag — is recorded as fixed base64 rather than as the generator
that produced it. The point of the corpus is that both implementations see the
same bytes forever, and a generator is one refactor away from producing
different ones.

### `expect`

Every field is optional and every field present is checked, so a partial block
is a partial assertion rather than a weaker one.

| Field | Checked against |
| --- | --- |
| `valid` | `ScanReport::valid`. |
| `comments` | Every comment, in order, as `{start, end, kind, action}`. `action` is `keep` or `remove`; the human-readable keep reason is deliberately not pinned here. |
| `diagnostics` | Every diagnostic, in order, as `{code, start, end}`. An empty array asserts that there are none. |
| `output_utf8` / `output_base64` | The transformed bytes. |

An operation that reports nothing (`apply_edits`) takes only the output fields;
an operation that writes nothing (`scan`, `scan-profile`) takes only the report
fields.

Whether or not a case carries an `expect` block, the Rust test holds it to the
engine's structural promises: the run must not panic, and a transformation's
edits must be sorted, non-overlapping, inside the source, and must reproduce
the output when applied in one pass.

## Adding a case

1. Add the case to `hazards.json` — or to `builtins.json` if it is the plain
   one-source-per-language coverage. Never to a Python or Rust file: a hazard
   hardcoded in a runner is invisible to the other runner.
2. Write the `note` first. Name the clause of the official lexical
   specification the case exercises and say what is supposed to happen. A case
   whose expected behaviour cannot be sourced is a bug report, not a fixture.
3. Leave `expect` out for the moment and run
   `opam exec -- ./tools/differential.sh`. A mismatch between the two
   implementations is a real finding; settle it before recording anything.
4. Record `expect` with `python3 tools/differential.py --record` once that run
   is green, then run both consumers again.

Raising `cases` and `expectations` in `v1/floor.txt` is optional when adding a
case and mandatory when the corpus is reorganised: the floors exist so that
neither a case nor its recorded expectation can quietly disappear.

## Recording an `expect` block

An `expect` block is *recorded*, never hand-written:

```sh
cargo build --manifest-path rust/Cargo.toml -p ocomment-core --example ref_driver --locked
opam exec -- dune build --root ocaml bin/main.exe
python3 tools/differential.py --record
```

`--record` runs the whole corpus through both implementations first and records
nothing unless every case agreed, so a recorded block is a record of the
specification and not of whichever implementation was consulted. It fills in
only the cases that have no `expect` block, and rewrites a document only when
something changed; on an already-recorded corpus it is a no-op.

An output longer than about a kilobyte is left unrecorded rather than inlined.
`column-unicode-width-scalar-sample` is the one such case, and the comment span
and validity still pin it while the differential comparison covers the bytes.

Re-recording is deliberate. `--record` will not overwrite a block that is
already there, so an intentional behaviour change means deleting that case's
`expect` block and re-recording it in the same commit that argues for the
change. Changing a recorded value is a specification change, and needs what any
other one needs: the clause that now says otherwise, in the case `note` and in
`CHANGELOG.md`.
