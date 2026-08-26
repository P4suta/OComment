//! Versioned scanner-plugin boundary.
//!
//! A scanner plugin finds comments in a syntax `ocomment-core` has no scanner
//! for and hands their spans back to the host, which puts them through the
//! ordinary policy with
//! [`transform_spans`](ocomment_core::transform_spans). This crate is the
//! contract between the two: the [`PluginComment`] a guest returns, the
//! [`API_VERSION`] it was built against, and the [`validate_comments`] check
//! the host runs before it trusts any of it.
//!
//! A plugin is untrusted code, so nothing it returns is taken on faith. The
//! host validates first and refuses the whole batch on the first fault; it
//! never removes bytes on the strength of a span it has not checked.
//!
//! ```
//! use ocomment_core::{ByteSpan, CommentKind};
//! use ocomment_plugin_sdk::{API_VERSION, PluginComment, ValidationError, validate_comments};
//!
//! let source = b"a ;; note\n";
//! let found = [PluginComment {
//!     span: ByteSpan::new(2, 9),
//!     kind: CommentKind::Line,
//! }];
//! assert!(validate_comments(source.len(), API_VERSION, &found).is_ok());
//!
//! // A guest built against another revision of the contract is refused
//! // before its spans are even read.
//! assert!(matches!(
//!     validate_comments(source.len(), API_VERSION + 1, &found),
//!     Err(ValidationError::ApiVersion { .. }),
//! ));
//! ```

use ocomment_core::{ByteSpan, CommentKind};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The revision of this contract that host and guest must agree on.
///
/// It is bumped whenever the shape of a [`PluginComment`] or the rules in
/// [`validate_comments`] change. A guest reports the version it was built
/// against and the host refuses anything else, so a plugin compiled against
/// an older SDK fails loudly instead of being misread.
pub const API_VERSION: u32 = 1;

/// One comment a plugin found.
///
/// A plugin reports where a comment is and what it is; it never decides
/// whether the comment is removed. That stays with the host's policy, so one
/// configuration governs built-in and plugin-scanned files alike.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PluginComment {
    /// Where the comment's bytes are, delimiters included.
    pub span: ByteSpan,
    /// What the comment is, which is what the host's policy then judges.
    pub kind: CommentKind,
}

/// Why a plugin's answer cannot be trusted.
///
/// Each variant is a way a guest could otherwise make the host remove bytes
/// it should not, or spend unbounded work trying.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ValidationError {
    /// The guest was built against a different revision of this contract.
    #[error("plugin API version {received} is unsupported; host supports {supported}")]
    ApiVersion {
        /// The version the guest reported.
        received: u32,
        /// The only version this host accepts, [`API_VERSION`].
        supported: u32,
    },
    /// A span is inverted or reaches past the end of the source.
    #[error("plugin comment span is outside the {source_len}-byte source")]
    OutOfBounds {
        /// The length of the source the spans had to fit in.
        source_len: usize,
    },
    /// A span starts before its predecessor ends, which no single-pass edit
    /// could apply.
    #[error("plugin spans are not strictly sorted and non-overlapping")]
    Overlap,
    /// A span covers no bytes, so it names no comment.
    #[error("plugin comment spans must not be empty")]
    EmptySpan,
    /// More spans than the source could hold comments, which is a guest
    /// spending the host's memory rather than reporting anything.
    #[error("plugin returned more than the allowed {limit} spans")]
    SpanLimit {
        /// The most spans this source could have justified.
        limit: usize,
    },
}

/// Check everything a plugin returned before the host acts on any of it.
///
/// The version is checked first, so a guest built against another revision is
/// refused before its spans are read at all. The spans must then each be
/// non-empty, inside the source, and start no earlier than the previous one
/// ended — the same contract
/// [`transform_spans`](ocomment_core::transform_spans) enforces, checked here
/// so a host can refuse a plugin's whole answer rather than a single span of
/// it. The count is capped as well: no source can hold more comments than it
/// has bytes, plus one.
///
/// # Errors
///
/// Returns the [`ValidationError`] for the first fault found. On any error
/// the batch is refused whole; there is no partial acceptance.
///
/// # Examples
///
/// ```
/// use ocomment_core::{ByteSpan, CommentKind};
/// use ocomment_plugin_sdk::{API_VERSION, PluginComment, ValidationError, validate_comments};
///
/// let comment = |start, end| PluginComment {
///     span: ByteSpan::new(start, end),
///     kind: CommentKind::Line,
/// };
///
/// assert!(validate_comments(10, API_VERSION, &[comment(0, 2), comment(2, 10)]).is_ok());
/// assert_eq!(
///     validate_comments(10, API_VERSION, &[comment(4, 7), comment(6, 8)]),
///     Err(ValidationError::Overlap),
/// );
/// assert_eq!(
///     validate_comments(10, API_VERSION, &[comment(9, 11)]),
///     Err(ValidationError::OutOfBounds { source_len: 10 }),
/// );
/// ```
pub fn validate_comments(
    source_len: usize,
    api_version: u32,
    comments: &[PluginComment],
) -> Result<(), ValidationError> {
    if api_version != API_VERSION {
        return Err(ValidationError::ApiVersion {
            received: api_version,
            supported: API_VERSION,
        });
    }
    let limit = source_len.saturating_add(1).min(1_000_000);
    if comments.len() > limit {
        return Err(ValidationError::SpanLimit { limit });
    }
    let mut cursor = 0;
    for (index, comment) in comments.iter().enumerate() {
        if comment.span.start > comment.span.end || comment.span.end > source_len {
            return Err(ValidationError::OutOfBounds { source_len });
        }
        if comment.span.is_empty() {
            return Err(ValidationError::EmptySpan);
        }
        if index > 0 && comment.span.start < cursor {
            return Err(ValidationError::Overlap);
        }
        cursor = comment.span.end;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(start: usize, end: usize) -> PluginComment {
        PluginComment {
            span: ByteSpan::new(start, end),
            kind: CommentKind::Line,
        }
    }

    #[test]
    fn accepts_only_bounded_sorted_nonempty_spans() {
        assert!(validate_comments(10, API_VERSION, &[item(0, 2), item(2, 10)]).is_ok());
        assert_eq!(
            validate_comments(10, API_VERSION, &[item(4, 7), item(6, 8)]),
            Err(ValidationError::Overlap)
        );
        assert_eq!(
            validate_comments(10, API_VERSION, &[item(3, 3)]),
            Err(ValidationError::EmptySpan)
        );
        assert_eq!(
            validate_comments(10, API_VERSION, &[item(9, 11)]),
            Err(ValidationError::OutOfBounds { source_len: 10 })
        );
    }

    #[test]
    fn rejects_api_mismatch_before_reading_spans() {
        assert!(matches!(
            validate_comments(0, API_VERSION + 1, &[]),
            Err(ValidationError::ApiVersion { .. })
        ));
    }
}
