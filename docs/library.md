# Library

`ocomment-core` is the engine the CLI, the LSP server, and the plugin host are
all built on, published as an ordinary crate. It does no I/O: it takes bytes and
returns what it found and what it would write.

```sh
cargo add ocomment-core
```

The complete API reference lives on docs.rs and is generated from the source, so
it is the authority on every type and every field:

- [`ocomment-core`](https://docs.rs/ocomment-core) — the scanner, the policy, the
  transform, and the source map.
- [`ocomment-plugin-sdk`](https://docs.rs/ocomment-plugin-sdk) — for writing a
  WebAssembly scanner plugin; see [Plugins](plugins.md).
- [`ocomment`](https://docs.rs/ocomment) — the CLI crate, if you need to depend
  on the binary's own version metadata.

Every example on this page is a doctest in the crate, so CI compiles and runs it
on every pull request. The longer ones are checked-in examples you can run:

```sh
cargo run -p ocomment-core --example strip
cargo run -p ocomment-core --example external_spans
cargo run -p ocomment-core --example incremental
cargo run -p ocomment-core --example profile
```

## The three calls

`scan` reports. `transform` reports and also gives you the bytes. `apply_edits`
is the last step of `transform`, exposed on its own for a caller that wants to
filter or postpone the edits.

```rust
use ocomment_core::{CommentKind, Language, ScanOptions, scan};

let report = scan(b"let x = 1; // note\n", Language::Rust, ScanOptions::default());
assert_eq!(report.comments.len(), 1);
assert_eq!(report.comments[0].kind, CommentKind::Line);
assert!(report.comments[0].disposition.is_remove());
```

```rust
use ocomment_core::{Language, TransformOptions, transform};

// A BOM, a CRLF ending, and a comment to take out.
let source = "\u{feff}fn main() {} // trailing\r\n".as_bytes();
let result = transform(source, Language::Rust, TransformOptions::default());
assert_eq!(result.output, "\u{feff}fn main() {} \r\n".as_bytes());
```

```rust
use ocomment_core::{Language, TransformOptions, apply_edits, transform};

let source = b"let x = 1; // note\nlet y = 2; // and\n";
let result = transform(source, Language::Rust, TransformOptions::default());

// Sorted and non-overlapping, so one pass applies them.
assert!(
    result
        .edits
        .windows(2)
        .all(|pair| pair[0].span.end <= pair[1].span.start)
);
assert_eq!(apply_edits(source, &result.edits), result.output);
```

A `TransformResult` carries the `output`, the `edits` that produced it, the
`report` those edits came from, and a `source_map` from the original byte
offsets to the new ones — which is what lets an editor keep a cursor, a
diagnostic, or a breakpoint pointing at the right place after a removal.

## What the engine guarantees

- Byte spans are half-open: `span.start..span.end`.
- Edits are sorted and non-overlapping, so applying them in order is enough.
- The source is never required to be valid UTF-8. A BOM, CRLF line endings, a
  missing trailing newline, and non-UTF-8 bytes outside the edited spans all
  come back unchanged.
- The output of a scan is deterministic for the same bytes, language, and
  options — that is what the OCaml reference implementation is compared
  against.

## What survives

Every comment is classified as a `CommentKind` first — from its delimiters, then
from its own text and position — and the `Policy` then decides that kind:

| Kind | `safe` | `legal` | `all` |
| --- | --- | --- | --- |
| `line`, `block`, `doc-line`, `doc-block` | remove | remove | remove |
| `license` | remove | keep | remove |
| `directive`, `html-comment`, `optimizer-hint`, `version-comment` | keep | keep | remove |
| `shebang`, `encoding` | keep | keep | keep unless forced |

The policy is the last word rather than the first: `keep_kinds`, `keep_regex`,
`remove_kinds` and `remove_regex` on `ScanOptions` are all tested before it, in
that order. [Policies](policies.md) is the same table from the binary's own
mouth, and [Why a comment was kept](why-kept.md) lists what makes a comment a
directive.

`explain_disposition` answers *why* for one comment, naming the rule that
applied rather than summarising it:

```rust
use ocomment_core::{
    Action, CommentKind, DispositionExplanation, Language, Policy, ScanOptions,
    explain_disposition,
};

let mut options = ScanOptions::default();
let why = explain_disposition(CommentKind::Line, b"// note", Language::Rust, &options);
assert_eq!(why.action(), Action::Remove);
assert!(matches!(why, DispositionExplanation::RemovedByDefault(Policy::Safe)));

options.keep_regex.push(r"^//\s*NOTE\b".into());
let kept = explain_disposition(CommentKind::Line, b"// NOTE: why", Language::Rust, &options);
assert_eq!(kept.action(), Action::Keep);
assert!(matches!(kept, DispositionExplanation::KeptByRegex { index: 0, .. }));
assert_eq!(kept.to_string(), r"kept: matched keep_regex #0 `^//\s*NOTE\b`");
```

`explain_disposition_with` takes pattern sets compiled once, which is what you
want when explaining a whole file rather than one comment.

## Choosing the language

`detect_language` resolves a path and its contents to a `Language`, and
`Language` can also be named directly when you already know it — which is what
you want for a buffer that has no path. [Languages and dialects](languages.md)
lists everything built in, and a profile describes a delimiter-based syntax
that is not.

```rust
use std::path::Path;
use ocomment_core::{Dialect, Language, detect_language};

let found = detect_language(Some(Path::new("src/app.tsx")), b"").unwrap();
assert_eq!(found.language, Language::TypeScript);
assert_eq!(found.dialect, Dialect::Tsx);
assert_eq!(found.reason, "extension");

// No name, so the shebang decides.
let piped = detect_language(None, b"#!/usr/bin/env python3\n").unwrap();
assert_eq!(piped.language, Language::Python);
```

## A scanner of your own

`transform_spans` takes comment spans an external scanner already found and puts
them through the same policy, layout, edit validation, and source map as a
built-in scan, after checking that the spans are non-empty, sorted,
non-overlapping, and inside the source. That is the hand-off point a
WebAssembly plugin uses, and it is the one to use for a scanner you would rather
keep in your own process.

```rust
use ocomment_core::{
    ByteSpan, CommentKind, ExternalSpanError, Language, TransformOptions, transform_spans,
};

let source = b"a/* ordinary */b/* directive */";
let result = transform_spans(
    source,
    Language::Unknown,
    &[
        (ByteSpan::new(1, 15), CommentKind::Block),
        (ByteSpan::new(16, source.len()), CommentKind::Directive),
    ],
    TransformOptions::default(),
)
.unwrap();
// The same policy the built-in scanners get: the directive is kept.
assert_eq!(result.output, b"a b/* directive */");

let bad = transform_spans(
    source,
    Language::Unknown,
    &[(ByteSpan::new(2, source.len() + 1), CommentKind::Block)],
    TransformOptions::default(),
);
assert!(matches!(bad, Err(ExternalSpanError::OutOfBounds { .. })));
```

A `DeclarativeProfile` is the smaller answer: literal comment and string
delimiters, read in a single byte-oriented pass. It needs no code, and what it
cannot express it refuses rather than guesses — `validate_profile` rejects a
delimiter that is a prefix of another, a nested block whose tokens overlap, and
a delimiter containing a line terminator, because none of those has a single
reading.

```rust
use ocomment_core::{
    CommentKind, DeclarativeProfile, LineDelimiter, StringDelimiter, TransformOptions,
    transform_profile,
};

let profile = DeclarativeProfile {
    name: "lisp".into(),
    extensions: vec!["lisp".into()],
    line_comments: vec![LineDelimiter {
        start: ";;".into(),
        requires_boundary: false,
        kind: CommentKind::Line,
    }],
    strings: vec![StringDelimiter {
        start: "\"".into(),
        end: "\"".into(),
        escape: Some("\\".into()),
        multiline: false,
    }],
    ..Default::default()
};

let source = b"(print \";; not a comment\") ;; a comment\n";
let result = transform_profile(source, &profile, TransformOptions::default()).unwrap();
assert_eq!(result.output, b"(print \";; not a comment\") \n");
```

The same profile can be written in `.ocomment.toml` instead, which is what most
callers want; see [Configuration](configuration.md).

## Editing a live buffer

`IncrementalDocument` applies `DocumentChange`s and rescans only what moved,
under a `PositionEncoding` of UTF-8, UTF-16, or UTF-32. That is the path the LSP
server takes, and it is the one to use for anything that rescans on every
keystroke rather than on every save.

`apply_changes` is transactional. A batch that fails validation — a stale
version, an inverted span, a span reaching past the end — leaves the source, the
report, the checkpoints, and the version exactly as they were, so a
misbehaving client cannot corrupt the document:

```rust
use ocomment_core::{
    ByteSpan, DocumentChange, IncrementalDocument, IncrementalError, Language, ScanOptions,
};

let mut document = IncrementalDocument::new(
    b"let x = 1; // note\n".to_vec(),
    Language::Rust,
    ScanOptions::default(),
    1,
);

// A span past the end of the document is refused, and nothing moves.
let outside = ByteSpan::new(0, document.source().len() + 1);
assert_eq!(
    document.apply_changes(
        &[DocumentChange {
            span: outside,
            replacement: Vec::new(),
        }],
        2,
    ),
    Err(IncrementalError::InvalidSpan),
);
assert_eq!(document.version(), 1);
assert_eq!(document.report().comments.len(), 1);
```

`last_rescan_span` reports how much of the document the last edit actually cost,
so the saving is measurable rather than assumed.

## Versioning

The workspace follows semantic versioning, and every crate in it is released
at the same version by the same tag. The minimum supported Rust version is
1.88, and a dedicated CI job builds the workspace against exactly that
toolchain on every pull request, so a dependency that quietly raises it fails
the build rather than a user's install.

Both library crates are documented item by item: `missing_docs` is denied in
CI, the doctests on this page run there, and `cargo doc` runs with
`-D warnings`, so a public item added without documentation, an example that
stops compiling, or a broken intra-doc link fails the build. The `ocomment`
binary crate is documented in the same run for its links alone — nothing
publishes its rustdoc, but its modules describe each other, and a link naming
an item somebody has since renamed is a wrong sentence wherever it is written.
