use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, str::FromStr};

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
    /// YAML, detected from `.yml` and `.yaml`, and from the extensionless
    /// configuration names written in it, such as `.clang-format`.
    Yaml,
    /// PHP, detected from `.php`, `.phtml` and `.phpt`, and from a `php` `#!`
    /// line. The inline HTML around the `<?php ... ?>` tags is content.
    Php,
    /// Ruby, detected from `.rb` and its siblings, from the extensionless
    /// project files written in it, such as `Gemfile`, and from a `ruby` `#!`
    /// line.
    Ruby,
    /// Zig, detected from `.zig` and from the `.zon` of Zig Object Notation.
    /// It has no block comment: `/*` is two operators.
    Zig,
    /// R, detected from `.r` in either case, from a `.Rprofile` name, and from
    /// an `Rscript` or `r` `#!` line.
    R,
    /// Dart, detected from `.dart` and from a `dart` `#!` line. Its block
    /// comments nest.
    Dart,
    /// Swift, detected from `.swift` and from a `swift` `#!` line. Its block
    /// comments nest and `#/ ... /#` is an opaque regular expression literal.
    Swift,
    /// C#, detected from `.cs` and from the `.csx` of a script. A line whose
    /// first non-blank byte is `#` is a preprocessor directive and carries at
    /// most a `//` comment.
    #[serde(rename = "csharp")]
    CSharp,
    /// Scala, detected from `.scala` and from the `.sc` of a script, and from
    /// a `scala` or `scala-cli` `#!` line. Its block comments nest, a string
    /// is interpolated when an identifier stands directly before its quote,
    /// and a `<` with the shape of an XML literal opens one.
    Scala,
    /// Vue, detected from the `.vue` of a single-file component. Its template
    /// is HTML with `{{ ... }}` code, and its `<script>` and `<style>` bodies
    /// are scanned as their own languages, the `lang` attribute choosing which.
    Vue,
    /// Svelte, detected from the `.svelte` of a component. Its template is
    /// HTML with `{ ... }` code, and its `<script>` and `<style>` bodies are
    /// scanned as their own languages.
    Svelte,
    /// Markdown, detected from `.md`, `.markdown` and the `.Rmd` of an R
    /// Markdown document. HTML comments are comments, fenced code blocks are
    /// scanned as the language their info string names, and inline and
    /// indented code are opaque.
    Markdown,
    /// Perl, detected from `.pl`, `.pm` and `.t`, and from a `perl` `#!`
    /// line. Its quote words, here-documents and regular expressions hide a
    /// `#`, its POD blocks are opaque, and a `/` the parse context alone
    /// settles is reported as lexically ambiguous.
    Perl,
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
    pub const ALL: [Self; 30] = [
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
        Self::Yaml,
        Self::Php,
        Self::Ruby,
        Self::Zig,
        Self::R,
        Self::Dart,
        Self::Swift,
        Self::CSharp,
        Self::Scala,
        Self::Vue,
        Self::Svelte,
        Self::Markdown,
        Self::Perl,
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
            Self::Yaml => "yaml",
            Self::Php => "php",
            Self::Ruby => "ruby",
            Self::Zig => "zig",
            Self::R => "r",
            Self::Dart => "dart",
            Self::Swift => "swift",
            Self::CSharp => "csharp",
            Self::Scala => "scala",
            Self::Vue => "vue",
            Self::Svelte => "svelte",
            Self::Markdown => "markdown",
            Self::Perl => "perl",
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
            | Self::Php
            | Self::Zig
            | Self::Dart
            | Self::Swift
            | Self::Scala
            | Self::Vue
            | Self::Svelte
            | Self::Markdown
            | Self::Perl
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
            Self::Yaml => &["yml"],
            Self::Ruby => &["rb"],
            /* NOTE: `Rscript` is the front end that runs an R script and the name
             * a `#!` line carries, so someone naming the language after the
             * command they type reaches the same scanner. GitHub Linguist
             * publishes it as an alias of R for the same reason. */
            Self::R => &["rscript"],
            /* NOTE: `c#` is the language's own name, and `cs` the suffix its
             * files carry and the identifier every editor knows it by. The
             * third spelling a project writes, `c-sharp`, needs no row of its
             * own: [`FromStr`] folds `-` and `_` out of a spelling before it
             * looks it up, so those letters already reach the canonical name. */
            Self::CSharp => &["cs", "c#"],
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
    /// SCSS: CSS with `//` line comments and `#{ ... }` interpolation.
    Scss,
    /// The indentation-based Sass syntax. Silent comments also own their
    /// more-deeply-indented body lines.
    Sass,
}

impl Dialect {
    /// Every CLI-visible dialect.
    pub const ALL: [Self; 18] = [
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
        Self::Scss,
        Self::Sass,
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
            Self::Scss => "scss",
            Self::Sass => "sass",
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
            | Self::Oracle
            | Self::Scss
            | Self::Sass => &[],
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
    /// [`Policy::Conservative`] keeps one.
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
    /// A directive the language or its build reads as part of the program:
    /// `//go:build`, `# frozen_string_literal:`, `// swift-tools-version:`.
    /// Removing one changes what compiles or what the code does, rather than
    /// what a tool reports about it, so a `remove` policy does not reach it
    /// and only [`ScanOptions::force_protected`] gives it up.
    LoadBearing,
}

/// How strongly a [`CommentKind`] is held back from every policy.
///
/// This is a property of the kind rather than a decision any run makes: a
/// shebang is required by the file's own syntax whatever anyone configures,
/// and a `//go:build` is read by the compiler whatever anyone configures. The
/// only way past either is [`ScanOptions::force_protected`], which is a
/// sentence someone types.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Protection {
    /// No protection. The policy has the last word.
    None,
    /// A line the source needs in order to be read at all.
    Preamble,
    /// Read by the language, its build, or the server that executes it.
    LoadBearing,
}

impl Protection {
    /// The reason a report gives for a comment held back at this tier.
    ///
    /// Two of the strings the differential protocol freezes, which is why they
    /// live beside the tier rather than beside the code that prints them.
    pub const fn reason(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Preamble => Some("required source preamble"),
            Self::LoadBearing => Some("required by the language or its build"),
        }
    }
}

impl CommentKind {
    /// Every CLI-visible comment kind.
    pub const ALL: [Self; 12] = [
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
        Self::LoadBearing,
    ];

    /// Which protection this kind carries, before any policy is consulted.
    ///
    /// Exhaustive on purpose: a new kind does not compile until somebody has
    /// decided whether removing one changes what the toolchain produces. That
    /// question is the whole of the distinction, and leaving it to be answered
    /// later has meant, twice, that it was answered by accident.
    pub const fn protection(self) -> Protection {
        match self {
            Self::Shebang | Self::Encoding => Protection::Preamble,
            // NOTE: The SQL pair is here because the server reads them as part
            // NOTE: of the statement: one is executed and one decides the plan.
            Self::LoadBearing | Self::OptimizerHint | Self::VersionComment => {
                Protection::LoadBearing
            }
            Self::Line
            | Self::Block
            | Self::DocLine
            | Self::DocBlock
            | Self::Directive
            | Self::License
            | Self::HtmlComment => Protection::None,
        }
    }

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
            Self::LoadBearing => "load-bearing",
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
            | Self::VersionComment
            | Self::LoadBearing => &[],
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

/// A rule about a comment's *shape* rather than its kind, and the verdict it
/// reached.
///
/// These are decided over the whole file — how many lines a run of adjacent
/// comments covers, whether code sits before one on its line — so unlike every
/// other rule they cannot be re-derived from a comment's own bytes. Recording
/// the rule here is what lets an explanation state the one that actually
/// applied instead of falling back to the policy and contradicting the
/// verdict on the line above it.
///
/// [`ScanOptions::allow`] is the only thing that produces one.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "kebab-case")]
pub enum ShapeRule {
    /// [`AllowRules::tags`]: kept for the tag its text opens with.
    Tagged {
        /// The configured tag it matched, in the configured spelling.
        tag: String,
    },
    /// [`AllowRules::expiry`]: the tag allowed it, and its time is up.
    ///
    /// Produced by a caller that can read a repository, never by the scan —
    /// see [`AllowRules::expiry`] for why the two are separate.
    Expired {
        /// The configured tag it matched.
        tag: String,
        /// How old the line carrying it is, in days.
        age: Age,
        /// How old the configuration lets it get.
        limit: Age,
    },
    /// [`AllowRules::trailing`] is `false`: removed for sitting after code.
    Trailing,
    /// [`AllowRules::max_lines`]: removed with the run of comments it belongs
    /// to, because that run is longer than the limit.
    TooLong {
        /// How many lines the run covers.
        lines: usize,
        /// How many [`AllowRules::max_lines`] permits.
        limit: usize,
    },
}

impl ShapeRule {
    /// The verdict this rule reaches, which is fixed per rule.
    ///
    /// A [`Comment`] carrying a rule always carries the matching
    /// [`Disposition`]: both are written from this one value, so the two
    /// cannot drift apart.
    pub const fn action(&self) -> Action {
        match self {
            Self::Tagged { .. } => Action::Keep,
            Self::Trailing | Self::TooLong { .. } | Self::Expired { .. } => Action::Remove,
        }
    }

    /// The disposition a comment this rule decided carries.
    pub fn disposition(&self) -> Disposition {
        match self {
            Self::Tagged { tag } => Disposition::Keep {
                reason: format!("tagged `{tag}`"),
            },
            Self::Trailing | Self::TooLong { .. } | Self::Expired { .. } => Disposition::Remove,
        }
    }

    /// The explanation this rule writes for the comment it decided.
    pub fn explanation(&self) -> DispositionExplanation {
        match self {
            Self::Tagged { tag } => DispositionExplanation::KeptByTag { tag: tag.clone() },
            Self::Trailing => DispositionExplanation::RemovedAsTrailing,
            Self::TooLong { lines, limit } => DispositionExplanation::RemovedByLength {
                lines: *lines,
                limit: *limit,
            },
            Self::Expired { tag, age, limit } => DispositionExplanation::RemovedAsExpired {
                tag: tag.clone(),
                age: *age,
                limit: *limit,
            },
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
///
/// One rule is not about the comment's bytes at all and so is not a branch of
/// that table: [`Self::KeptStructural`] is decided by where the comment sits in
/// the file, is tested after every other rule, and is the answer only
/// [`explain_comment`](crate::explain_comment) can give.
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
    /// A directive the language or its build reads as part of the program.
    /// Removing it would change what compiles or what the code does, so no
    /// `remove` policy reaches it and only [`ScanOptions::force_protected`]
    /// gives it up.
    KeptLoadBearing {
        /// The directive's name, when the catalogue could name it.
        name: Option<&'static str>,
    },
    /// A directive addressed to a tool or to the compiler.
    KeptDirective {
        /// The kind that was classified as a directive.
        kind: CommentKind,
        /// The directive's name, when the catalogue could name it.
        name: Option<&'static str>,
    },
    /// A documentation comment under [`Policy::Conservative`].
    ///
    /// It is the API documentation rather than a remark about the code, so
    /// removing it takes something published: a page on docs.rs, an entry on
    /// pkg.go.dev, a javadoc section.
    KeptDocumentation {
        /// Which of the two documentation kinds it is.
        kind: CommentKind,
    },
    /// A license or copyright notice under [`Policy::Conservative`].
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
    RemovedByPolicy {
        /// The policy that removed it.
        policy: Policy,
        /// The kind it was removed as.
        kind: CommentKind,
    },
    /// Nothing protected the comment, so the policy default removed it.
    ///
    /// A policy removes several kinds and removes them for different reasons,
    /// so the kind is part of the answer: it is what tells a reader which
    /// setting they would have to change to keep this one. Without it a
    /// license notice and an ordinary line comment give the same explanation.
    RemovedByDefault {
        /// The policy whose default applied.
        policy: Policy,
        /// The kind it was removed as.
        kind: CommentKind,
    },
    /// [`AllowRules::tags`] matched the tag the comment's text opens with.
    KeptByTag {
        /// The configured tag it matched.
        tag: String,
    },
    /// [`AllowRules::trailing`] is `false` and code sits before this comment
    /// on its line.
    RemovedAsTrailing,
    /// The tag allowed the comment, and [`AllowRules::expiry`] gave it a
    /// deadline the line has now passed.
    RemovedAsExpired {
        /// The configured tag it matched.
        tag: String,
        /// How old the line carrying it is.
        age: Age,
        /// How old the configuration lets it get.
        limit: Age,
    },
    /// The run of adjacent comments this one belongs to is longer than
    /// [`AllowRules::max_lines`].
    RemovedByLength {
        /// How many lines the run covers.
        lines: usize,
        /// How many the configuration permits.
        limit: usize,
    },
    /// A comment every rule above would have removed, kept because a block
    /// scalar's body ends at it and a comment the run keeps sits below it,
    /// deep enough that the body would take that comment back.
    ///
    /// No option overrules this one: `--policy all` removes the comment below
    /// it and the question with it, but a comment an override still keeps
    /// leaves the value depending on this line.
    KeptStructural {
        /// The language whose layout rule decided it, which is
        /// [`Language::Yaml`] wherever this is returned today.
        language: Language,
    },
}

impl DispositionExplanation {
    /// The verdict alone. Equal to the [`Disposition`] the scanner records for
    /// the same comment under the same options.
    pub const fn action(&self) -> Action {
        match self {
            Self::KeptByKind(_)
            | Self::KeptByRegex { .. }
            | Self::ProtectedPreamble
            | Self::KeptLoadBearing { .. }
            | Self::KeptHtml
            | Self::KeptDirective { .. }
            | Self::KeptDocumentation { .. }
            | Self::KeptLicense { .. }
            | Self::KeptByTag { .. }
            | Self::KeptStructural { .. } => Action::Keep,
            Self::RemovedByKind(_)
            | Self::RemovedByRegex { .. }
            | Self::RemovedByPolicy { .. }
            | Self::RemovedAsTrailing
            | Self::RemovedAsExpired { .. }
            | Self::RemovedByLength { .. }
            | Self::RemovedByDefault { .. } => Action::Remove,
        }
    }
}

/// What a policy removed, named as the kind rather than as "comments".
///
/// A policy default removes more than one kind, and a reader who is told only
/// that "the policy removes ordinary comments" cannot tell whether the comment
/// in front of them was ordinary. Naming the kind is what makes the sentence
/// checkable against the kind the same line already reports.
///
/// The kinds a policy default cannot reach — a shebang, a load-bearing
/// directive — are spelled generically rather than omitted, so that adding a
/// kind cannot silently produce a sentence with a hole in it.
const fn removed_noun(kind: CommentKind) -> &'static str {
    match kind {
        CommentKind::Line | CommentKind::Block => "ordinary comments",
        CommentKind::DocLine | CommentKind::DocBlock => "doc comments",
        CommentKind::License => "license comments",
        CommentKind::HtmlComment => "HTML comments",
        CommentKind::Directive => "tool and language directives",
        CommentKind::OptimizerHint => "optimizer hints",
        CommentKind::VersionComment => "version-gated comments",
        CommentKind::Shebang => "shebang lines",
        CommentKind::Encoding => "encoding declarations",
        CommentKind::LoadBearing => "comments the language or its build reads",
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
            Self::KeptLoadBearing { name } => match name {
                Some(name) => write!(
                    f,
                    "kept: `{name}` is read by the language or its build, so removing it would change the code rather than a report about it"
                ),
                None => f.write_str(
                    "kept: the language or its build reads this comment, so removing it would change the code rather than a report about it",
                ),
            },
            Self::KeptHtml => f.write_str("kept: HTML comments are DOM-observable"),
            Self::KeptDirective { kind, name } => match name {
                Some(name) => write!(f, "kept: tool or language directive `{name}`"),
                None => write!(f, "kept: `{kind}` is a tool or language directive"),
            },
            /* NOTE: The policy is spelled through `Policy` rather than written
             * out, so that renaming one cannot leave this sentence naming a
             * policy the binary no longer accepts. */
            Self::KeptDocumentation { kind } => write!(
                f,
                "kept: policy {} protects documentation comments, and this is a `{kind}`",
                Policy::Conservative
            ),
            Self::KeptLicense { marker } => {
                let policy = Policy::Conservative;
                match marker {
                    Some(marker) => write!(
                        f,
                        "kept: policy {policy} protects license comments, and this one says `{marker}`"
                    ),
                    None => write!(f, "kept: policy {policy} protects license comments"),
                }
            }
            Self::RemovedByKind(kind) => {
                write!(
                    f,
                    "removed: comment kind `{kind}` is listed in remove_kinds"
                )
            }
            Self::RemovedByRegex { index, pattern } => {
                write!(f, "removed: matched remove_regex #{index} `{pattern}`")
            }
            Self::RemovedByPolicy { policy, kind } => {
                write!(
                    f,
                    "removed: policy `{policy}` removes every comment, this one a `{kind}`"
                )
            }
            Self::RemovedByDefault { policy, kind } => {
                write!(f, "removed: policy `{policy}` removes {}", removed_noun(*kind))
            }
            Self::KeptByTag { tag } => {
                write!(f, "kept: its text opens with the allowed tag `{tag}`")
            }
            Self::RemovedAsTrailing => {
                f.write_str("removed: code comes before it on its line, and trailing comments are not allowed")
            }
            Self::RemovedAsExpired { tag, age, limit } => write!(
                f,
                "removed: `{tag}` is a promise with {limit} to keep it, and this line is {age} old"
            ),
            Self::RemovedByLength { lines, limit } => write!(
                f,
                "removed: it belongs to a run of {lines} adjacent comment lines, and at most {limit} is allowed"
            ),
            Self::KeptStructural { language } => write!(
                f,
                "kept: it separates a `{language}` block scalar from the kept comment below it"
            ),
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
    /// The shape rule that settled it, when one did.
    ///
    /// `None` is the ordinary case: the policy, the kind lists and the pattern
    /// lists decided, and all three can be read back off the comment's own
    /// bytes. A [`ShapeRule`] cannot, so it is carried rather than guessed at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<ShapeRule>,
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

/// How far the damage from a diagnostic reaches.
///
/// A scanner that reports an error has already decided how to carry on, and the
/// two ways it can do that are not the same for anyone acting on what it found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Damage {
    /// Confined to the bytes the diagnostic names. A C# string that never
    /// closes ends at the newline, and line two is lexed by a scanner that
    /// knows exactly where it is.
    Span,
    /// Everything from where the diagnostic starts. The scan stopped there, or
    /// carried on from a position it guessed, and what it reports past that
    /// point is a reading it cannot defend.
    Rest,
}

/// Every error a scan can report, and how far each one reaches.
///
/// Two lists in one, so that the check worth making is that no code is in
/// neither: `error_codes_are_all_classified` reads the scanners' own source and
/// fails on a code this does not name, which is the only way a rule about
/// errors survives the next error being added. A code that reaches
/// [`Diagnostic::damage`] without appearing here is treated as [`Damage::Rest`]
/// -- the direction that declines to edit rather than the one that edits on a
/// guess.
///
/// Most errors are [`Damage::Span`], because most of them are a token that did
/// not end: the scanner consumed as far as it was willing to, said so, and
/// resumed after it. An unterminated block comment names bytes running to the
/// end of the file and so covers all of them; an unterminated single-line
/// string names bytes running to the newline and covers only those. Both fall
/// out of the same rule, which is why neither needs an entry of its own.
///
/// The three exceptions are the scans that cannot say where they stopped being
/// right. `lexical-ambiguity` is a `/` the scanner could not tell from a
/// division and read as a regex; if that was the wrong reading, every token
/// after it is wrong too. `nesting-limit` abandons the rest of the source and
/// names no bytes at all. `unknown-language` scans nothing.
pub const ERROR_CODES: [(&str, Damage); 19] = [
    ("invalid-unicode-escape", Damage::Span),
    ("lexical-ambiguity", Damage::Rest),
    ("nesting-limit", Damage::Rest),
    ("unknown-language", Damage::Rest),
    ("unterminated-comment", Damage::Span),
    ("unterminated-embedded-language", Damage::Span),
    ("unterminated-fstring-expression", Damage::Span),
    ("unterminated-heredoc", Damage::Span),
    ("unterminated-html-tag", Damage::Span),
    ("unterminated-identifier", Damage::Span),
    ("unterminated-interpolation", Damage::Span),
    ("unterminated-jsx-element", Damage::Span),
    ("unterminated-jsx-tag", Damage::Span),
    ("unterminated-operator", Damage::Span),
    ("unterminated-profile-comment", Damage::Span),
    ("unterminated-profile-string", Damage::Span),
    ("unterminated-regex", Damage::Span),
    ("unterminated-string", Damage::Span),
    ("unterminated-template-expression", Damage::Span),
];

impl Diagnostic {
    /// How far this reaches, for a caller deciding what it may still act on.
    ///
    /// Anything milder than an error damages nothing by construction: a warning
    /// is something the caller should look at in a source that lexed.
    #[must_use]
    pub fn damage(&self) -> Option<Damage> {
        if self.severity != Severity::Error {
            return None;
        }
        Some(
            ERROR_CODES
                .iter()
                .find(|(code, _)| *code == self.code)
                .map_or(Damage::Rest, |&(_, damage)| damage),
        )
    }
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

impl ScanReport {
    /// Whether this scan established what it reported about `span`.
    ///
    /// [`valid`](Self::valid) says whether the lex failed. It cannot say where,
    /// and anyone acting on a verdict needs that: a comment the scanner
    /// delimited away from the failure is worth exactly what a comment in a
    /// clean file is worth, while one inside it rests on a guess about where
    /// the token ends. An unterminated block opener is reported as a comment
    /// running to the end of the file, and the code under it is not a comment.
    ///
    /// Removing the first is removing a comment. Removing the second is
    /// removing bytes nobody established were one.
    #[must_use]
    pub fn established(&self, span: ByteSpan) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| match diagnostic.damage() {
                None => true,
                Some(Damage::Span) => {
                    span.end <= diagnostic.span.start || span.start >= diagnostic.span.end
                }
                Some(Damage::Rest) => span.end <= diagnostic.span.start,
            })
    }

    /// Whether every comment in this report is one the scan established.
    ///
    /// The cheap answer for a caller that only wants to know whether the
    /// distinction applies at all, so that a clean report costs nothing to ask
    /// about and carries nothing extra when it is written out.
    #[must_use]
    pub fn established_everything(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.damage().is_none())
    }
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
    /// The default. Removes ordinary and documentation comments; keeps
    /// license notices, directives, HTML comments, SQL hints and version
    /// comments, and the shebang or encoding preamble. A
    /// [`CommentKind::LoadBearing`] directive is kept under every policy.
    ///
    /// Was spelled `legal`, which named the one kind it adds rather than
    /// where it sits, and which left the weaker-sounding `safe` as the
    /// default that removed licence notices.
    #[default]
    #[serde(alias = "legal")]
    Conservative,
    /// As [`Self::Conservative`], and license and copyright notices go too.
    ///
    /// Was spelled `safe` and was the default. It is neither the safest
    /// policy nor a safe default for a repository that states its licence in
    /// its sources: REUSE compliance does not survive it.
    #[serde(alias = "safe")]
    Standard,
    /// Removes every comment, directives and HTML comments included. The
    /// shebang and encoding preamble survive, and so does a
    /// [`CommentKind::LoadBearing`] directive the language or its build reads
    /// as part of the program; all three go only when
    /// [`ScanOptions::force_protected`] is set.
    All,
}

impl Policy {
    /// Every CLI-visible policy, weakest first.
    ///
    /// The order is the order of how much a policy takes, so that a list of
    /// them reads as a scale. It is also the order help output uses, which is
    /// where a reader forms the expectation that the names have an order at
    /// all.
    pub const ALL: [Self; 3] = [Self::Conservative, Self::Standard, Self::All];

    /// The canonical name, identical to the serde representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conservative => "conservative",
            Self::Standard => "standard",
            Self::All => "all",
        }
    }

    /// Accepted spellings besides [`Self::as_str`], already case-folded.
    ///
    /// The former names are kept so that an existing configuration and an
    /// existing command line both still resolve. They resolve to the same
    /// behaviour they always named; what changed is which one is the default.
    pub const fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Conservative => &["legal"],
            Self::Standard => &["safe"],
            Self::All => &[],
        }
    }

    /// Whether this policy keeps a comment of `kind`, absent every other rule.
    ///
    /// This is the table in the crate documentation, as code. It was prose in
    /// one place and a chain of `if`s in another, and a reader asking "would a
    /// weaker policy have kept this?" had nothing to ask — so the CLI answered
    /// with a hand-written guess about kinds, which was wrong in the only case
    /// that occurs: a Rust crate with both documentation and a licence header.
    ///
    /// This is the policy's own answer and not the last word. A shebang, an
    /// encoding line, a load-bearing directive and the two SQL forms a server
    /// reads are held back from every policy by [`CommentKind::protection`],
    /// which is tested before this — and given up by
    /// [`ScanOptions::force_protected`], which is what lets `all` reach them.
    /// Answering `true` here for those kinds would close that door, and the
    /// first version of this did.
    ///
    /// The match is exhaustive, which is the point: a new [`CommentKind`] does
    /// not compile until every policy has an answer for it.
    pub const fn keeps(self, kind: CommentKind) -> bool {
        match kind {
            CommentKind::Line | CommentKind::Block => false,
            CommentKind::DocLine | CommentKind::DocBlock | CommentKind::License => {
                matches!(self, Self::Conservative)
            }
            CommentKind::Directive | CommentKind::HtmlComment => !matches!(self, Self::All),
            /* NOTE: False, and not "true because every policy keeps them". The
             * protection keeps them and is tested first; this is what the
             * policy would do if the protection were lifted, which is exactly
             * what `--force-protected` asks for. Answering `true` closed that
             * door, and the test comparing this table against the explanation
             * branch table is what said so. */
            CommentKind::Shebang
            | CommentKind::Encoding
            | CommentKind::LoadBearing
            | CommentKind::OptimizerHint
            | CommentKind::VersionComment => false,
        }
    }

    /// The policy that keeps every one of `kinds` while taking the most, if
    /// any does.
    ///
    /// The strongest rather than the weakest, because the caller is someone
    /// removing comments: of the policies that would make their run clean, the
    /// one worth naming is the one that still takes everything else. Suggesting
    /// the gentlest would answer "how do I stop seeing findings" instead of
    /// "how do I keep the ones I meant to keep".
    ///
    /// `ALL` is ordered by how much each policy takes, weakest first, so this
    /// walks it backwards.
    pub fn strongest_keeping(kinds: &[CommentKind]) -> Option<Self> {
        Self::ALL
            .into_iter()
            .rev()
            .find(|policy| kinds.iter().all(|kind| policy.keeps(*kind)))
    }

    /// The name this policy used to go by, for a deprecation notice.
    pub const fn former_name(self) -> Option<&'static str> {
        match self {
            Self::Conservative => Some("legal"),
            Self::Standard => Some("safe"),
            Self::All => None,
        }
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
/// The choice is only about the hole: no layout moves a byte the comment did
/// not cover, except where the hole itself would say something. That happens
/// in one place, and it is YAML.
///
/// A block scalar decides where its body ends from the lines *below* it
/// (YAML 1.2.2, 8.1.1), so a whole-line comment under a body is what
/// terminates it — and the hole a removal would leave on that line is read
/// back as content. A line of spaces as wide as the comment, which `columns`
/// writes, is indented at least as deep as the body whenever the comment was
/// wide enough; an empty line, which `lines` writes, is content under `|+` and
/// `>+`, which keep every empty line trailing a body (8.1.1.2). So in YAML a
/// whole-line comment sitting in the run of blank and comment lines under a
/// block scalar body is removed by taking its whole line, terminator and all,
/// under **every** layout: `lines` gives up that line's number and `columns`
/// its columns, rather than give up the value. Under `|+` and `>+` the removal
/// also takes the blank lines the comment was sheltering — they are content
/// the moment it is gone — and never the blank lines above the first comment,
/// which were content already.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    /// The default. The line terminators inside the comment are kept, so
    /// every following line keeps its number, and a comment with code on
    /// both sides leaves a single space so the two tokens stay apart. The YAML
    /// exception above is the one place a line does not keep its number.
    #[default]
    Lines,
    /// As [`Self::Lines`], but the comment is replaced by spaces of the same
    /// display width, so every following column on the line keeps its number
    /// as well. Tabs are expanded to the next multiple of eight. A line of
    /// spaces under a YAML block scalar body is indented into it, so the YAML
    /// exception above applies here too — and reaches further, because it
    /// applies whatever the block scalar chomps.
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
    ///
    /// A removal never leaves more consecutive blank lines than the longest
    /// run it was already standing next to. A comment set off by a blank line
    /// above and another below is three lines of file for one comment, and
    /// taking only the middle one would leave the two blanks touching — a run
    /// one line longer than the file ever had. So a removal with `before`
    /// blanks above it and `after` below takes `min(before, after)` of the
    /// ones below, leaving `max(before, after)` behind. Blank lines above a
    /// removal are never touched and no more are taken than followed the
    /// comment, so two lines of code that had a blank line between them still
    /// do.
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
/// [`Self::default`] is the [`Policy::Standard`] policy with no overrides.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
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
    /// What a comment has to be to survive, beyond what its kind decides.
    pub allow: AllowRules,
    /// Markers this project's own tools read, and how strongly each is held.
    ///
    /// A directive is a comment addressed to a tool, and the catalogue of them
    /// this crate ships knows the tools everybody uses. It cannot know yours.
    /// A project whose mutation tester reads `// rust-mutants: skip` had only
    /// `keep_regex` to protect it, and a pattern does not change what the
    /// comment *is*: the comment stayed an ordinary line comment that
    /// `--policy all` was entitled to remove, and the project's own build read
    /// something the tool had decided was prose.
    ///
    /// A pattern here decides the comment's kind. The weaker tier records it
    /// as [`CommentKind::Directive`], which every policy but
    /// [`Policy::All`] keeps; the stronger one records it as
    /// [`CommentKind::LoadBearing`], which no policy reaches and only
    /// [`Self::force_protected`] gives up. This is the same field a
    /// [`DeclarativeProfile`](crate::DeclarativeProfile) carries, applied to
    /// every file rather than to one format — the question a profile answers
    /// about its own syntax is the question a project answers about its own
    /// tooling.
    pub protected: Vec<crate::ProtectedPattern>,
}

/// What a comment has to be, beyond being of a kind the policy keeps.
///
/// The policy decides by kind, and a kind is a coarse thing to decide by: a
/// one-line `// NOTE:` explaining a decision and a forty-line essay above a
/// function are both `line`, and a project that wants the first and not the
/// second cannot say so. These are the other axes, and they cut across the
/// policy rather than under it — a comment that fails one of them is removed
/// whatever kept it, short of the protections no policy reaches.
///
/// Empty or `None` everywhere means "no opinion", which is what every
/// configuration written before these existed meant.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AllowRules {
    /// The tags a surviving comment may open with, without punctuation:
    /// `["NOTE", "SAFETY"]`.
    ///
    /// Matched against the comment's *text* — its delimiters removed, and the
    /// common prefix of a block comment's lines removed with them — so the
    /// same convention holds in every language. A `keep_regex` cannot do this:
    /// it is matched against the whole raw token, so `^#\s*NOTE` protects a
    /// Python comment and silently fails to protect the identical rule written
    /// in Lua, where the token opens `--`.
    ///
    /// Empty means no tag rule at all. A non-empty list means a comment
    /// carrying one of these tags is kept, and says nothing about the ones
    /// that do not.
    pub tags: Vec<String>,
    /// How many lines a comment, or a run of comments with nothing between
    /// them, may occupy.
    ///
    /// `Some(1)` is the strictest useful value: a comment may be one line and
    /// no more. This is the axis a kind cannot express, and it is usually the
    /// real complaint — not that a comment exists, but that it goes on.
    ///
    /// A run is measured rather than a single token because four consecutive
    /// `//` lines are four comments to a scanner and one paragraph to a
    /// reader, and the reader is right.
    pub max_lines: Option<usize>,
    /// Whether a comment may sit after code on the same line.
    ///
    /// `Some(false)` removes them. It closes the obvious way around a rule
    /// about comments above code, which is to put the comment beside it
    /// instead.
    pub trailing: Option<bool>,
    /// Tags that are a promise rather than a remark, and how long each has.
    ///
    /// A `TODO` is not the same kind of thing as a `SAFETY`. One records why
    /// the code is the way it is and is true for as long as the code is; the
    /// other says somebody will do something, and saying so is not doing it.
    /// A rule that treats them alike either forbids writing a `TODO` at all —
    /// which nobody obeys, and which loses the note along with the nagging —
    /// or permits one forever, which is how a repository ends up with a `TODO`
    /// from four years ago that everybody has learned to read past.
    ///
    /// A tag here is allowed exactly as one in [`Self::tags`] is, until the
    /// line carrying it reaches this age; after that it is a finding. The age
    /// is measured from the commit that introduced the line, so writing one
    /// costs nothing and a deadline starts running only once the promise is
    /// part of the repository. [`Age::ZERO`] therefore means "from the next
    /// commit".
    ///
    /// Nothing in this crate produces the resulting verdict: measuring the age
    /// means reading a repository, and this crate performs no I/O. It owns the
    /// vocabulary — [`ShapeRule::Expired`] — so that a caller with a clock
    /// reports through the same channel every other rule reports through.
    pub expiry: BTreeMap<String, Age>,
}

impl AllowRules {
    /// Every tag a comment may open with, whether or not it comes with a
    /// deadline.
    ///
    /// A tag under [`Self::expiry`] does not have to be repeated in
    /// [`Self::tags`]: it is allowed for as long as it is allowed, and a
    /// configuration that had to list it twice would let the two lists
    /// disagree.
    pub fn every_tag(&self) -> Vec<&str> {
        self.tags
            .iter()
            .map(String::as_str)
            .chain(self.expiry.keys().map(String::as_str))
            .collect()
    }

    /// Whether any rule here is set at all.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
            && self.max_lines.is_none()
            && self.trailing.is_none()
            && self.expiry.is_empty()
    }
}

/// How long a promise has, in days.
///
/// Written `"14d"` or `"2w"` in a configuration, and `"0d"` for a deadline
/// that starts at the next commit. Days are the smallest unit because the
/// clock this is measured against is a commit date, and nobody writes a
/// `TODO` with an afternoon in mind.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Age {
    days: u32,
}

impl Age {
    /// Due at the next commit: the line is over its deadline the moment it has
    /// one.
    pub const ZERO: Self = Self { days: 0 };

    /// This many days.
    pub const fn from_days(days: u32) -> Self {
        Self { days }
    }

    /// How many days this is.
    pub const fn days(self) -> u32 {
        self.days
    }
}

impl fmt::Display for Age {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}d", self.days)
    }
}

impl FromStr for Age {
    type Err = String;

    /// `14d`, `2w`, or a bare number of days.
    ///
    /// The unit is required to be one a commit date can answer: an hour is not
    /// a meaningful deadline for a line of source, and a month is not a fixed
    /// number of days. Weeks are offered because that is how the deadline is
    /// usually said out loud.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim();
        let (digits, multiplier) = match trimmed.strip_suffix(['d', 'D']) {
            Some(digits) => (digits, 1),
            None => match trimmed.strip_suffix(['w', 'W']) {
                Some(digits) => (digits, 7),
                None => (trimmed, 1),
            },
        };
        let days: u32 = digits.trim().parse().map_err(|_| {
            format!("cannot read `{value}` as an age; write it as `14d`, `2w`, or a number of days")
        })?;
        days.checked_mul(multiplier)
            .map(Self::from_days)
            .ok_or_else(|| format!("`{value}` is too long an age to measure"))
    }
}

impl Serialize for Age {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Age {
    /// Read from a string, and from a bare integer for the configuration that
    /// writes `TODO = 14`.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Written {
            Text(String),
            Days(u32),
        }
        match Written::deserialize(deserializer)? {
            Written::Text(text) => text.parse().map_err(serde::de::Error::custom),
            Written::Days(days) => Ok(Self::from_days(days)),
        }
    }
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            policy: Policy::Standard,
            dialect: Dialect::Standard,
            force_invalid: false,
            force_protected: false,
            keep_kinds: Vec::new(),
            remove_kinds: Vec::new(),
            keep_regex: Vec::new(),
            remove_regex: Vec::new(),
            allow: AllowRules::default(),
            protected: Vec::new(),
        }
    }
}

/// A [`ScanOptions`] and what to leave behind in place of each removal.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
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

/// The scan and edits of a transformation, before output bytes are built.
///
/// Planning is the useful half of a transformation for a checker, a report
/// renderer, or a caller that wants to inspect or filter edits. It deliberately
/// carries neither a copy of the transformed source nor a source map. Call
/// [`Self::finish`] only when those materialized results are needed.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransformPlan {
    /// The edits selected by the scan, sorted and non-overlapping.
    pub edits: Vec<Edit>,
    /// The scan those edits were decided from.
    pub report: ScanReport,
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
