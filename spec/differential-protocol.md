# Differential protocol v1

Each JSONL request is one object with `id`, `operation` (`scan`, `transform`,
`apply_edits`, `transform-spans`, `scan-profile`, or `transform-profile`),
`language`, byte-preserving `source_base64`, and options. External-span requests
add ordered `{start,end,kind}` entries; profile requests add a declarative
profile object; `apply_edits` requests add ordered
`{span:{start,end},replacement_base64}` entries. Each
response repeats `id` and contains either `ok` with the normalized result or
`error`. Spans are half-open byte offsets; comments and edits are ordered by
`(start,end)`; object keys are serialized in protocol order. Binary payloads use
base64 so invalid UTF-8 is never normalized by a JSON implementation.

`id` is echoed back unchanged and is not interpreted. `tools/differential.py`
sends the fixture id, so a response names the case it belongs to.

The Rust conformance driver and `ocomment-ref` must compare comment spans,
classification, dispositions, diagnostics, edits, safe/all output, and source
map segments byte-for-byte.

## Where the requests come from

Every request is built from one case of the fixture corpus in
`spec/fixtures/v1/*.json`, which is the single source of truth for what the two
implementations are asked and what they are expected to answer. No fixture
bytes live in `tools/differential.py`, in the Rust test suite, or in the OCaml
reference; adding a hazard to a runner instead of to the corpus hides it from
the other runner.

A case maps onto a request directly, under the same names:

| Case field | Request |
| --- | --- |
| `id` | `id` |
| `language` | `language` |
| `dialect` | `options.dialect` |
| `operation` | `operation`, defaulting to `transform` |
| `options` | `options`, over the defaults `{"policy": "safe", "layout": "lines"}` |
| `source_utf8` or `source_base64` | `source_base64` |
| `spans`, `edits`, `profile` | the same key, unchanged |

`note` and `expect` are not sent. `expect` is the result the corpus has
recorded for the case: `tools/differential.py` checks it against the agreed
response, so the corpus pins absolute behaviour and not only agreement, and
`rust/ocomment-core/tests/spec_fixtures.rs` checks the same blocks with no
OCaml toolchain in sight. `spec/fixtures/README.md` documents the schema and
how a block is recorded.
