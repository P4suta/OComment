use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// A half-open byte range `[start, end)`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ByteSpan {
    /// The first byte of the range.
    pub start: usize,
    /// One byte past the last byte of the range.
    pub end: usize,
}

impl ByteSpan {
    /// The span running from `start` up to, but not including, `end`.
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    /// The number of bytes covered, or `0` when `end` precedes `start`.
    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }
    /// Whether the span covers no bytes at all.
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
    /// Whether `offset` falls inside the span. The `end` offset does not.
    pub const fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }
    /// Whether the two spans overlap: each starts before the other ends.
    /// For two non-empty spans that is exactly sharing at least one byte.
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

/// A language OComment has a built-in scanner for.
///
/// The serde representation is the canonical name [`Self::as_str`] returns.
/// [`FromStr`] accepts that name and every spelling in [`Self::aliases`],
/// case-folded and with `-` and `_` ignored, so `C++`, `cxx` and `cpp` all
/// name [`Self::Cpp`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    /// Rust, detected from `.rs`.
    Rust,
    /// OCaml, detected from `.ml` and `.mli`.
    Ocaml,
    /// C, detected from `.c` and `.h`; `.m` selects [`Dialect::ObjectiveC`].
    C,
    /// C++, detected from `.cpp` and its siblings; `.mm` and `.cu` select
    /// [`Dialect::ObjectiveCpp`] and [`Dialect::Cuda`].
    Cpp,
    /// Go, detected from `.go`.
    Go,
    /// Java, detected from `.java`.
    Java,
    /// JavaScript, detected from `.js`, `.mjs` and `.cjs`; `.jsx` selects
    /// [`Dialect::Jsx`].
    #[serde(rename = "javascript")]
    JavaScript,
    /// TypeScript, detected from `.ts`, `.mts` and `.cts`; `.tsx` selects
    /// [`Dialect::Tsx`].
    #[serde(rename = "typescript")]
    TypeScript,
    /// Python, detected from `.py`, `.pyw` and `.pyi`.
    Python,
    /// Shell, detected from `.sh`, `.bash` and `.zsh`, and from a `Dockerfile`
    /// or `Makefile` name.
    Shell,
    /// HTML, detected from `.html` and its siblings. `<script>` and `<style>`
    /// bodies are scanned as JavaScript and CSS.
    Html,
    /// CSS, detected from `.css`.
    Css,
    /// JSON with comments, detected from `.jsonc`, `.json5`, and from a
    /// `tsconfig.json` or `jsconfig.json` name.
    Jsonc,
    /// SQL, detected from `.sql`. The [`Dialect`] decides the string and
    /// comment rules.
    Sql,
    /// Kotlin, detected from `.kt` and `.kts`.
    Kotlin,
    /// TOML, detected from `.toml` and from the lock file names written in
    /// it, such as `Cargo.lock`.
    Toml,
    /// Lua, detected from `.lua` and `.rockspec`, and from a `lua` or
    /// `luajit` `#!` line.
    Lua,
    /// No built-in scanner, and the default.
    ///
    /// Scanning it yields no comments and one `unknown-language` error
    /// diagnostic. A syntax with no built-in scanner is handled by a
    /// [`DeclarativeProfile`](crate::DeclarativeProfile) or by
    /// [`transform_spans`](crate::transform_spans) instead.
    #[default]
    Unknown,
}

impl Language {
    /// Every CLI-visible language; `Unknown` is deliberately excluded.
    pub const ALL: [Self; 17] = [
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
        Self::Toml,
        Self::Lua,
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
            Self::Toml => "toml",
            Self::Lua => "lua",
            Self::Unknown => "unknown",
        }
    }

    /// Accepted spellings besides [`Self::as_str`], already case- and
    /// separator-folded.
    pub const fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rs"],
            Self::Ocaml => &["ml"],
            Self::C
            | Self::Java
            | Self::Css
            | Self::Sql
            | Self::Toml
            | Self::Lua
            | Self::Unknown => &[],
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

/// A vendor or extension variant of a [`Language`]'s lexical rules.
///
/// A dialect never changes the file type: [`Self::MySql`] is still
/// [`Language::Sql`]. It changes what counts as a string, an identifier, or a
/// comment while scanning. Naming one a language does not support is an error
/// rather than a silent fallback.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dialect {
    /// The language's own rules, with no vendor extension. The default.
    #[default]
    Standard,
    /// JavaScript with JSX syntax enabled.
    Jsx,
    /// TypeScript with TSX syntax enabled.
    Tsx,
    /// Objective-C. The comment rules are C's; the dialect records the
    /// flavour of the file.
    #[serde(rename = "objective-c")]
    ObjectiveC,
    /// Objective-C++. The comment rules are C++'s.
    #[serde(rename = "objective-cpp")]
    ObjectiveCpp,
    /// C with the GNU extensions. The comment rules are C's.
    #[serde(rename = "gnu-c")]
    GnuC,
    /// C++ with the GNU extensions. The comment rules are C++'s.
    #[serde(rename = "gnu-cpp")]
    GnuCpp,
    /// CUDA C++. The comment rules are C++'s.
    Cuda,
    /// POSIX `sh`, which has no `$'...'` ANSI-C quoted strings.
    #[serde(rename = "posix-sh")]
    PosixSh,
    /// Bash 5.3, which adds `$'...'` ANSI-C quoted strings.
    #[serde(rename = "bash53")]
    Bash53,
    /// Zsh, which also has `$'...'` ANSI-C quoted strings.
    Zsh,
    /// PostgreSQL: nested `/* ... */`, `$tag$ ... $tag$` dollar-quoted
    /// strings, and backslash escapes inside `E'...'`.
    #[serde(rename = "postgresql")]
    PostgreSql,
    /// MySQL: `#` line comments, `--` only when a boundary follows, strings
    /// in double quotes, and backslash escapes.
    #[serde(rename = "mysql")]
    MySql,
    /// SQLite, which uses the standard SQL rules.
    Sqlite,
    /// Transact-SQL: nested `/* ... */` and `[bracketed]` identifiers.
    #[serde(rename = "t-sql")]
    TSql,
    /// Oracle, which adds `q'[...]'` quoted literals.
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

/// What a comment is, which is what a [`Policy`] decides against.
///
/// The kind is lexical to begin with and then refined by the comment's own
/// bytes and position: a `//` token is [`Self::Line`] until it turns out to
/// carry an SPDX identifier ([`Self::License`]) or a build tag
/// ([`Self::Directive`]).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommentKind {
    /// An ordinary one-line comment: `// ...`, `# ...`, `-- ...`.
    #[default]
    Line,
    /// An ordinary delimited comment: `/* ... */`, `(* ... *)`.
    Block,
    /// A one-line documentation comment, such as Rust's `///` and `//!`.
    DocLine,
    /// A delimited documentation comment, such as `/** ... */`.
    DocBlock,
    /// A comment addressed to a tool or to the compiler: a build tag, a
    /// linter suppression, a type-checker pragma. `spec/directives.toml` is
    /// the catalogue.
    Directive,
    /// A license or copyright notice, such as an SPDX identifier. Only
    /// [`Policy::Legal`] keeps one.
    License,
    /// An HTML `<!-- ... -->` comment, which the DOM exposes to scripts.
    HtmlComment,
    /// A `#!` interpreter line at the very start of the file.
    Shebang,
    /// A Python source-encoding declaration in the first two lines.
    Encoding,
    /// A SQL optimizer hint, `/*+ ... */`, which the planner reads.
    OptimizerHint,
    /// A SQL version-gated comment, `/*! ... */`, whose body the server
    /// executes.
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

/// What the policy decided about one comment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum Disposition {
    /// The comment is removed.
    Remove,
    /// The comment stays, and `reason` says in a few words why.
    Keep {
        /// Which rule protected the comment, phrased for a human.
        reason: String,
    },
}

impl Disposition {
    /// Whether this is [`Self::Remove`].
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
    /// The comment stays.
    Keep,
    /// The comment goes.
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

    /// Whether this is [`Self::Remove`].
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
    KeptByRegex {
        /// Position of the entry in [`ScanOptions::keep_regex`].
        index: usize,
        /// That entry, verbatim.
        pattern: String,
    },
    /// A shebang or encoding declaration the source needs to keep working;
    /// only [`ScanOptions::force_protected`] gives it up.
    ProtectedPreamble,
    /// An HTML comment, which the DOM exposes to scripts.
    KeptHtml,
    /// A directive addressed to a tool or to the compiler.
    KeptDirective {
        /// The kind that was classified as a directive.
        kind: CommentKind,
        /// The directive's name, when the catalogue could name it.
        name: Option<&'static str>,
    },
    /// A license or copyright notice under [`Policy::Legal`].
    KeptLicense {
        /// The marker that identified it, such as `spdx-license-identifier`.
        marker: Option<&'static str>,
    },
    /// The kind is listed in [`ScanOptions::remove_kinds`].
    RemovedByKind(CommentKind),
    /// A [`ScanOptions::remove_regex`] entry matched the whole comment token.
    RemovedByRegex {
        /// Position of the entry in [`ScanOptions::remove_regex`].
        index: usize,
        /// That entry, verbatim.
        pattern: String,
    },
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

/// One comment the scanner found, and what the policy decided about it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    /// Where the comment's bytes are, delimiters included.
    pub span: ByteSpan,
    /// What the comment turned out to be.
    pub kind: CommentKind,
    /// Whether it is removed, and why if it is not.
    pub disposition: Disposition,
}

/// How serious a [`Diagnostic`] is.
///
/// Only [`Self::Error`] changes what a transformation writes: it makes
/// [`ScanReport::valid`] false, and nothing is edited unless
/// [`ScanOptions::force_invalid`] is set.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// The source could not be lexed: an unterminated comment or string.
    Error,
    /// Something the caller should look at, which still lexed.
    Warning,
    /// Ordinary information, and the default.
    #[default]
    Info,
    /// The mildest note.
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

/// Something the scanner has to say about the source it was given.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// A stable machine identifier, such as `unterminated-string`.
    pub code: String,
    /// The human sentence.
    pub message: String,
    /// How serious it is.
    pub severity: Severity,
    /// The bytes it is about.
    pub span: ByteSpan,
}

/// Everything a scan found.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScanReport {
    /// The language it was scanned as.
    pub language: Language,
    /// Every comment, in source order, non-overlapping.
    pub comments: Vec<Comment>,
    /// Everything the scanner had to say about the source.
    pub diagnostics: Vec<Diagnostic>,
    /// False when any diagnostic is a [`Severity::Error`].
    pub valid: bool,
}

/// One replacement of a byte range.
///
/// The edits of a [`TransformResult`] are sorted and non-overlapping, so
/// [`apply_edits`](crate::apply_edits) can walk them once.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Edit {
    /// The bytes to replace.
    pub span: ByteSpan,
    /// The bytes to put there, empty to delete. Serde renders these as a
    /// lossy UTF-8 string.
    #[serde(with = "bytes_serde")]
    pub replacement: Vec<u8>,
}

/// Validation failure for comments supplied by an external scanner.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ExternalSpanError {
    /// A span reaches past the end of the source.
    #[error("external comment #{index} is outside the {source_len}-byte source")]
    OutOfBounds {
        /// Position of the offending comment in the slice handed over.
        index: usize,
        /// The length of the source it had to fit in.
        source_len: usize,
    },
    /// A span covers no bytes.
    #[error("external comment #{index} has an empty span")]
    Empty {
        /// Position of the offending comment in the slice handed over.
        index: usize,
    },
    /// A span starts before its predecessor ends.
    #[error("external comment #{index} is out of order or overlaps its predecessor")]
    OrderOrOverlap {
        /// Position of the offending comment in the slice handed over.
        index: usize,
    },
    /// A `keep_regex` or `remove_regex` entry would not compile.
    #[error("invalid external-scan policy regex: {0}")]
    InvalidPattern(String),
}

/// Which comments survive by default.
///
/// A policy is the last word, not the first: [`ScanOptions::keep_kinds`],
/// [`ScanOptions::keep_regex`], [`ScanOptions::remove_kinds`] and
/// [`ScanOptions::remove_regex`] are all tested before it. The full table of
/// policy against [`CommentKind`] is in the crate documentation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Policy {
    /// The default. Removes ordinary, documentation, and license comments;
    /// keeps directives, HTML comments, SQL hints and version comments, and
    /// the shebang or encoding preamble.
    #[default]
    Safe,
    /// As [`Self::Safe`], but license and copyright notices are kept too.
    Legal,
    /// Removes every comment, directives and HTML comments included. The
    /// shebang and encoding preamble still survive unless
    /// [`ScanOptions::force_protected`] is set.
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

/// What a removal leaves behind in place of the comment.
///
/// No layout ever moves a byte the comment did not cover, so the choice is
/// only about the hole.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    /// The default. The line terminators inside the comment are kept, so
    /// every following line keeps its number, and a comment with code on
    /// both sides leaves a single space so the two tokens stay apart.
    #[default]
    Lines,
    /// As [`Self::Lines`], but the comment is replaced by spaces of the same
    /// display width, so every following column on the line keeps its number
    /// as well. Tabs are expanded to the next multiple of eight.
    Columns,
    /// As [`Self::Lines`], except that a line which held nothing but a
    /// removed comment goes away instead of staying behind as a blank one,
    /// and the whitespace a removal would leave at the end of a line is
    /// trimmed away with it.
    ///
    /// Code keeps its own lines. A comment that shared a line with code
    /// leaves that line, its terminator and its CRLF or LF style exactly as
    /// they were, so a comment running across several lines with code before
    /// or after it closes up to one line rather than joining two statements.
    /// A surviving line keeps the ending it had in the source — the same LF
    /// or CRLF, from inside the comment if that is where it was — or no
    /// ending at all if the file stopped there without one.
    ///
    /// Being alone on a line is judged from the original bytes, so a line
    /// holding two comments and nothing else keeps its terminator: neither
    /// comment was alone on it.
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

/// Everything that decides what a scan finds and what it does with it.
///
/// [`Self::default`] is the [`Policy::Safe`] policy with no overrides.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScanOptions {
    /// Which kinds survive by default.
    pub policy: Policy,
    /// The vendor rules to lex with. It must be one the language supports.
    pub dialect: Dialect,
    /// Edit even a source the scanner reported invalid. Without it a file
    /// with an unterminated comment or string comes back byte for byte.
    pub force_invalid: bool,
    /// Remove the shebang and encoding preamble as well. Nothing else
    /// protects them.
    pub force_protected: bool,
    /// Kinds kept whatever the policy says. Tested before everything else.
    pub keep_kinds: Vec<CommentKind>,
    /// Kinds removed unless a keep rule claimed them first.
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

/// A [`ScanOptions`] and what to leave behind in place of each removal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TransformOptions {
    /// What to find and what to decide about it.
    pub scan: ScanOptions,
    /// What a removal leaves behind.
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
    /// The bytes in the original source.
    pub original: ByteSpan,
    /// The bytes they became in the output.
    pub output: ByteSpan,
    /// True when the section is unchanged, so an offset maps through it
    /// byte for byte; false for a replaced section, where every original
    /// offset maps to the start of the replacement.
    pub exact: bool,
}

/// Where each byte of the original source ended up in the output.
///
/// This is what lets an editor keep a cursor, a diagnostic, or a breakpoint
/// pointing at the right place after a removal.
///
/// # Examples
///
/// ```
/// use ocomment_core::{Language, TransformOptions, transform};
///
/// let source = b"let x = 1; // note\nlet y = 2;\n";
/// let result = transform(source, Language::Rust, TransformOptions::default());
/// let map = &result.source_map;
///
/// // The `let y` on the second line survived, at a lower offset.
/// let original = source.windows(5).position(|w| w == b"let y").unwrap();
/// let moved = map.original_to_output(original).unwrap();
/// assert_eq!(&result.output[moved..moved + 5], b"let y");
/// assert_eq!(map.output_to_original(moved), Some(original));
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceMap {
    /// The sections, in order, covering the whole of both sides.
    pub segments: Vec<SourceMapSegment>,
}

impl SourceMap {
    /// Build a byte source map for sorted, non-overlapping edits.
    ///
    /// # Panics
    ///
    /// Panics if an edit has `start > end`, starts before its predecessor
    /// ends, or reaches past `source_len` — the same contract
    /// [`apply_edits`](crate::apply_edits) enforces, so a map is never built
    /// for edits that could not be applied.
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

    /// Where an original offset landed in the output.
    ///
    /// An offset inside a replaced section maps to the start of that
    /// replacement, and the end of the source maps to the end of the output.
    /// `None` when `offset` is past the end of the original.
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

    /// Where an output offset came from in the original.
    ///
    /// The mirror of [`Self::original_to_output`], with the same rule for
    /// replaced sections and the same `None` past the end.
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

/// The bytes a transformation would write, and the account of how.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransformResult {
    /// The transformed source. Serde renders it as a lossy UTF-8 string.
    #[serde(with = "bytes_serde")]
    pub output: Vec<u8>,
    /// The edits that turned the source into [`Self::output`], sorted and
    /// non-overlapping.
    pub edits: Vec<Edit>,
    /// The scan those edits were decided from.
    pub report: ScanReport,
    /// Where every byte of the source ended up.
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
