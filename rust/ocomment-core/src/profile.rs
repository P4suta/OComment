use crate::{
    ByteSpan, Comment, CommentKind, Diagnostic, Language, ScanOptions, ScanReport, Severity,
    TransformOptions, TransformResult,
    scanner::{DispositionPatterns, disposition},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A deliberately limited scanner profile for unambiguous comment syntaxes.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeclarativeProfile {
    pub name: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub line_comments: Vec<LineDelimiter>,
    #[serde(default)]
    pub block_comments: Vec<BlockDelimiter>,
    #[serde(default)]
    pub strings: Vec<StringDelimiter>,
    #[serde(default)]
    pub protected_patterns: Vec<ProtectedPattern>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineDelimiter {
    pub start: String,
    #[serde(default)]
    pub requires_boundary: bool,
    #[serde(default)]
    pub kind: CommentKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockDelimiter {
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub nested: bool,
    #[serde(default)]
    pub kind: CommentKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StringDelimiter {
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub escape: Option<String>,
    #[serde(default)]
    pub multiline: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedPattern {
    pub contains: String,
    pub reason: String,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProfileError {
    #[error("profile name must not be empty")]
    EmptyName,
    #[error("profile must define at least one comment delimiter")]
    NoCommentDelimiter,
    #[error("delimiter `{0}` must not be empty")]
    EmptyDelimiter(&'static str),
    #[error("ambiguous delimiter prefix: `{0}` and `{1}`")]
    AmbiguousDelimiter(String, String),
    #[error("ambiguous string delimiter prefix: `{0}` and `{1}`")]
    AmbiguousStringDelimiter(String, String),
    #[error("ambiguous comment/string delimiter prefix: `{0}` and `{1}`")]
    CommentStringCollision(String, String),
    #[error("nested block delimiters require distinct non-overlapping start and end tokens")]
    InvalidNesting,
    #[error("delimiter contains a newline")]
    NewlineDelimiter,
    #[error("protected patterns need non-empty `contains` and `reason` values")]
    EmptyProtectedPattern,
    #[error("invalid policy regex: {0}")]
    InvalidPolicyRegex(String),
}

pub fn validate_profile(profile: &DeclarativeProfile) -> Result<(), ProfileError> {
    if profile.name.trim().is_empty() {
        return Err(ProfileError::EmptyName);
    }
    if profile.line_comments.is_empty() && profile.block_comments.is_empty() {
        return Err(ProfileError::NoCommentDelimiter);
    }
    let mut comments: Vec<&str> = Vec::new();
    for delimiter in &profile.line_comments {
        validate_token(&delimiter.start, "line start")?;
        comments.push(&delimiter.start);
    }
    for delimiter in &profile.block_comments {
        validate_token(&delimiter.start, "block start")?;
        validate_token(&delimiter.end, "block end")?;
        if delimiter.nested
            && (delimiter.start == delimiter.end
                || delimiter.start.contains(&delimiter.end)
                || delimiter.end.contains(&delimiter.start))
        {
            return Err(ProfileError::InvalidNesting);
        }
        comments.push(&delimiter.start);
    }
    let mut strings: Vec<&str> = Vec::new();
    for delimiter in &profile.strings {
        validate_token(&delimiter.start, "string start")?;
        validate_token(&delimiter.end, "string end")?;
        if let Some(escape) = &delimiter.escape {
            validate_token(escape, "string escape")?;
        }
        strings.push(&delimiter.start);
    }
    for (index, left) in comments.iter().enumerate() {
        for right in comments.iter().skip(index + 1) {
            if left.starts_with(*right) || right.starts_with(*left) {
                return Err(ProfileError::AmbiguousDelimiter(
                    (*left).into(),
                    (*right).into(),
                ));
            }
        }
        if let Some(right) = strings
            .iter()
            .find(|right| left.starts_with(**right) || right.starts_with(*left))
        {
            return Err(ProfileError::CommentStringCollision(
                (*left).into(),
                (**right).into(),
            ));
        }
    }
    for (index, left) in strings.iter().enumerate() {
        for right in strings.iter().skip(index + 1) {
            if left.starts_with(*right) || right.starts_with(*left) {
                return Err(ProfileError::AmbiguousStringDelimiter(
                    (*left).into(),
                    (*right).into(),
                ));
            }
        }
    }
    if profile
        .protected_patterns
        .iter()
        .any(|pattern| pattern.contains.is_empty() || pattern.reason.trim().is_empty())
    {
        return Err(ProfileError::EmptyProtectedPattern);
    }
    Ok(())
}

/// Interpret a validated declarative profile with a single byte-oriented pass.
pub fn scan_profile(
    source: &[u8],
    profile: &DeclarativeProfile,
    options: ScanOptions,
) -> Result<ScanReport, ProfileError> {
    validate_profile(profile)?;
    let patterns = DispositionPatterns::compile(&options)
        .map_err(|error| ProfileError::InvalidPolicyRegex(error.to_string()))?;
    let mut comments = Vec::new();
    let mut diagnostics = Vec::new();
    let mut index = 0;
    while index < source.len() {
        if let Some(string) = profile
            .strings
            .iter()
            .find(|string| starts(source, index, string.start.as_bytes()))
        {
            let start = index;
            index += string.start.len();
            let mut closed = false;
            while index < source.len() {
                if starts(source, index, string.end.as_bytes()) {
                    index += string.end.len();
                    closed = true;
                    break;
                }
                if let Some(escape) = &string.escape
                    && starts(source, index, escape.as_bytes())
                {
                    index = (index + escape.len() + 1).min(source.len());
                    continue;
                }
                if !string.multiline && matches!(source[index], b'\r' | b'\n') {
                    break;
                }
                index += 1;
            }
            if !closed {
                diagnostics.push(Diagnostic {
                    code: "unterminated-profile-string".into(),
                    message: format!("unterminated string in profile `{}`", profile.name),
                    severity: Severity::Error,
                    span: ByteSpan::new(start, index),
                });
            }
            continue;
        }
        if let Some(delimiter) = profile.line_comments.iter().find(|delimiter| {
            starts(source, index, delimiter.start.as_bytes())
                && (!delimiter.requires_boundary
                    || index == 0
                    || source[index - 1].is_ascii_whitespace())
        }) {
            let mut end = index + delimiter.start.len();
            while end < source.len() && !matches!(source[end], b'\r' | b'\n') {
                end += 1;
            }
            comments.push(profile_comment(
                source,
                index,
                end,
                delimiter.kind,
                profile,
                &options,
                &patterns,
            ));
            index = end;
            continue;
        }
        if let Some(delimiter) = profile
            .block_comments
            .iter()
            .find(|delimiter| starts(source, index, delimiter.start.as_bytes()))
        {
            let start = index;
            index += delimiter.start.len();
            let mut depth = 1usize;
            while index < source.len() {
                if delimiter.nested && starts(source, index, delimiter.start.as_bytes()) {
                    depth += 1;
                    index += delimiter.start.len();
                } else if starts(source, index, delimiter.end.as_bytes()) {
                    depth -= 1;
                    index += delimiter.end.len();
                    if depth == 0 {
                        break;
                    }
                } else {
                    index += 1;
                }
            }
            comments.push(profile_comment(
                source,
                start,
                index,
                delimiter.kind,
                profile,
                &options,
                &patterns,
            ));
            if depth != 0 {
                diagnostics.push(Diagnostic {
                    code: "unterminated-profile-comment".into(),
                    message: format!("unterminated block comment in profile `{}`", profile.name),
                    severity: Severity::Error,
                    span: ByteSpan::new(start, index),
                });
            }
            continue;
        }
        index += 1;
    }
    let valid = diagnostics.is_empty();
    Ok(ScanReport {
        language: Language::Unknown,
        comments,
        diagnostics,
        valid,
    })
}

pub fn transform_profile(
    source: &[u8],
    profile: &DeclarativeProfile,
    options: TransformOptions,
) -> Result<TransformResult, ProfileError> {
    let report = scan_profile(source, profile, options.scan.clone())?;
    Ok(crate::transform::transform_report(source, report, options))
}

fn profile_comment(
    source: &[u8],
    start: usize,
    end: usize,
    mut kind: CommentKind,
    profile: &DeclarativeProfile,
    options: &ScanOptions,
    patterns: &DispositionPatterns,
) -> Comment {
    let raw = String::from_utf8_lossy(&source[start..end]);
    let protected = profile
        .protected_patterns
        .iter()
        .find(|pattern| raw.contains(&pattern.contains));
    if protected.is_some() {
        kind = CommentKind::Directive;
    }
    let mut disposition = disposition(kind, options, &source[start..end], patterns);
    if let (Some(pattern), crate::Disposition::Keep { reason }) = (protected, &mut disposition) {
        *reason = pattern.reason.clone();
    }
    Comment {
        span: ByteSpan::new(start, end),
        kind,
        disposition,
    }
}

fn starts(source: &[u8], index: usize, token: &[u8]) -> bool {
    source.get(index..index.saturating_add(token.len())) == Some(token)
}

fn validate_token(token: &str, name: &'static str) -> Result<(), ProfileError> {
    if token.is_empty() {
        return Err(ProfileError::EmptyDelimiter(name));
    }
    if token.contains(['\r', '\n']) {
        return Err(ProfileError::NewlineDelimiter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_prefix_ambiguity() {
        let profile = DeclarativeProfile {
            name: "x".into(),
            line_comments: vec![
                LineDelimiter {
                    start: "/".into(),
                    requires_boundary: false,
                    kind: CommentKind::Line,
                },
                LineDelimiter {
                    start: "//".into(),
                    requires_boundary: false,
                    kind: CommentKind::Line,
                },
            ],
            ..Default::default()
        };
        assert!(matches!(
            validate_profile(&profile),
            Err(ProfileError::AmbiguousDelimiter(..))
        ));
    }

    #[test]
    fn scans_profile_without_looking_inside_strings() {
        let profile = DeclarativeProfile {
            name: "demo".into(),
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
        let report = scan_profile(b"\";; no\" ;; yes\n", &profile, ScanOptions::default()).unwrap();
        assert_eq!(report.comments.len(), 1);
        assert_eq!(
            &b"\";; no\" ;; yes\n"[report.comments[0].span.start..report.comments[0].span.end],
            b";; yes"
        );
    }

    #[test]
    fn rejects_empty_and_string_ambiguous_profiles() {
        assert_eq!(
            validate_profile(&DeclarativeProfile {
                name: "empty".into(),
                ..Default::default()
            }),
            Err(ProfileError::NoCommentDelimiter)
        );
        let profile = DeclarativeProfile {
            name: "ambiguous".into(),
            line_comments: vec![LineDelimiter {
                start: "#".into(),
                requires_boundary: false,
                kind: CommentKind::Line,
            }],
            strings: vec![StringDelimiter {
                start: "##".into(),
                end: "##".into(),
                escape: None,
                multiline: false,
            }],
            ..Default::default()
        };
        assert!(matches!(
            validate_profile(&profile),
            Err(ProfileError::CommentStringCollision(..))
        ));
    }
}
