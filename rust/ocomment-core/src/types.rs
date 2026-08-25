use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// A half-open byte range `[start, end)`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl ByteSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
    pub const fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }
    pub const fn intersects(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Fold a spelling to lower case and drop every `-` and `_`.
fn fold_compact(value: &str) -> String {
    value.to_ascii_lowercase().replace(['_', '-'], "")
}

/// Fold a spelling to lower case and normalise `_` to the canonical `-`.
fn fold_kebab(value: &str) -> String {
    value.to_ascii_lowercase().replace('_', "-")
}

/// Fold a spelling to lower case.
fn fold_lower(value: &str) -> String {
    value.to_ascii_lowercase()
}

/// Find the variant whose canonical name or alias equals the folded spelling.
fn lookup<T: Copy>(
    all: &[T],
    folded: &str,
    name: fn(T) -> &'static str,
    aliases: fn(T) -> &'static [&'static str],
) -> Option<T> {
    all.iter()
        .copied()
        .find(|value| name(*value) == folded || aliases(*value).contains(&folded))
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    Rust,
    Ocaml,
    C,
    Cpp,
    Go,
    Java,
    #[serde(rename = "javascript")]
    JavaScript,
    #[serde(rename = "typescript")]
    TypeScript,
    Python,
    Shell,
    Html,
    Css,
    Jsonc,
    Sql,
    Kotlin,
    #[default]
    Unknown,
}

impl Language {
    /// Every CLI-visible language; `Unknown` is deliberately excluded.
    pub const ALL: [Self; 15] = [
        Self::Rust,
        Self::Ocaml,
        Self::C,
        Self::Cpp,
        Self::Go,
        Self::Java,
        Self::JavaScript,
        Self::TypeScript,
        Self::Python,
        Self::Shell,
        Self::Html,
        Self::Css,
        Self::Jsonc,
        Self::Sql,
        Self::Kotlin,
    ];

    /// The canonical name, identical to the serde representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Ocaml => "ocaml",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Go => "go",
            Self::Java => "java",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Shell => "shell",
            Self::Html => "html",
            Self::Css => "css",
            Self::Jsonc => "jsonc",
            Self::Sql => "sql",
            Self::Kotlin => "kotlin",
            Self::Unknown => "unknown",
        }
    }

    /// Accepted spellings besides [`Self::as_str`], already case- and
    /// separator-folded.
    pub const fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rs"],
            Self::Ocaml => &["ml"],
            Self::C | Self::Java | Self::Css | Self::Sql | Self::Unknown => &[],
            Self::Cpp => &["c++", "cxx"],
            Self::Go => &["golang"],
            Self::JavaScript => &["js", "jsx", "ecmascript"],
            Self::TypeScript => &["ts", "tsx"],
            Self::Python => &["py"],
            Self::Shell => &["sh", "bash", "zsh"],
            Self::Html => &["htm"],
            Self::Jsonc => &["json5"],
            Self::Kotlin => &["kt", "kts"],
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Language {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        lookup(
            &Self::ALL,
            &fold_compact(value),
            Self::as_str,
            Self::aliases,
        )
        .ok_or_else(|| format!("unsupported language `{value}`"))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dialect {
    #[default]
    Standard,
    Jsx,
    Tsx,
    #[serde(rename = "objective-c")]
    ObjectiveC,
    #[serde(rename = "objective-cpp")]
    ObjectiveCpp,
    #[serde(rename = "gnu-c")]
    GnuC,
    #[serde(rename = "gnu-cpp")]
    GnuCpp,
    Cuda,
    #[serde(rename = "posix-sh")]
    PosixSh,
    #[serde(rename = "bash53")]
    Bash53,
    Zsh,
    #[serde(rename = "postgresql")]
    PostgreSql,
    #[serde(rename = "mysql")]
    MySql,
    Sqlite,
    #[serde(rename = "t-sql")]
    TSql,
    Oracle,
}

impl Dialect {
    /// Every CLI-visible dialect.
    pub const ALL: [Self; 16] = [
        Self::Standard,
        Self::Jsx,
        Self::Tsx,
        Self::ObjectiveC,
        Self::ObjectiveCpp,
        Self::GnuC,
        Self::GnuCpp,
        Self::Cuda,
        Self::PosixSh,
        Self::Bash53,
        Self::Zsh,
        Self::PostgreSql,
        Self::MySql,
        Self::Sqlite,
        Self::TSql,
        Self::Oracle,
    ];

    /// The canonical name, identical to the serde representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Jsx => "jsx",
            Self::Tsx => "tsx",
            Self::ObjectiveC => "objective-c",
            Self::ObjectiveCpp => "objective-cpp",
            Self::GnuC => "gnu-c",
            Self::GnuCpp => "gnu-cpp",
            Self::Cuda => "cuda",
            Self::PosixSh => "posix-sh",
            Self::Bash53 => "bash53",
            Self::Zsh => "zsh",
            Self::PostgreSql => "postgresql",
            Self::MySql => "mysql",
            Self::Sqlite => "sqlite",
            Self::TSql => "t-sql",
            Self::Oracle => "oracle",
        }
    }

    /// Accepted spellings besides [`Self::as_str`], already case- and
    /// separator-folded.
    pub const fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Standard
            | Self::Jsx
            | Self::Tsx
            | Self::Cuda
            | Self::Zsh
            | Self::MySql
            | Self::Sqlite
            | Self::Oracle => &[],
            Self::ObjectiveC => &["objc"],
            Self::ObjectiveCpp => &["objective-c++", "objcpp"],
            Self::GnuC => &["gnuc"],
            Self::GnuCpp => &["gnu-c++", "gnucpp"],
            Self::PosixSh => &["posix", "sh"],
            Self::Bash53 => &["bash-5.3", "bash"],
            Self::PostgreSql => &["postgres", "pgsql"],
            Self::TSql => &["tsql"],
        }
    }
}

impl fmt::Display for Dialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Dialect {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        lookup(&Self::ALL, &fold_kebab(value), Self::as_str, Self::aliases)
            .ok_or_else(|| format!("unknown dialect `{value}`"))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommentKind {
    #[default]
    Line,
    Block,
    DocLine,
    DocBlock,
    Directive,
    License,
    HtmlComment,
    Shebang,
    Encoding,
    OptimizerHint,
    VersionComment,
}

impl CommentKind {
    /// Every CLI-visible comment kind.
    pub const ALL: [Self; 11] = [
        Self::Line,
        Self::Block,
        Self::DocLine,
        Self::DocBlock,
        Self::Directive,
        Self::License,
        Self::HtmlComment,
        Self::Shebang,
        Self::Encoding,
        Self::OptimizerHint,
        Self::VersionComment,
    ];

    /// The canonical name, identical to the serde representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Block => "block",
            Self::DocLine => "doc-line",
            Self::DocBlock => "doc-block",
            Self::Directive => "directive",
            Self::License => "license",
            Self::HtmlComment => "html-comment",
            Self::Shebang => "shebang",
            Self::Encoding => "encoding",
            Self::OptimizerHint => "optimizer-hint",
            Self::VersionComment => "version-comment",
        }
    }

    /// Accepted spellings besides [`Self::as_str`], already case- and
    /// separator-folded.
    pub const fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Line
            | Self::Block
            | Self::DocBlock
            | Self::Shebang
            | Self::Encoding
            | Self::OptimizerHint
            | Self::VersionComment => &[],
            Self::DocLine => &["doc"],
            Self::Directive => &["pragma"],
            Self::License => &["legal"],
            Self::HtmlComment => &["html"],
        }
    }
}

impl fmt::Display for CommentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CommentKind {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        lookup(&Self::ALL, &fold_kebab(value), Self::as_str, Self::aliases)
            .ok_or_else(|| format!("unknown comment kind `{value}`"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum Disposition {
    Remove,
    Keep { reason: String },
}

impl Disposition {
    pub const fn is_remove(&self) -> bool {
        matches!(self, Self::Remove)
    }
}

impl fmt::Display for Disposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Remove => f.write_str("remove"),
            Self::Keep { reason } => write!(f, "keep ({reason})"),
        }
    }
}

/// A [`DispositionExplanation`] with the reasoning taken away: the
/// keep-or-remove verdict on its own.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Action {
    Keep,
    Remove,
}

impl Action {
    /// The canonical name, matching the `action` tag [`Disposition`]
    /// serialises.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Remove => "remove",
        }
    }

    pub const fn is_remove(self) -> bool {
        matches!(self, Self::Remove)
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which rule decided one comment's fate.
///
/// A [`Disposition`] says what happens and gives a short reason a machine
/// cannot take apart; an explanation names the branch, so a caller can quote
/// the pattern, kind or directive that actually applied. The variants are
/// listed in the order the rules are tested, and the first rule that applies is
/// the variant returned — `keep` overrides always win, and the policy default
/// is the last word.
///
/// Regex indices are zero-based positions in [`ScanOptions::keep_regex`] and
/// [`ScanOptions::remove_regex`], and `pattern` is that entry verbatim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispositionExplanation {
    /// The kind is listed in [`ScanOptions::keep_kinds`].
    KeptByKind(CommentKind),
    /// A [`ScanOptions::keep_regex`] entry matched the whole comment token.
    KeptByRegex { index: usize, pattern: String },
    /// A shebang or encoding declaration the source needs to keep working;
    /// only [`ScanOptions::force_protected`] gives it up.
    ProtectedPreamble,
    /// An HTML comment, which the DOM exposes to scripts.
    KeptHtml,
    /// A directive addressed to a tool or to the compiler.
    KeptDirective {
        kind: CommentKind,
        name: Option<&'static str>,
    },
    /// A license or copyright notice under [`Policy::Legal`].
    KeptLicense { marker: Option<&'static str> },
    /// The kind is listed in [`ScanOptions::remove_kinds`].
    RemovedByKind(CommentKind),
    /// A [`ScanOptions::remove_regex`] entry matched the whole comment token.
    RemovedByRegex { index: usize, pattern: String },
    /// The policy removes every comment it is offered.
    RemovedByPolicy(Policy),
    /// Nothing protected an ordinary comment, so the policy default removed it.
    RemovedByDefault(Policy),
}

impl DispositionExplanation {
    /// The verdict alone. Equal to the [`Disposition`] the scanner records for
    /// the same comment under the same options.
    pub const fn action(&self) -> Action {
        match self {
            Self::KeptByKind(_)
            | Self::KeptByRegex { .. }
            | Self::ProtectedPreamble
            | Self::KeptHtml
            | Self::KeptDirective { .. }
            | Self::KeptLicense { .. } => Action::Keep,
            Self::RemovedByKind(_)
            | Self::RemovedByRegex { .. }
            | Self::RemovedByPolicy(_)
            | Self::RemovedByDefault(_) => Action::Remove,
        }
    }
}

impl fmt::Display for DispositionExplanation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeptByKind(kind) => {
                write!(f, "kept: comment kind `{kind}` is listed in keep_kinds")
            }
            Self::KeptByRegex { index, pattern } => {
                write!(f, "kept: matched keep_regex #{index} `{pattern}`")
            }
            Self::ProtectedPreamble => {
                f.write_str("kept: required source preamble, removable only with force_protected")
            }
            Self::KeptHtml => f.write_str("kept: HTML comments are DOM-observable"),
            Self::KeptDirective { kind, name } => match name {
                Some(name) => write!(f, "kept: tool or language directive `{name}`"),
                None => write!(f, "kept: `{kind}` is a tool or language directive"),
            },
            Self::KeptLicense { marker } => match marker {
                Some(marker) => write!(
                    f,
                    "kept: policy legal protects license comments, and this one says `{marker}`"
                ),
                None => f.write_str("kept: policy legal protects license comments"),
            },
            Self::RemovedByKind(kind) => {
                write!(
                    f,
                    "removed: comment kind `{kind}` is listed in remove_kinds"
                )
            }
            Self::RemovedByRegex { index, pattern } => {
                write!(f, "removed: matched remove_regex #{index} `{pattern}`")
            }
            Self::RemovedByPolicy(policy) => {
                write!(f, "removed: policy `{policy}` removes every comment")
            }
            Self::RemovedByDefault(policy) => {
                write!(f, "removed: policy `{policy}` removes ordinary comments")
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub span: ByteSpan,
    pub kind: CommentKind,
    pub disposition: Disposition,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Error,
    Warning,
    #[default]
    Info,
    Hint,
}

impl Severity {
    /// Every severity, ordered from most to least severe.
    pub const ALL: [Self; 4] = [Self::Error, Self::Warning, Self::Info, Self::Hint];

    /// The canonical name, identical to the serde representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }

    /// Accepted spellings besides [`Self::as_str`], already case-folded.
    pub const fn aliases(self) -> &'static [&'static str] {
        &[]
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Severity {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        lookup(&Self::ALL, &fold_lower(value), Self::as_str, Self::aliases)
            .ok_or_else(|| format!("unknown severity `{value}`"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub severity: Severity,
    pub span: ByteSpan,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScanReport {
    pub language: Language,
    pub comments: Vec<Comment>,
    pub diagnostics: Vec<Diagnostic>,
    pub valid: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Edit {
    pub span: ByteSpan,
    #[serde(with = "bytes_serde")]
    pub replacement: Vec<u8>,
}

/// Validation failure for comments supplied by an external scanner.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ExternalSpanError {
    #[error("external comment #{index} is outside the {source_len}-byte source")]
    OutOfBounds { index: usize, source_len: usize },
    #[error("external comment #{index} has an empty span")]
    Empty { index: usize },
    #[error("external comment #{index} is out of order or overlaps its predecessor")]
    OrderOrOverlap { index: usize },
    #[error("invalid external-scan policy regex: {0}")]
    InvalidPattern(String),
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Policy {
    #[default]
    Safe,
    Legal,
    All,
}

impl Policy {
    /// Every CLI-visible policy.
    pub const ALL: [Self; 3] = [Self::Safe, Self::Legal, Self::All];

    /// The canonical name, identical to the serde representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Legal => "legal",
            Self::All => "all",
        }
    }

    /// Accepted spellings besides [`Self::as_str`], already case-folded.
    pub const fn aliases(self) -> &'static [&'static str] {
        &[]
    }
}

impl fmt::Display for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Policy {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        lookup(&Self::ALL, &fold_lower(value), Self::as_str, Self::aliases)
            .ok_or_else(|| format!("unknown policy `{value}`"))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    #[default]
    Lines,
    Columns,
    Compact,
}

impl Layout {
    /// Every CLI-visible layout.
    pub const ALL: [Self; 3] = [Self::Lines, Self::Columns, Self::Compact];

    /// The canonical name, identical to the serde representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lines => "lines",
            Self::Columns => "columns",
            Self::Compact => "compact",
        }
    }

    /// Accepted spellings besides [`Self::as_str`], already case-folded.
    pub const fn aliases(self) -> &'static [&'static str] {
        &[]
    }
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Layout {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        lookup(&Self::ALL, &fold_lower(value), Self::as_str, Self::aliases)
            .ok_or_else(|| format!("unknown layout `{value}`"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScanOptions {
    pub policy: Policy,
    pub dialect: Dialect,
    pub force_invalid: bool,
    pub force_protected: bool,
    pub keep_kinds: Vec<CommentKind>,
    pub remove_kinds: Vec<CommentKind>,
    /// Byte-regexes that protect matching complete comment tokens.
    pub keep_regex: Vec<String>,
    /// Byte-regexes that remove matching complete comment tokens.
    pub remove_regex: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            policy: Policy::Safe,
            dialect: Dialect::Standard,
            force_invalid: false,
            force_protected: false,
            keep_kinds: Vec::new(),
            remove_kinds: Vec::new(),
            keep_regex: Vec::new(),
            remove_regex: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TransformOptions {
    pub scan: ScanOptions,
    pub layout: Layout,
}

impl Default for TransformOptions {
    fn default() -> Self {
        Self {
            scan: ScanOptions::default(),
            layout: Layout::Lines,
        }
    }
}

/// One unchanged or replaced source-map section.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceMapSegment {
    pub original: ByteSpan,
    pub output: ByteSpan,
    pub exact: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceMap {
    pub segments: Vec<SourceMapSegment>,
}

impl SourceMap {
    /// Build a byte source map for sorted, non-overlapping edits.
    pub fn from_edits(source_len: usize, edits: &[Edit]) -> Self {
        let mut original = 0;
        let mut output = 0;
        let mut segments = Vec::with_capacity(edits.len() * 2 + 1);
        for edit in edits {
            assert!(
                edit.span.start <= edit.span.end,
                "edit has an inverted span"
            );
            assert!(
                edit.span.start >= original,
                "edits overlap or are not sorted"
            );
            assert!(edit.span.end <= source_len, "edit is outside the source");
            if original < edit.span.start {
                let length = edit.span.start - original;
                segments.push(SourceMapSegment {
                    original: ByteSpan::new(original, edit.span.start),
                    output: ByteSpan::new(output, output + length),
                    exact: true,
                });
                output += length;
            }
            segments.push(SourceMapSegment {
                original: edit.span,
                output: ByteSpan::new(output, output + edit.replacement.len()),
                exact: false,
            });
            output += edit.replacement.len();
            original = edit.span.end;
        }
        if original < source_len || segments.is_empty() {
            segments.push(SourceMapSegment {
                original: ByteSpan::new(original, source_len),
                output: ByteSpan::new(output, output + source_len - original),
                exact: true,
            });
        }
        Self { segments }
    }

    pub fn original_to_output(&self, offset: usize) -> Option<usize> {
        for segment in &self.segments {
            if segment.original.contains(offset) {
                return if segment.exact {
                    Some(segment.output.start + offset - segment.original.start)
                } else {
                    Some(segment.output.start)
                };
            }
        }
        self.segments
            .last()
            .and_then(|segment| (offset == segment.original.end).then_some(segment.output.end))
    }

    pub fn output_to_original(&self, offset: usize) -> Option<usize> {
        for segment in &self.segments {
            if segment.output.contains(offset) {
                return if segment.exact {
                    Some(segment.original.start + offset - segment.output.start)
                } else {
                    Some(segment.original.start)
                };
            }
        }
        self.segments
            .last()
            .and_then(|segment| (offset == segment.output.end).then_some(segment.original.end))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransformResult {
    #[serde(with = "bytes_serde")]
    pub output: Vec<u8>,
    pub edits: Vec<Edit>,
    pub report: ScanReport,
    pub source_map: SourceMap,
}

pub(crate) mod bytes_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&String::from_utf8_lossy(bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        Ok(String::deserialize(deserializer)?.into_bytes())
    }
}
