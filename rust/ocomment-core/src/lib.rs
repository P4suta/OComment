//! Byte-oriented scanning and transformation for OComment.
//!
//! `ocomment-core` finds the comments in a source file and works out which of
//! them may be removed. It does no I/O: it takes bytes and hands back what it
//! found and what it would write. The CLI, the LSP server, and the plugin host
//! are all built on the calls below.
//!
//! # Byte-preserving
//!
//! Byte offsets are the canonical coordinate system, and the engine never
//! decodes the complete input as UTF-8. Every byte outside a removed comment is
//! copied through untouched, so a BOM, CRLF line endings, a missing final
//! newline, and source bytes that are not UTF-8 at all all survive a
//! transformation unchanged. Nothing is reformatted, reindented, or reordered:
//! the only bytes that move are the ones a comment occupied.
//!
//! Three rules hold everywhere in this crate:
//!
//! - A [`ByteSpan`] is half-open, `start..end`, counted in bytes.
//! - The [`Edit`]s of a [`TransformResult`] are sorted by `span.start` and
//!   never overlap, so applying them in order in one pass is enough.
//! - The same bytes, language, and options always give the same answer. An
//!   independent OCaml implementation is compared against this one byte for
//!   byte.
//!
//! # The three calls
//!
//! [`scan`] reports. [`transform`] reports and also gives you the bytes.
//! [`apply_edits`] is the last step of `transform`, exposed on its own for a
//! caller that wants to filter or postpone the edits.
//!
//! ```
//! use ocomment_core::{CommentKind, Language, ScanOptions, scan};
//!
//! let report = scan(b"let x = 1; // note\n", Language::Rust, ScanOptions::default());
//! assert_eq!(report.comments.len(), 1);
//! assert_eq!(report.comments[0].kind, CommentKind::Line);
//! assert!(report.comments[0].disposition.is_remove());
//! ```
//!
//! ```
//! use ocomment_core::{Language, TransformOptions, transform};
//!
//! // A BOM, a CRLF ending, and a comment to take out.
//! let source = "\u{feff}fn main() {} // trailing\r\n".as_bytes();
//! let result = transform(source, Language::Rust, TransformOptions::default());
//! assert_eq!(result.output, "\u{feff}fn main() {} \r\n".as_bytes());
//! ```
//!
//! ```
//! use ocomment_core::{Language, TransformOptions, apply_edits, transform};
//!
//! let source = b"let x = 1; // note\nlet y = 2; // and\n";
//! let result = transform(source, Language::Rust, TransformOptions::default());
//!
//! // Sorted and non-overlapping, so one pass applies them.
//! assert!(
//!     result
//!         .edits
//!         .windows(2)
//!         .all(|pair| pair[0].span.end <= pair[1].span.start)
//! );
//! assert_eq!(apply_edits(source, &result.edits), result.output);
//! ```
//!
//! A [`TransformResult`] also carries a [`SourceMap`] between the original
//! offsets and the new ones, which is what lets an editor keep a cursor, a
//! diagnostic, or a breakpoint pointing at the right place after a removal.
//!
//! # What survives
//!
//! Every comment is classified as a [`CommentKind`] first — from its
//! delimiters, then from its own text and position — and the [`Policy`] then
//! decides that kind:
//!
//! | Kind | [`Policy::Safe`] | [`Policy::Legal`] | [`Policy::All`] |
//! | --- | --- | --- | --- |
//! | `line`, `block`, `doc-line`, `doc-block` | remove | remove | remove |
//! | `license` | remove | keep | remove |
//! | `directive`, `html-comment`, `optimizer-hint`, `version-comment` | keep | keep | remove |
//! | `shebang`, `encoding` | keep | keep | keep unless forced |
//!
//! The shebang and the encoding declaration are the two a source needs to keep
//! working, so even [`Policy::All`] leaves them until
//! [`ScanOptions::force_protected`] says otherwise. The policy is the last word
//! rather than the first: [`ScanOptions::keep_kinds`],
//! [`ScanOptions::keep_regex`], [`ScanOptions::remove_kinds`] and
//! [`ScanOptions::remove_regex`] are all tested before it.
//!
//! [`explain_disposition`] answers *why* for one comment, naming the rule that
//! applied rather than summarising it, so a caller can quote the pattern, kind,
//! or directive back to a user. [`explain_disposition_with`] takes pattern sets
//! compiled once for a whole file. [`explain_comment`] answers it for a comment
//! a scan produced, which is the same answer plus the one rule a comment's own
//! bytes cannot account for: a YAML block scalar leaning on the comment that
//! ends it keeps that comment because of where it sits.
//!
//! # Scanners this crate does not have
//!
//! [`transform_spans`] takes comment spans an external scanner already found
//! and puts them through the same policy, layout, edit validation, and source
//! map as a built-in scan, after checking that the spans are non-empty, sorted,
//! non-overlapping, and inside the source. That is the hand-off point for a
//! WebAssembly plugin.
//!
//! A [`DeclarativeProfile`] is the smaller answer: literal comment and string
//! delimiters, read in a single byte-oriented pass, with the ambiguities that
//! would make that pass wrong rejected up front by [`validate_profile`]. It
//! needs no code, and it cannot express a syntax whose comments depend on more
//! than delimiters.
//!
//! # Editing a live buffer
//!
//! [`IncrementalDocument`] rescans only what an edit disturbed and is the path
//! the LSP server takes. [`IncrementalDocument::apply_changes`] is
//! transactional: a batch that fails validation leaves the source, the report,
//! the checkpoints, and the version exactly as they were, so a client that
//! sends a stale or malformed batch cannot corrupt the document.
//!
//! ```
//! use ocomment_core::{
//!     ByteSpan, DocumentChange, IncrementalDocument, IncrementalError, Language, ScanOptions,
//! };
//!
//! let mut document = IncrementalDocument::new(
//!     b"let x = 1; // note\n".to_vec(),
//!     Language::Rust,
//!     ScanOptions::default(),
//!     1,
//! );
//!
//! // A span past the end of the document is refused, and nothing moves.
//! let outside = ByteSpan::new(0, document.source().len() + 1);
//! assert_eq!(
//!     document.apply_changes(
//!         &[DocumentChange {
//!             span: outside,
//!             replacement: Vec::new(),
//!         }],
//!         2,
//!     ),
//!     Err(IncrementalError::InvalidSpan),
//! );
//! assert_eq!(document.version(), 1);
//! assert_eq!(document.report().comments.len(), 1);
//! ```

mod detect;
mod incremental;
mod profile;
mod scanner;
mod transform;
mod types;

pub use detect::{Detection, detect_language, shebang_interpreters};
pub use incremental::{DocumentChange, IncrementalDocument, IncrementalError, PositionEncoding};
pub use profile::{
    BlockDelimiter, DeclarativeProfile, LineDelimiter, ProfileError, ProtectedPattern,
    StringDelimiter, scan_profile, transform_profile, validate_profile,
};
pub use scanner::{
    DispositionPatterns, explain_comment, explain_comment_with, explain_disposition,
    explain_disposition_with, scan,
};
pub use transform::{apply_edits, transform, transform_spans};
pub use types::*;
