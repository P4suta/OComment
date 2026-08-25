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
    pub const BUILT_INS: [Self; 15] = [
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
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Language {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().replace(['_', '-'], "").as_str() {
            "rust" | "rs" => Ok(Self::Rust),
            "ocaml" | "ml" => Ok(Self::Ocaml),
            "c" => Ok(Self::C),
            "cpp" | "c++" | "cxx" => Ok(Self::Cpp),
            "go" | "golang" => Ok(Self::Go),
            "java" => Ok(Self::Java),
            "javascript" | "js" | "jsx" | "ecmascript" => Ok(Self::JavaScript),
            "typescript" | "ts" | "tsx" => Ok(Self::TypeScript),
            "python" | "py" => Ok(Self::Python),
            "shell" | "sh" | "bash" | "zsh" => Ok(Self::Shell),
            "html" | "htm" => Ok(Self::Html),
            "css" => Ok(Self::Css),
            "jsonc" | "json5" => Ok(Self::Jsonc),
            "sql" => Ok(Self::Sql),
            "kotlin" | "kt" | "kts" => Ok(Self::Kotlin),
            _ => Err(format!("unsupported language `{value}`")),
        }
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

impl FromStr for Dialect {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().replace('_', "-").as_str() {
            "standard" => Ok(Self::Standard),
            "jsx" => Ok(Self::Jsx),
            "tsx" => Ok(Self::Tsx),
            "objective-c" | "objc" => Ok(Self::ObjectiveC),
            "objective-cpp" | "objective-c++" | "objcpp" => Ok(Self::ObjectiveCpp),
            "gnu-c" | "gnuc" => Ok(Self::GnuC),
            "gnu-cpp" | "gnu-c++" | "gnucpp" => Ok(Self::GnuCpp),
            "cuda" => Ok(Self::Cuda),
            "posix-sh" | "posix" | "sh" => Ok(Self::PosixSh),
            "bash53" | "bash-5.3" | "bash" => Ok(Self::Bash53),
            "zsh" => Ok(Self::Zsh),
            "postgresql" | "postgres" | "pgsql" => Ok(Self::PostgreSql),
            "mysql" => Ok(Self::MySql),
            "sqlite" => Ok(Self::Sqlite),
            "t-sql" | "tsql" => Ok(Self::TSql),
            "oracle" => Ok(Self::Oracle),
            _ => Err(format!("unknown dialect `{value}`")),
        }
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

impl FromStr for CommentKind {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().replace('_', "-").as_str() {
            "line" => Ok(Self::Line),
            "block" => Ok(Self::Block),
            "doc-line" | "doc" => Ok(Self::DocLine),
            "doc-block" => Ok(Self::DocBlock),
            "directive" | "pragma" => Ok(Self::Directive),
            "license" | "legal" => Ok(Self::License),
            "html" | "html-comment" => Ok(Self::HtmlComment),
            "shebang" => Ok(Self::Shebang),
            "encoding" => Ok(Self::Encoding),
            "optimizer-hint" => Ok(Self::OptimizerHint),
            "version-comment" => Ok(Self::VersionComment),
            _ => Err(format!("unknown comment kind `{value}`")),
        }
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

impl FromStr for Policy {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "safe" => Ok(Self::Safe),
            "legal" => Ok(Self::Legal),
            "all" => Ok(Self::All),
            _ => Err(format!("unknown policy `{value}`")),
        }
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

impl FromStr for Layout {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "lines" => Ok(Self::Lines),
            "columns" => Ok(Self::Columns),
            "compact" => Ok(Self::Compact),
            _ => Err(format!("unknown layout `{value}`")),
        }
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
