# ocomment-core

`ocomment-core` is OComment's byte-oriented public scanning and transformation
library. Its stable entry points are `scan`, `transform`, and `apply_edits`.
Inputs need not be valid UTF-8; all canonical positions are half-open byte
ranges.

`IncrementalDocument` accepts transactional, sorted edit batches. It resumes
from a lexically neutral line checkpoint, stops when the unchanged lexical
state converges, and reuses the untouched tail. `last_rescan_span()` exposes
the actual scan window; invalid source falls back to a conservative full scan.
UTF-8, UTF-16, and UTF-32 position conversion is available through
`PositionEncoding`.

See the [project README](https://github.com/P4suta/OComment) for supported
languages, policies, and examples.
