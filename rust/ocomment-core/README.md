# ocomment-core

`ocomment-core` is OComment's byte-oriented public scanning and transformation
library. Its stable entry points are `scan`, `transform`, `transform_plan`,
and `apply_edits`.
Inputs need not be valid UTF-8; all canonical positions are half-open byte
ranges.

`PreparedScanner` compiles an effective `ScanOptions` policy once for reuse
across files and embedded-language scans. `TransformPlan` carries the report
and edits without allocating transformed bytes or a source map; `finish()`
materializes the compatible `TransformResult` only when a caller needs it.

`IncrementalDocument` accepts transactional, sorted edit batches. It resumes
from a lexically neutral line checkpoint, stops when the unchanged lexical
state converges, and reuses the untouched tail. `last_rescan_span()` exposes
the actual scan window; invalid source falls back to a conservative full scan.
UTF-8, UTF-16, and UTF-32 position conversion is available through
`PositionEncoding`.

See the [project README](https://github.com/P4suta/OComment) for supported
languages, policies, and examples.
