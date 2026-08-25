# Differential protocol v1

Each JSONL request is one object with `id`, `operation` (`scan`, `transform`,
`apply_edits`, `transform-spans`, `scan-profile`, or `transform-profile`),
`language`, byte-preserving `source_base64`, and options. External-span requests
add ordered `{start,end,kind}` entries; profile requests add a declarative
profile object. Each
response repeats `id` and contains either `ok` with the normalized result or
`error`. Spans are half-open byte offsets; comments and edits are ordered by
`(start,end)`; object keys are serialized in protocol order. Binary payloads use
base64 so invalid UTF-8 is never normalized by a JSON implementation.

The Rust conformance driver and `ocomment-ref` must compare comment spans,
classification, dispositions, diagnostics, edits, safe/all output, and source
map segments byte-for-byte.
