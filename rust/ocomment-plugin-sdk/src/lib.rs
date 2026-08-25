//! Versioned scanner-plugin boundary.

use ocomment_core::{ByteSpan, CommentKind};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const API_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PluginComment {
    pub span: ByteSpan,
    pub kind: CommentKind,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ValidationError {
    #[error("plugin API version {received} is unsupported; host supports {supported}")]
    ApiVersion { received: u32, supported: u32 },
    #[error("plugin comment span is outside the {source_len}-byte source")]
    OutOfBounds { source_len: usize },
    #[error("plugin spans are not strictly sorted and non-overlapping")]
    Overlap,
    #[error("plugin comment spans must not be empty")]
    EmptySpan,
    #[error("plugin returned more than the allowed {limit} spans")]
    SpanLimit { limit: usize },
}

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
