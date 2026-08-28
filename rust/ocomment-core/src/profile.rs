use crate::{
    ByteSpan, Comment, CommentKind, Diagnostic, Language, Layout, PreparedScanner, ScanOptions,
    ScanReport, Severity, TransformOptions, TransformPlan, TransformResult,
    scanner::{DispositionPatterns, disposition},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A deliberately limited scanner profile for unambiguous comment syntaxes.
///
/// A profile describes a syntax whose comments and strings are literal
/// delimiters and nothing more, so that one byte-oriented pass can find them
/// with no grammar and no backtracking. That is the whole of what it can
/// express, and the limits are enforced rather than assumed:
///
/// - Every delimiter is a literal token. It must not be empty and must not
///   contain a line terminator.
/// - No comment delimiter may be a prefix of another comment delimiter, no
///   string delimiter of another string delimiter, and no comment delimiter of
///   a string delimiter or the reverse. One position therefore never has two
///   readings, which is what makes the single pass correct.
/// - A nested block needs a start and an end that are distinct and neither
///   contained in the other, so the depth count cannot be fooled.
/// - A comment's [`CommentKind`] is whatever the delimiter declares. There is
///   no classification by content the way a built-in scanner does it: a
///   profile finds no shebang, no encoding line, and no license notice unless
///   a [`ProtectedPattern`] says so.
///
/// A syntax that needs more than this — a regex literal, a heredoc, an
/// indentation rule — needs a scanner plugin instead.
///
/// # Examples
///
/// ```
/// use ocomment_core::{
///     CommentKind, DeclarativeProfile, LineDelimiter, StringDelimiter, TransformOptions,
///     transform_profile,
/// };
///
/// let profile = DeclarativeProfile {
///     name: "lisp".into(),
///     extensions: vec!["lisp".into()],
///     line_comments: vec![LineDelimiter {
///         start: ";;".into(),
///         requires_boundary: false,
///         kind: CommentKind::Line,
///     }],
///     strings: vec![StringDelimiter {
///         start: "\"".into(),
///         end: "\"".into(),
///         escape: Some("\\".into()),
///         multiline: false,
///     }],
///     ..Default::default()
/// };
///
/// let source = b"(print \";; not a comment\") ;; a comment\n";
/// let result = transform_profile(source, &profile, TransformOptions::default()).unwrap();
/// assert_eq!(result.output, b"(print \";; not a comment\") \n");
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeclarativeProfile {
    /// What to call the profile in a diagnostic. It must not be blank.
    pub name: String,
    /// The file extensions this profile claims, with or without the leading
    /// dot and matched case-insensitively. The scanner never reads this; it
    /// is for whoever picks a profile for a path.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Tokens that open a comment running to the end of the line.
    #[serde(default)]
    pub line_comments: Vec<LineDelimiter>,
    /// Tokens that open a comment running to a closing token.
    #[serde(default)]
    pub block_comments: Vec<BlockDelimiter>,
    /// String forms to skip, so a comment token inside one is only text.
    #[serde(default)]
    pub strings: Vec<StringDelimiter>,
    /// Substrings that turn a comment into a kept directive.
    #[serde(default)]
    pub protected_patterns: Vec<ProtectedPattern>,
}

/// A token that opens a comment running to the end of the line.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineDelimiter {
    /// The opening token.
    pub start: String,
    /// Only open a comment at the start of the source or after ASCII
    /// whitespace, so a token that also occurs inside an identifier does not
    /// swallow the rest of the line.
    #[serde(default)]
    pub requires_boundary: bool,
    /// The kind to record, which is what the policy then judges.
    #[serde(default)]
    pub kind: CommentKind,
}

/// A token pair that opens and closes a delimited comment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockDelimiter {
    /// The opening token.
    pub start: String,
    /// The closing token.
    pub end: String,
    /// Count nesting, so an inner `start` needs its own `end`. Requires a
    /// `start` and `end` that are distinct and neither contained in the other.
    #[serde(default)]
    pub nested: bool,
    /// The kind to record, which is what the policy then judges.
    #[serde(default)]
    pub kind: CommentKind,
}

/// A string form the scan skips over, so a comment token inside one is text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StringDelimiter {
    /// The opening token.
    pub start: String,
    /// The closing token, which may be the same as `start`.
    pub end: String,
    /// A token that protects the byte after it, such as `\\`.
    #[serde(default)]
    pub escape: Option<String>,
    /// Whether the string may cross a line terminator. When it may not, a
    /// line terminator ends it and an `unterminated-profile-string`
    /// diagnostic is raised.
    #[serde(default)]
    pub multiline: bool,
}

/// A substring that makes a comment a kept directive.
///
/// A comment whose text contains it is recorded as a
/// [`CommentKind::Directive`], which every policy but
/// [`Policy::All`](crate::Policy::All) keeps, and `reason` becomes the reason
/// on its [`Disposition::Keep`](crate::Disposition::Keep).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedPattern {
    /// The substring to look for, compared against the comment's text as
    /// lossy UTF-8.
    pub contains: String,
    /// Why such a comment is kept, phrased for a human. It must not be blank.
    pub reason: String,
}

/// Why a [`DeclarativeProfile`] cannot be interpreted.
///
/// Every variant is a limit of the single-pass design rather than a passing
/// problem with the input, so the same profile always fails the same way.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProfileError {
    /// The profile has no name to put in a diagnostic.
    #[error("profile name must not be empty")]
    EmptyName,
    /// The profile declares neither a line nor a block comment.
    #[error("profile must define at least one comment delimiter")]
    NoCommentDelimiter,
    /// The named token is the empty string, which would match everywhere.
    #[error("delimiter `{0}` must not be empty")]
    EmptyDelimiter(&'static str),
    /// Two comment delimiters where one is a prefix of the other.
    #[error("ambiguous delimiter prefix: `{0}` and `{1}`")]
    AmbiguousDelimiter(String, String),
    /// Two string delimiters where one is a prefix of the other.
    #[error("ambiguous string delimiter prefix: `{0}` and `{1}`")]
    AmbiguousStringDelimiter(String, String),
    /// A comment delimiter and a string delimiter where one is a prefix of
    /// the other, so one position could open either.
    #[error("ambiguous comment/string delimiter prefix: `{0}` and `{1}`")]
    CommentStringCollision(String, String),
    /// A nested block whose start and end are equal or overlap, which no
    /// depth count can read.
    #[error("nested block delimiters require distinct non-overlapping start and end tokens")]
    InvalidNesting,
    /// A delimiter spanning a line terminator, which a one-line token cannot.
    #[error("delimiter contains a newline")]
    NewlineDelimiter,
    /// A [`ProtectedPattern`] with nothing to look for or no reason to give.
    #[error("protected patterns need non-empty `contains` and `reason` values")]
    EmptyProtectedPattern,
    /// A `keep_regex` or `remove_regex` entry of the [`ScanOptions`] would not
    /// compile.
    #[error("invalid policy regex: {0}")]
    InvalidPolicyRegex(String),
}

/// Check that a profile is one the single-pass interpreter can read.
///
/// [`scan_profile`] calls this first, so validating separately is only worth
/// it to report a bad configuration before any file is opened.
///
/// # Errors
///
/// Returns the first [`ProfileError`] the profile runs into. The checks are
/// on the profile alone and never on a source, so the answer is the same
/// every time.
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
///
/// Strings are matched first, then line comments, then block comments, so a
/// comment token inside a string is never a comment. The report names
/// [`Language::Unknown`] — a profile is not one of the built-in languages —
/// and an unterminated string or block raises an
/// `unterminated-profile-string` or `unterminated-profile-comment`
/// diagnostic, which makes the report invalid.
///
/// # Errors
///
/// Returns a [`ProfileError`] when the profile itself is unreadable, which
/// is checked before the source is touched.
pub fn scan_profile(
    source: &[u8],
    profile: &DeclarativeProfile,
    options: ScanOptions,
) -> Result<ScanReport, ProfileError> {
    let prepared = PreparedScanner::new(options)
        .map_err(|error| ProfileError::InvalidPolicyRegex(error.to_string()))?;
    prepared.scan_profile(source, profile)
}

impl PreparedScanner {
    /// Scan a declarative profile with this scanner's already-compiled policy.
    pub fn scan_profile(
        &self,
        source: &[u8],
        profile: &DeclarativeProfile,
    ) -> Result<ScanReport, ProfileError> {
        scan_profile_with(source, profile, self.options(), &self.patterns)
    }

    /// Plan a declarative-profile transformation without materializing its
    /// output bytes or source map.
    pub fn transform_profile_plan(
        &self,
        source: &[u8],
        profile: &DeclarativeProfile,
        layout: Layout,
    ) -> Result<TransformPlan, ProfileError> {
        let report = self.scan_profile(source, profile)?;
        Ok(crate::transform::plan_report(
            source,
            report,
            layout,
            self.options().force_invalid,
        ))
    }
}

fn scan_profile_with(
    source: &[u8],
    profile: &DeclarativeProfile,
    options: &ScanOptions,
    patterns: &DispositionPatterns,
) -> Result<ScanReport, ProfileError> {
    validate_profile(profile)?;
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
                options,
                patterns,
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
                options,
                patterns,
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

/// Scan under a profile and produce the bytes a removal would write.
///
/// [`scan_profile`] followed by the same layout, edit validation, and
/// source-map engine the built-in languages go through, so the guarantees
/// of [`transform`](crate::transform) hold here too.
///
/// # Errors
///
/// Returns a [`ProfileError`] when the profile itself is unreadable.
pub fn transform_profile(
    source: &[u8],
    profile: &DeclarativeProfile,
    options: TransformOptions,
) -> Result<TransformResult, ProfileError> {
    let prepared = PreparedScanner::new(options.scan)
        .map_err(|error| ProfileError::InvalidPolicyRegex(error.to_string()))?;
    Ok(prepared
        .transform_profile_plan(source, profile, options.layout)?
        .finish(source))
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
