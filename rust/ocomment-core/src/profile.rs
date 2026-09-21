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
///         kind: CommentKind::Line,
///         ..Default::default()
///     }],
///     strings: vec![StringDelimiter {
///         start: "\"".into(),
///         end: "\"".into(),
///         escape: Some("\\".into()),
///         ..Default::default()
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
    /// Whole file names this profile claims, matched case-sensitively.
    ///
    /// Some of the files most worth reaching have no extension at all --
    /// `dune`, `CODEOWNERS`, `Doxyfile` -- and a profile that could only be
    /// selected by suffix could not describe them. Case-sensitive because
    /// these names are conventions of the tools that read them, and those
    /// tools are case-sensitive about them.
    ///
    /// Like [`Self::extensions`], the scanner never reads this; it is for
    /// whoever picks a profile for a path.
    #[serde(default)]
    pub filenames: Vec<String>,
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
    /// Whether an ordinary line comment directly below a documentation one
    /// continues it.
    ///
    /// Some languages mark only the *first* line of a documentation comment
    /// and continue it with the ordinary opener. Haddock is written
    ///
    /// ```text
    /// -- | The first line is marked.
    /// --   The rest is not.
    /// ```
    ///
    /// and both lines are the documentation. Read one token at a time the
    /// second is a remark, and a policy that removes remarks would take half a
    /// published page away — which is the same loss removing a doc comment
    /// outright would be, arrived at by a route nothing was watching.
    ///
    /// A run is what continues: adjacent comment lines with no code and no
    /// blank line between them, which is what
    /// [`AllowRules::max_lines`](crate::AllowRules::max_lines) already
    /// measures. A blank line ends it, because that is how a writer says the
    /// next remark is a separate remark.
    ///
    /// Off by default. A language whose documentation comment marks every line
    /// — Rust's `///`, Gleam's — must leave it off: there a `//` under a `///`
    /// is a remark the author wrote deliberately.
    #[serde(default)]
    pub doc_continuation: bool,
}

/// A token that opens a comment running to the end of the line.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineDelimiter {
    /// The opening token.
    pub start: String,
    /// Only open a comment at the start of the source or after ASCII
    /// whitespace, so a token that also occurs inside an identifier does not
    /// swallow the rest of the line.
    #[serde(default)]
    pub requires_boundary: bool,
    /// Only open a comment when the token is the first byte of its line.
    ///
    /// Several formats give `#` that rule and only that rule: a `.gitignore`
    /// pattern may contain one -- `file#name` is a file called `file#name` --
    /// and `\\#literal` is how a pattern that starts with one is written. A
    /// profile that opened a comment at either wrote a shorter pattern back,
    /// so a default `fix` quietly stopped ignoring what the line named. The
    /// rule is the whole line's first byte and not "after whitespace", because
    /// leading whitespace in such a file is part of the pattern too.
    #[serde(default)]
    pub requires_line_start: bool,
    /// Characters that, coming directly after the token, mean it does not open
    /// a comment after all.
    ///
    /// The mirror of [`Self::requires_boundary`], which looks at the byte
    /// before. Several languages build operators out of the same characters
    /// their comment opens with, and the rule that tells the two apart is what
    /// comes next: in Haskell `-->` and `<--` are operators while `-- x` is a
    /// comment, and the clause that says so is Haskell 2010 §2.2.
    ///
    /// The token's final character may repeat before the test, because that is
    /// how such a language spells the token: Haskell's opener is a *run* of
    /// dashes, so `---x` is a comment and `---->` is an operator. A profile
    /// that left this empty is one where the question does not arise, and
    /// nothing repeats.
    ///
    /// Compared by byte, so only ASCII characters belong here.
    #[serde(default)]
    pub forbidden_after: String,
    /// The kind to record, which is what the policy then judges.
    #[serde(default)]
    pub kind: CommentKind,
}

/// A token pair that opens and closes a delimited comment.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
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
/// A comment whose text contains it is recorded under the kind its
/// [`ProtectionTier`] names, and `reason` becomes the reason on its
/// [`Disposition::Keep`](crate::Disposition::Keep).
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedPattern {
    /// The substring to look for, compared against the comment's text as
    /// lossy UTF-8.
    pub contains: String,
    /// Why such a comment is kept, phrased for a human. It must not be blank.
    pub reason: String,
    /// How strongly it is kept. Defaults to [`ProtectionTier::Tool`], which is
    /// what every profile written before this field existed asked for.
    #[serde(default)]
    pub tier: ProtectionTier,
}

/// How strongly a [`ProtectedPattern`] asks for its comment to be kept.
///
/// A profile describes a syntax this crate has no scanner for, and the person
/// writing one knows something about that syntax that the policy cannot: a
/// marker their toolchain reads is not the same as a marker their linter
/// reads, and only one of the two is a choice a policy gets to make. Without
/// the distinction every profile protection was the weaker one, so
/// [`Policy::All`](crate::Policy::All) removed a marker a build depended on
/// and the profile had no way to say otherwise.
///
/// The default is the weaker tier because that is what a profile written
/// without this field already meant, and because claiming the stronger one
/// should be an act rather than an accident.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProtectionTier {
    /// Addressed to a tool. Every policy but
    /// [`Policy::All`](crate::Policy::All) keeps it, recorded as
    /// [`CommentKind::Directive`].
    #[default]
    Tool,
    /// Read by the language or its build as part of the program. No policy
    /// removes it and only
    /// [`ScanOptions::force_protected`](crate::ScanOptions::force_protected)
    /// does, recorded as [`CommentKind::LoadBearing`].
    LoadBearing,
}

impl ProtectionTier {
    /// The comment kind a match under this tier is recorded as.
    pub const fn kind(self) -> CommentKind {
        match self {
            Self::Tool => CommentKind::Directive,
            Self::LoadBearing => CommentKind::LoadBearing,
        }
    }
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
            /* NOTE: Equal, not "a prefix of". One comment token being the
             * start of another is how a language spells a documentation
             * comment -- Gleam's `//`, `///` and `////`, Haskell's `--` and
             * `-- |` -- and the scan resolves it by taking the longest token
             * that matches, so the relationship carries no ambiguity. Two
             * delimiters spelled the same way do: nothing could choose between
             * them, and they would differ only in the kind they record. */
            if left == right {
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

/// Whether what follows the token — past any repetition of its final
/// character — leaves it opening a comment.
///
/// See [`LineDelimiter::forbidden_after`]. A delimiter that names no such
/// characters answers yes without reading anything, which is every profile
/// written before the field existed.
fn opens_past_its_run(source: &[u8], index: usize, delimiter: &LineDelimiter) -> bool {
    if delimiter.forbidden_after.is_empty() {
        return true;
    }
    let mut cursor = index + delimiter.start.len();
    if let Some(last) = delimiter.start.as_bytes().last() {
        while source.get(cursor) == Some(last) {
            cursor += 1;
        }
    }
    /* NOTE: The end of the source, and the end of the line, both open a
     * comment: an empty one is still one, and a token with nothing after it is
     * not a token somebody built an operator out of. */
    source
        .get(cursor)
        .is_none_or(|byte| !delimiter.forbidden_after.as_bytes().contains(byte))
}

/// Every token this profile opens a comment with.
fn profile_openers(profile: &DeclarativeProfile) -> Vec<&[u8]> {
    profile
        .line_comments
        .iter()
        .map(|delimiter| delimiter.start.as_bytes())
        .chain(
            profile
                .block_comments
                .iter()
                .map(|delimiter| delimiter.start.as_bytes()),
        )
        .collect()
}

/// Every token this profile closes one with. A line comment closes at the end
/// of its line and contributes none.
fn profile_closers(profile: &DeclarativeProfile) -> Vec<&[u8]> {
    profile
        .block_comments
        .iter()
        .map(|delimiter| delimiter.end.as_bytes())
        .collect()
}

/// Carry a documentation kind down the run it opens.
///
/// See [`DeclarativeProfile::doc_continuation`]. Applied before any policy or
/// rule reads a kind, so every later question — what the policy keeps, what
/// the shape rules skip, what the style rules reach — is asked about the kind
/// the language actually gives the line.
fn continue_documentation(
    source: &[u8],
    comments: &mut [Comment],
    profile: &DeclarativeProfile,
    options: &ScanOptions,
    patterns: &DispositionPatterns,
) {
    for (start, end) in crate::scanner::comment_runs(source, comments) {
        let mut carrying = false;
        for comment in &mut comments[start..end] {
            match comment.kind {
                CommentKind::DocLine => carrying = true,
                CommentKind::Line if carrying => {
                    /* NOTE: Rebuilt rather than relabelled. The verdict was
                     * read off the kind, so a kind written over the top of it
                     * would leave a comment whose disposition answers for the
                     * kind it used to be. There is one place that knows how to
                     * make a comment under a profile, and this is it. */
                    *comment = profile_comment(
                        source,
                        comment.span.start,
                        comment.span.end,
                        CommentKind::DocLine,
                        profile,
                        options,
                        patterns,
                    );
                }
                /* NOTE: Anything else ends the carry rather than passing
                 * through it. A licence notice or a directive between two
                 * documentation lines is not documentation, and the line under
                 * it is not a continuation of the one above it either. A plain
                 * line comment reaches here only when nothing was being
                 * carried, where ending the carry is what has already
                 * happened. */
                CommentKind::Line
                | CommentKind::Block
                | CommentKind::DocBlock
                | CommentKind::Directive
                | CommentKind::License
                | CommentKind::HtmlComment
                | CommentKind::Shebang
                | CommentKind::Encoding
                | CommentKind::OptimizerHint
                | CommentKind::VersionComment
                | CommentKind::LoadBearing => carrying = false,
            }
        }
    }
}

/// Whether a block comment that closes with `end` opens again at `index`.
///
/// Every declared opener that pairs with the same closer counts, because they
/// all have to be got past before that closer ends anything.
fn nested_opener(source: &[u8], index: usize, profile: &DeclarativeProfile, end: &str) -> bool {
    nested_opener_len(source, index, profile, end) > 0
}

/// How long that opener is, or zero when none opens here. The longest wins,
/// for the reason [`opener_at`] gives.
fn nested_opener_len(
    source: &[u8],
    index: usize,
    profile: &DeclarativeProfile,
    end: &str,
) -> usize {
    profile
        .block_comments
        .iter()
        .filter(|other| other.end == end && starts(source, index, other.start.as_bytes()))
        .map(|other| other.start.len())
        .max()
        .unwrap_or(0)
}

/// Which comment delimiter opens at `index`, and what kind of one it is.
enum Opener<'a> {
    Line(&'a LineDelimiter),
    Block(&'a BlockDelimiter),
}

/// The comment delimiter that opens at `index`, taking the longest token that
/// matches.
///
/// Longest rather than first-declared, which is the whole of what lets a
/// profile describe a language that spells its documentation comment as a
/// longer form of its ordinary one. First-declared would work too, for an
/// author who happened to list `////` above `//`; it would silently do
/// something else for one who did not, and an order that has to be right is a
/// way to be wrong.
///
/// A tie is impossible rather than broken: two matching tokens of the same
/// length would have to be the same token, and [`validate_profile`] refuses a
/// profile that declares one twice.
fn opener_at<'a>(
    source: &[u8],
    index: usize,
    profile: &'a DeclarativeProfile,
) -> Option<Opener<'a>> {
    let mut best: Option<(usize, Opener<'a>)> = None;
    let mut consider = |length: usize, opener: Opener<'a>| {
        if best.as_ref().is_none_or(|(best, _)| length > *best) {
            best = Some((length, opener));
        }
    };
    for delimiter in &profile.line_comments {
        if starts(source, index, delimiter.start.as_bytes())
            && (!delimiter.requires_boundary
                || index == 0
                || source[index - 1].is_ascii_whitespace())
            /* NOTE: The byte before is the line feed, which is also true of a
             * CRLF ending: the `\r` belongs to the line before it. */
            && (!delimiter.requires_line_start || index == 0 || source[index - 1] == b'\n')
            && opens_past_its_run(source, index, delimiter)
        {
            consider(delimiter.start.len(), Opener::Line(delimiter));
        }
    }
    for delimiter in &profile.block_comments {
        if starts(source, index, delimiter.start.as_bytes()) {
            consider(delimiter.start.len(), Opener::Block(delimiter));
        }
    }
    best.map(|(_, opener)| opener)
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
        match opener_at(source, index, profile) {
            Some(Opener::Line(delimiter)) => {
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
            }
            Some(Opener::Block(delimiter)) => {
                let start = index;
                index += delimiter.start.len();
                let mut depth = 1usize;
                while index < source.len() {
                    /* NOTE: Any opener that closes with this delimiter's `end`
                     * counts, not just the one that began the comment. Nesting is
                     * a property of the pairing: Haskell writes a documentation
                     * comment `{-| ... -}` and a remark `{- ... -}`, and a remark
                     * nested inside the documentation is still something the
                     * `-}` has to get past. Counting only the opener that began
                     * the comment let the inner `-}` close the outer comment and
                     * left the outer one dangling as code. */
                    if delimiter.nested && nested_opener(source, index, profile, &delimiter.end) {
                        depth += 1;
                        index += nested_opener_len(source, index, profile, &delimiter.end);
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
                        message: format!(
                            "unterminated block comment in profile `{}`",
                            profile.name
                        ),
                        severity: Severity::Error,
                        span: ByteSpan::new(start, index),
                    });
                }
            }
            None => index += 1,
        }
    }
    let valid = diagnostics.is_empty();
    /* NOTE: The same rules the built-in scanners apply, for the same reason. A
     * profile describes a file format rather than a policy, so a project's tag
     * convention and length limit have to reach a `.gitignore` exactly as they
     * reach a `.rs` -- and they did not, which showed up as this repository's
     * own tagged comments surviving in Rust and vanishing in a profile file. */
    if profile.doc_continuation {
        continue_documentation(source, &mut comments, profile, options, patterns);
    }
    crate::scanner::apply_allow_rules(source, &mut comments, options, patterns);
    /* NOTE: And the other axis, for the same reason: a project's spelling
     * convention reaches a `.gitignore` exactly as it reaches a `.rs`. The
     * profile's own delimiters are what it is asked about, because they are
     * what the file opens its comments with. */
    let openers = profile_openers(profile);
    let closers = profile_closers(profile);
    crate::scanner::apply_style_rules_with(
        source,
        &mut comments,
        options,
        crate::Markers {
            openers: &openers,
            closers: &closers,
        },
    );
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
    /* NOTE: The same classification the built-in scanners run, so that a
     * licence header or a cross-language tool directive is the kind it is
     * whichever reader found it. Without this a `# SPDX-License-Identifier:`
     * was a licence in a Python file and an ordinary comment in a `.gitignore`
     * -- the same bytes, kept by one reader and removed by the other.
     * `Language::Unknown` is the truth about a profile: it is not one of the
     * built-in languages, so the language-specific directives do not apply and
     * the profile declares its own below. */
    kind = crate::scanner::classify_comment(source, Language::Unknown, kind, start, end, 0);
    let raw = String::from_utf8_lossy(&source[start..end]);
    let protected = profile
        .protected_patterns
        .iter()
        .find(|pattern| raw.contains(&pattern.contains));
    if let Some(pattern) = protected {
        kind = pattern.tier.kind();
    }
    let mut disposition = disposition(kind, options, &source[start..end], patterns);
    if let (Some(pattern), crate::Disposition::Keep { reason }) = (protected, &mut disposition) {
        *reason = pattern.reason.clone();
    }
    Comment::new(ByteSpan::new(start, end), kind, disposition)
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
    use crate::Policy;
    /// Two delimiters spelled the same way: nothing could choose between them,
    /// and they would differ only in the kind they record.
    #[test]
    fn rejects_a_delimiter_declared_twice() {
        let profile = DeclarativeProfile {
            name: "x".into(),
            line_comments: vec![
                LineDelimiter {
                    start: "//".into(),
                    kind: CommentKind::Line,
                    ..Default::default()
                },
                LineDelimiter {
                    start: "//".into(),
                    kind: CommentKind::DocLine,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(matches!(
            validate_profile(&profile),
            Err(ProfileError::AmbiguousDelimiter(..))
        ));
    }

    /// One token being the start of another is how a language spells a
    /// documentation comment. It was refused as ambiguous, which made such a
    /// language inexpressible; the scan resolves it by taking the longest
    /// token that matches.
    #[test]
    fn a_prefix_is_resolved_by_length_rather_than_refused() {
        /* NOTE: Declared shortest first, which is the order that used to be
         * wrong. Nothing about the answer depends on it. */
        let profile = DeclarativeProfile {
            name: "gleam-like".into(),
            line_comments: vec![
                LineDelimiter {
                    start: "//".into(),
                    kind: CommentKind::Line,
                    ..Default::default()
                },
                LineDelimiter {
                    start: "///".into(),
                    kind: CommentKind::DocLine,
                    ..Default::default()
                },
                LineDelimiter {
                    start: "////".into(),
                    kind: CommentKind::DocLine,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(validate_profile(&profile).is_ok());
        let report = scan_profile(
            b"//// module\n/// item\n// remark\n",
            &profile,
            ScanOptions::default(),
        )
        .expect("the profile is valid");
        let kinds: Vec<_> = report.comments.iter().map(|comment| comment.kind).collect();
        assert_eq!(
            kinds,
            [
                CommentKind::DocLine,
                CommentKind::DocLine,
                CommentKind::Line
            ]
        );
    }

    /// The clause that tells a Haskell comment from a Haskell operator, which
    /// is the one thing a delimiter list could not say.
    #[test]
    fn a_forbidden_character_after_the_run_closes_the_opener() {
        let profile = DeclarativeProfile {
            name: "haskell-like".into(),
            line_comments: vec![LineDelimiter {
                start: "--".into(),
                forbidden_after: "<>|-".into(),
                kind: CommentKind::Line,
                ..Default::default()
            }],
            ..Default::default()
        };
        let report = scan_profile(
            b"a --> b\nc ----> d\n---x is a comment\n-- so is this\n",
            &profile,
            ScanOptions::default(),
        )
        .expect("the profile is valid");
        let text: Vec<_> = report
            .comments
            .iter()
            .map(|comment| {
                String::from_utf8_lossy(
                    &b"a --> b\nc ----> d\n---x is a comment\n-- so is this\n"
                        [comment.span.start..comment.span.end],
                )
                .into_owned()
            })
            .collect();
        assert_eq!(text, ["---x is a comment", "-- so is this"]);
    }

    /// A profile says how strongly each protection asks, and `all` honours it.
    ///
    /// Both halves are checked, because a tier that is only ever observed
    /// keeping has not been shown to be a tier: the tool-tier pattern must be
    /// taken by `all`, and the load-bearing one must survive it and then go
    /// when `force_protected` says so.
    #[test]
    fn a_profile_protection_states_which_tier_it_claims() {
        let profile = DeclarativeProfile {
            name: "demo".into(),
            line_comments: vec![LineDelimiter {
                start: ";;".into(),
                kind: CommentKind::Line,
                ..Default::default()
            }],
            protected_patterns: vec![
                ProtectedPattern {
                    contains: "KEEPTOOL".into(),
                    reason: "tool tier".into(),
                    tier: ProtectionTier::Tool,
                },
                ProtectedPattern {
                    contains: "KEEPBUILD".into(),
                    reason: "build tier".into(),
                    tier: ProtectionTier::LoadBearing,
                },
            ],
            ..Default::default()
        };
        let source = b";; KEEPTOOL one\n;; KEEPBUILD two\n;; ordinary\n";

        let conservative =
            scan_profile(source, &profile, ScanOptions::default()).expect("valid profile");
        assert_eq!(conservative.comments[0].kind, CommentKind::Directive);
        assert_eq!(conservative.comments[1].kind, CommentKind::LoadBearing);
        assert!(!conservative.comments[0].action().removes());
        assert!(!conservative.comments[1].action().removes());

        let all = ScanOptions {
            policy: Policy::All,
            ..Default::default()
        };
        let stripped = scan_profile(source, &profile, all.clone()).expect("valid profile");
        assert!(
            stripped.comments[0].action().removes(),
            "the tool tier is what `all` is entitled to take"
        );
        assert!(
            !stripped.comments[1].action().removes(),
            "no policy reaches the load-bearing tier"
        );

        let forced = ScanOptions {
            force_protected: true,
            ..all
        };
        let forced = scan_profile(source, &profile, forced).expect("valid profile");
        assert!(
            forced.comments[1].action().removes(),
            "force_protected is the one way out, and a tier with no way out is untestable"
        );
    }

    #[test]
    fn scans_profile_without_looking_inside_strings() {
        let profile = DeclarativeProfile {
            name: "demo".into(),
            line_comments: vec![LineDelimiter {
                start: ";;".into(),
                kind: CommentKind::Line,
                ..Default::default()
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
                kind: CommentKind::Line,
                ..Default::default()
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
