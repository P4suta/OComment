use crate::{
    ByteSpan, Comment, CommentKind, Diagnostic, Dialect, Disposition, DispositionExplanation,
    Language, Policy, ScanOptions, ScanReport, Severity,
};
use memchr::{memchr, memchr2, memchr3, memmem};
use regex::bytes::RegexSet;
use std::cmp::Ordering;

/// Find every comment in `source` and decide what happens to each.
///
/// One pass over the bytes, with the source never decoded as a whole: the
/// spans that come back are byte offsets into `source`, which does not have
/// to be valid UTF-8. Nothing is written and no output is built — that is
/// [`transform`](crate::transform).
///
/// A source that will not lex, such as one with an unterminated comment or
/// string, comes back with a [`Severity::Error`](crate::Severity::Error)
/// diagnostic and [`ScanReport::valid`] false.
///
/// # Examples
///
/// ```
/// use ocomment_core::{CommentKind, Language, ScanOptions, scan};
///
/// let report = scan(b"let x = 1; // note\n", Language::Rust, ScanOptions::default());
/// assert!(report.valid);
/// assert_eq!(report.comments.len(), 1);
/// assert_eq!(report.comments[0].kind, CommentKind::Line);
/// assert!(report.comments[0].disposition.is_remove());
///
/// // A build tag is a directive, and the default policy keeps one.
/// let tagged = scan(b"//go:build linux\n", Language::Go, ScanOptions::default());
/// assert_eq!(tagged.comments[0].kind, CommentKind::Directive);
/// assert!(!tagged.comments[0].disposition.is_remove());
/// ```
pub fn scan(source: &[u8], language: Language, options: ScanOptions) -> ScanReport {
    scan_internal(source, language, options, 0, false, None).0
}

pub(crate) fn scan_with_checkpoints(
    source: &[u8],
    language: Language,
    options: ScanOptions,
    offset: usize,
) -> (ScanReport, Vec<usize>) {
    let (report, checkpoints, _) = scan_internal(source, language, options, offset, true, None);
    (report, checkpoints)
}

/// Scan `source` — which must be the *whole* remaining document suffix, so that
/// every lexical lookahead sees the bytes a full scan would — but stop as soon
/// as the scanner reaches the clean top-level state at absolute offset `stop`.
///
/// Returns whether that state was actually reached. When it was not, the scan
/// ran to the end of `source` and the report covers the entire suffix. Handing
/// the scanner a slice that had been truncated at `stop` instead would let a
/// bounded lookahead straddling the cut decide differently than it does in the
/// real document.
pub(crate) fn scan_until_checkpoint(
    source: &[u8],
    language: Language,
    options: ScanOptions,
    offset: usize,
    stop: usize,
) -> (ScanReport, Vec<usize>, bool) {
    scan_internal(source, language, options, offset, true, Some(stop))
}

/// Every safe checkpoint a scan of `source` offers, paired with the watermark
/// standing when it was offered — one past the furthest byte any decision made
/// before it had read.
///
/// Test-only, and the whole of what [`Reach`] is for: the checkpoint-soundness
/// property asserts the mechanism directly rather than inferring it from a
/// rescan that may re-lex the same bytes by luck.
#[cfg(test)]
pub(crate) fn scan_checkpoint_watermarks(
    source: &[u8],
    language: Language,
    options: ScanOptions,
) -> Vec<(usize, usize)> {
    let mut scanner = Scanner::with_offset(source, language, options, 0, true, None);
    scanner.scan_language();
    scanner
        .safe_checkpoints
        .iter()
        .copied()
        .zip(scanner.checkpoint_watermarks)
        .collect()
}

fn scan_internal(
    source: &[u8],
    language: Language,
    options: ScanOptions,
    offset: usize,
    track_checkpoints: bool,
    stop: Option<usize>,
) -> (ScanReport, Vec<usize>, bool) {
    let mut scanner =
        Scanner::with_offset(source, language, options, offset, track_checkpoints, stop);
    scanner.scan_language();
    debug_assert!(scanner.comments.windows(2).all(|comments| {
        comments[0].span.start < comments[1].span.start
            && comments[0].span.end <= comments[1].span.start
    }));
    let valid = !scanner
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    (
        ScanReport {
            language,
            comments: scanner.comments,
            diagnostics: scanner.diagnostics,
            valid,
        },
        scanner.safe_checkpoints,
        scanner.stopped,
    )
}

/// One past the furthest byte a lookahead read, in the coordinates of the
/// slice it was handed.
///
/// INVARIANT: a safe checkpoint promises that nothing decided before it depends
/// on bytes at or after it, and [`Scanner::add_safe_checkpoint`] keeps that
/// promise by refusing any position an earlier decision already read through.
/// That is a mechanism rather than an audit only while every *lookahead*
/// reports how far it went: a helper takes `&mut Reach`, records each byte as
/// it consults it — a `get` that came back `None` included, because deciding
/// that the document ends there is a decision about that byte just the same —
/// and its caller folds the result into the scan with [`Scanner::consult`].
///
/// NOTE: a plain forward scan is not a lookahead and reports nothing: the index
/// consumes every byte it reads, so a restart inside the region reaches the
/// same answer for what is left of it. What has to be reported is the read that
/// the scan then *rewinds* behind — a delimiter parse that gives up and lexes
/// the same bytes again, an unbounded search for a closing token that fails —
/// because the bytes past the resume point have already decided something no
/// rescan from a later checkpoint would revisit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Reach(usize);

impl Reach {
    /// Record a read of every byte below `end`.
    fn through(&mut self, end: usize) {
        self.0 = self.0.max(end);
    }

    /// Record a read of the byte at `index`.
    fn byte(&mut self, index: usize) {
        self.through(index + 1);
    }

    /// Record a search that ran off the end of `bytes`.
    ///
    /// One byte further than the document holds, because what such a search
    /// decided out of is not only every byte it crossed but the end itself —
    /// and an append is an edit exactly there. A checkpoint at the end of the
    /// document is the one an append reuses the whole prefix from, so it is
    /// the one this has to withdraw.
    fn end_of(&mut self, bytes: &[u8]) {
        self.through(bytes.len() + 1);
    }
}

/// One past the last byte a bounded window of `width` bytes at `index` can
/// consult: the windows that tell a character literal from something else stop
/// at the first line terminator, and read that terminator to know they must.
fn line_bounded_reach(bytes: &[u8], index: usize, width: usize) -> usize {
    let limit = (index + width).min(bytes.len());
    let window = &bytes[index.min(limit)..limit];
    window
        .iter()
        .position(|byte| is_line_terminator(*byte))
        .map_or(limit, |stop| index + stop + 1)
}

struct Scanner<'a> {
    source: &'a [u8],
    language: Language,
    options: ScanOptions,
    comments: Vec<Comment>,
    diagnostics: Vec<Diagnostic>,
    offset: usize,
    patterns: DispositionPatterns,
    safe_checkpoints: Vec<usize>,
    track_checkpoints: bool,
    stop: Option<usize>,
    stopped: bool,
    /// One past the furthest byte any decision so far consulted, in the local
    /// coordinates every index here is in. See [`Reach`].
    consulted: usize,
    /// The watermark as each safe checkpoint was offered, in the order
    /// `safe_checkpoints` holds them. Test-only: it is what lets the
    /// checkpoint-soundness property assert the mechanism itself, instead of
    /// trusting that a rescan happens to re-lex whatever a lookahead read.
    #[cfg(test)]
    checkpoint_watermarks: Vec<usize>,
    restart_rules: RestartRules,
    /// Every YAML block scalar the scan walked over, in source order. Empty
    /// for every other language, and for the YAML documents — nearly all of
    /// them — that hold no block scalar at all.
    yaml_blocks: Vec<YamlBlockScalar>,
}

impl<'a> Scanner<'a> {
    fn with_offset(
        source: &'a [u8],
        language: Language,
        options: ScanOptions,
        offset: usize,
        track_checkpoints: bool,
        stop: Option<usize>,
    ) -> Self {
        let (patterns, pattern_error) = match DispositionPatterns::compile(&options) {
            Ok(patterns) => (patterns, None),
            Err(error) => (DispositionPatterns::empty(), Some(error.to_string())),
        };
        let mut scanner = Self {
            source,
            language,
            options,
            comments: Vec::new(),
            diagnostics: Vec::new(),
            offset,
            patterns,
            safe_checkpoints: track_checkpoints
                .then_some(vec![offset])
                .unwrap_or_default(),
            track_checkpoints,
            stop,
            stopped: false,
            consulted: 0,
            #[cfg(test)]
            checkpoint_watermarks: track_checkpoints
                .then_some(vec![offset])
                .unwrap_or_default(),
            restart_rules: RestartRules::of(source, language),
            yaml_blocks: Vec::new(),
        };
        if let Some(error) = pattern_error {
            scanner.error(
                "invalid-policy-regex",
                &format!("invalid comment policy regex: {error}"),
                ByteSpan::new(0, 0),
            );
        }
        scanner
    }

    fn child(source: &'a [u8], language: Language, options: ScanOptions, offset: usize) -> Self {
        let patterns =
            DispositionPatterns::compile(&options).unwrap_or_else(|_| DispositionPatterns::empty());
        Self {
            source,
            language,
            options,
            comments: Vec::new(),
            diagnostics: Vec::new(),
            offset,
            patterns,
            safe_checkpoints: Vec::new(),
            track_checkpoints: false,
            stop: None,
            stopped: false,
            consulted: 0,
            #[cfg(test)]
            checkpoint_watermarks: Vec::new(),
            restart_rules: RestartRules::of(source, language),
            yaml_blocks: Vec::new(),
        }
    }

    /// Run the scanner for its own language.
    fn scan_language(&mut self) {
        match self.language {
            Language::Rust
            | Language::C
            | Language::Cpp
            | Language::Go
            | Language::Kotlin
            | Language::Css
            | Language::Jsonc => self.scan_c_family(),
            Language::Java => self.scan_java(),
            Language::JavaScript | Language::TypeScript => self.scan_javascript(),
            Language::Ocaml => self.scan_ocaml(),
            Language::Python => self.scan_python(),
            Language::Shell => self.scan_shell(),
            Language::Html => self.scan_html(),
            Language::Sql => self.scan_sql(),
            Language::Toml => self.scan_toml(),
            Language::Lua => self.scan_lua(),
            Language::Yaml => self.scan_yaml(),
            Language::Php => self.scan_php(),
            Language::Ruby => self.scan_ruby(),
            Language::Zig => self.scan_zig(),
            Language::R => self.scan_r(),
            Language::Dart => self.scan_dart(),
            Language::Swift => self.scan_swift(),
            Language::CSharp => self.scan_csharp(),
            Language::Scala => self.scan_scala(),
            Language::Unknown => self.error(
                "unknown-language",
                "a language is required",
                ByteSpan::new(0, 0),
            ),
        }
    }

    /// Fold what a lookahead read into the scan's watermark.
    ///
    /// Clamped to the source, so that a `get` past the last byte does not
    /// withdraw the checkpoint a trailing line terminator earns.
    fn consult(&mut self, reach: Reach) {
        self.consulted = self.consulted.max(reach.0.min(self.source.len() + 1));
    }

    fn error(&mut self, code: &str, message: &str, span: ByteSpan) {
        self.diagnostics.push(Diagnostic {
            code: code.into(),
            message: message.into(),
            severity: Severity::Error,
            span: ByteSpan::new(span.start + self.offset, span.end + self.offset),
        });
    }

    fn add_comment(&mut self, start: usize, end: usize, lexical_kind: CommentKind) {
        let kind = classify_comment(
            self.source,
            self.language,
            lexical_kind,
            start,
            end,
            self.offset,
        );
        let raw = &self.source[start.min(self.source.len())..end.min(self.source.len())];
        let disposition = disposition(kind, &self.options, raw, &self.patterns);
        self.comments.push(Comment {
            span: ByteSpan::new(start + self.offset, end + self.offset),
            kind,
            disposition,
        });
    }

    fn merge_child(&mut self, child: Scanner<'_>) {
        self.comments.extend(child.comments);
        self.diagnostics.extend(child.diagnostics);
    }

    /// Whether a checkpoint the scanner is about to emit at `local` is a
    /// restart point, asked of the same [`RestartRules`] the incremental engine
    /// consults before reusing one. A suffix scan is past the preamble by
    /// construction — its source starts mid-document, so the offset-sensitive
    /// rules cannot fire for it, and the engine validates the offset it
    /// restarts *from* against the whole edited document instead.
    ///
    /// The block scalar rule is asked of a suffix scan as well, because it is
    /// not about where the offset sits in a document: it is about what the
    /// bytes it is being asked of hold, and the suffix holds its own.
    fn checkpoint_is_restartable(&self, local: usize) -> bool {
        local <= self.restart_rules.first_block_scalar
            && (self.offset > 0 || self.restart_rules.permit_restart_at(self.source, local))
            && (self.restart_rules.language != Language::Scala
                || the_scala_xml_boundary_permits_a_restart(self.source, local))
    }

    /// Offer `local` as a restart point, if it may stand as one.
    ///
    /// INVARIANT: a checkpoint promises that nothing decided before it depends
    /// on bytes at or after it, so it may only stand where no earlier decision
    /// read past it. That is what the watermark is for: a position below what
    /// [`Reach`] has already recorded is refused outright, rather than left to
    /// an audit of which lookahead reaches how far. Refusing one costs a
    /// rescan the chance to start there; keeping an unsound one corrupts the
    /// rescan silently.
    fn add_safe_checkpoint(&mut self, local: usize) {
        if !self.track_checkpoints
            || local < self.consulted
            || !self.checkpoint_is_restartable(local)
        {
            return;
        }
        let absolute = self.offset + local;
        if self.safe_checkpoints.last().copied() != Some(absolute) {
            self.safe_checkpoints.push(absolute);
            #[cfg(test)]
            self.checkpoint_watermarks
                .push(self.offset + self.consulted);
        }
        if self.stop == Some(absolute) {
            self.stopped = true;
        }
    }

    fn add_safe_newlines(&mut self, mut start: usize, end: usize) {
        if !self.track_checkpoints {
            return;
        }
        while start < end && !self.stopped {
            let Some(relative) = memchr2(b'\r', b'\n', &self.source[start..end]) else {
                break;
            };
            let newline = start + relative;
            let next = consume_newline(self.source, newline).min(end);
            self.add_safe_checkpoint(next);
            start = next;
        }
    }

    fn scan_c_family(&mut self) {
        /* INVARIANT: Translation phase 2 line splicing is significant to C-family lexical
         * input. The remapped copy is scanned by a child, which tracks no
         * checkpoints — that is the document-wide half of the restart rules, and
         * it is the same answer the incremental engine gets from them. */
        if !self.restart_rules.splicing_permits_restarts {
            let mapped = MappedBytes::without_c_line_splices(self.source);
            let mut child = Scanner::child(&mapped.bytes, self.language, self.options.clone(), 0);
            child.scan_c_family_unmapped();
            self.merge_mapped(child, &mapped);
        } else {
            self.scan_c_family_unmapped();
        }
    }

    fn scan_c_family_unmapped(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        while index < bytes.len() {
            let Some(next) = next_c_family_trigger(bytes, index, self.language) else {
                self.add_safe_newlines(index, bytes.len());
                break;
            };
            self.add_safe_newlines(index, next);
            if self.stopped {
                break;
            }
            index = next;
            if starts(bytes, index, b"//") && self.language != Language::Css {
                let end = line_end(bytes, index + 2);
                self.add_comment(index, end, line_kind(bytes, index));
                index = end;
                continue;
            }
            if starts(bytes, index, b"/*") {
                let nested = matches!(self.language, Language::Rust | Language::Kotlin);
                let (end, closed) = block_end(bytes, index, b"/*", b"*/", nested);
                self.add_comment(index, end, block_kind(bytes, index));
                if !closed {
                    self.error(
                        "unterminated-comment",
                        "unterminated block comment",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            if let Some(end) = self.special_c_string(index) {
                index = end;
                continue;
            }
            index += 1;
        }
    }

    fn special_c_string(&mut self, index: usize) -> Option<usize> {
        let bytes = self.source;
        match self.language {
            Language::Rust => {
                if bytes[index] == b'"'
                    && let Some((raw_start, hashes)) = rust_raw_start_at_quote(bytes, index)
                {
                    let content = index + 1;
                    let mut end_token = Vec::with_capacity(hashes + 1);
                    end_token.push(b'"');
                    end_token.extend(std::iter::repeat_n(b'#', hashes));
                    if let Some(relative) = find_subslice(&bytes[content..], &end_token) {
                        return Some(content + relative + end_token.len());
                    }
                    self.error(
                        "unterminated-string",
                        "unterminated Rust raw string",
                        ByteSpan::new(raw_start, bytes.len()),
                    );
                    return Some(bytes.len());
                }
                if bytes[index] == b'"' || (starts(bytes, index, b"b\"") && index + 1 < bytes.len())
                {
                    let quote = if bytes[index] == b'b' {
                        index + 1
                    } else {
                        index
                    };
                    /* INVARIANT: a Rust string or byte-string literal carries a
                     * bare newline as content, unlike its C, Go, and Java
                     * cousins, so only the closing quote or the end of the file
                     * ends one. A character literal below still ends at the
                     * line, which is what keeps a lifetime from swallowing the
                     * rest of the source. */
                    return Some(self.quoted_or_error(quote, true, "string"));
                }
                if bytes[index] == b'\'' {
                    let mut reach = Reach::default();
                    let literal = rust_char_start(bytes, index, &mut reach);
                    self.consult(reach);
                    if literal {
                        return Some(self.quoted_or_error(index, false, "character literal"));
                    }
                    /* NOTE: What is left is an apostrophe this window read as
                     * no literal, and nothing on its line says whether it
                     * opens one. A Rust identifier is `XID_Start
                     * XID_Continue*` (Rust Reference, Identifiers) and has
                     * been since 1.53, so `'ä` is as good a lifetime or loop
                     * label as `'a` -- `fn f<'ä>() {}` and `'ä: loop {}` both
                     * compile -- and an unterminated non-ASCII character
                     * literal is spelled the same way within one line. `rustc`
                     * tells the two apart in the parser, which is where E0762
                     * is raised; this scanner is a lexer with a line-bounded
                     * window and cannot. So it reports neither: over-keeping a
                     * comment is the safe direction, calling a valid file
                     * invalid is not. */
                }
            }
            Language::C | Language::Cpp => {
                let raw = (self.language == Language::Cpp && bytes[index] == b'"')
                    .then(|| cpp_raw_start_at_quote(bytes, index))
                    .flatten()
                    .and_then(|raw_start| {
                        let mut reach = Reach::default();
                        let raw = cpp_raw_string(bytes, raw_start, &mut reach);
                        self.consult(reach);
                        raw.map(|(end, closed)| (raw_start, end, closed))
                    });
                if let Some((raw_start, end, closed)) = raw {
                    if !closed {
                        self.error(
                            "unterminated-string",
                            "unterminated C++ raw string",
                            ByteSpan::new(raw_start, end),
                        );
                    }
                    return Some(end);
                }
                if is_c_quote_start(bytes, index) {
                    let quote_index = if matches!(bytes[index], b'"' | b'\'') {
                        index
                    } else {
                        (index..(index + 3).min(bytes.len()))
                            .find(|i| matches!(bytes[*i], b'"' | b'\''))
                            .unwrap_or(index)
                    };
                    return Some(self.quoted_or_error(
                        quote_index,
                        false,
                        "string or character literal",
                    ));
                }
            }
            Language::Go => {
                if bytes[index] == b'`' {
                    return Some(self.delimited_or_error(index, b"`", "raw string"));
                }
                if matches!(bytes[index], b'"' | b'\'') {
                    return Some(self.quoted_or_error(index, false, "string or rune literal"));
                }
            }
            Language::Kotlin => {
                if starts(bytes, index, b"\"\"\"") {
                    return Some(self.scan_kotlin_string(index, true, 0));
                }
                if bytes[index] == b'"' {
                    return Some(self.scan_kotlin_string(index, false, 0));
                }
                if bytes[index] == b'\'' {
                    return Some(self.quoted_or_error(index, false, "Kotlin character literal"));
                }
            }
            Language::Jsonc => {
                /* NOTE: JSON5 4.4 writes a string with either quote, and this
                 * language is `JSON with comments, including JSON5` — it owns
                 * `.json5` as well as `.jsonc`. An apostrophe is already
                 * invalid in the stricter dialect, so reading one as a string
                 * only hides a `//` that the dialect could not have meant as a
                 * comment. Both quotes report the one construct a reader
                 * recognises, a JSON string. */
                if matches!(bytes[index], b'"' | b'\'') {
                    return Some(self.quoted_or_error(index, false, "JSON string"));
                }
            }
            Language::Css => {
                if matches!(bytes[index], b'"' | b'\'') {
                    return Some(self.quoted_or_error(index, true, "CSS string"));
                }
            }
            _ => {}
        }
        None
    }

    fn scan_kotlin_string(&mut self, start: usize, triple: bool, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "Kotlin string-template nesting limit exceeded",
                ByteSpan::new(start, start),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let delimiter = if triple { b"\"\"\"".as_slice() } else { b"\"" };
        let mut index = start + delimiter.len();
        while index < bytes.len() {
            if starts(bytes, index, delimiter) {
                return index + delimiter.len();
            }
            if !triple && bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
            } else if starts(bytes, index, b"${") {
                index = self.scan_kotlin_expression(index + 2, depth + 1);
            } else if !triple && matches!(bytes[index], b'\r' | b'\n') {
                self.error(
                    "unterminated-string",
                    "unterminated Kotlin string",
                    ByteSpan::new(start, index),
                );
                return index;
            } else {
                index += 1;
            }
        }
        self.error(
            "unterminated-string",
            if triple {
                "unterminated Kotlin triple-quoted string"
            } else {
                "unterminated Kotlin string"
            },
            ByteSpan::new(start, index),
        );
        index
    }

    fn scan_kotlin_expression(&mut self, mut index: usize, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "Kotlin string-template nesting limit exceeded",
                ByteSpan::new(index, index),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let mut braces = 1usize;
        while index < bytes.len() {
            if starts(bytes, index, b"//") {
                let end = line_end(bytes, index + 2);
                self.add_comment(index, end, line_kind(bytes, index));
                index = end;
                continue;
            }
            if starts(bytes, index, b"/*") {
                let (end, closed) = block_end(bytes, index, b"/*", b"*/", true);
                self.add_comment(index, end, block_kind(bytes, index));
                if !closed {
                    self.error(
                        "unterminated-comment",
                        "unterminated Kotlin block comment",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            if starts(bytes, index, b"\"\"\"") {
                index = self.scan_kotlin_string(index, true, depth + 1);
                continue;
            }
            match bytes[index] {
                b'"' => index = self.scan_kotlin_string(index, false, depth + 1),
                b'\'' => {
                    index = self.quoted_or_error(index, false, "Kotlin character literal");
                }
                b'{' => {
                    braces += 1;
                    index += 1;
                }
                b'}' => {
                    braces -= 1;
                    index += 1;
                    if braces == 0 {
                        return index;
                    }
                }
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-template-expression",
            "unterminated Kotlin string-template expression",
            ByteSpan::new(index, index),
        );
        index
    }

    fn quoted_or_error(&mut self, start: usize, multiline: bool, name: &str) -> usize {
        let quote = self.source[start];
        let mut index = start + 1;
        while index < self.source.len() {
            if self.source[index] == b'\\' {
                if index + 1 < self.source.len() {
                    index += 2;
                } else {
                    index += 1;
                }
            } else if self.source[index] == quote {
                return index + 1;
            } else if !multiline && matches!(self.source[index], b'\r' | b'\n') {
                self.error(
                    "unterminated-string",
                    &format!("unterminated {name}"),
                    ByteSpan::new(start, index),
                );
                return index;
            } else {
                index += 1;
            }
        }
        self.error(
            "unterminated-string",
            &format!("unterminated {name}"),
            ByteSpan::new(start, index),
        );
        index
    }

    fn js_quoted_or_error(&mut self, start: usize) -> usize {
        let quote = self.source[start];
        let mut index = start + 1;
        while index < self.source.len() {
            if self.source[index] == b'\\' {
                let escaped = index + 1;
                if let Some(width) = unicode_line_terminator_width(self.source, escaped) {
                    index = escaped + width;
                } else {
                    index = (index + 2).min(self.source.len());
                }
            } else if self.source[index] == quote {
                return index + 1;
            } else if unicode_line_terminator_width(self.source, index).is_some() {
                self.error(
                    "unterminated-string",
                    "unterminated JavaScript string",
                    ByteSpan::new(start, index),
                );
                return index;
            } else {
                index += 1;
            }
        }
        self.error(
            "unterminated-string",
            "unterminated JavaScript string",
            ByteSpan::new(start, index),
        );
        index
    }

    fn delimited_or_error(&mut self, start: usize, delimiter: &[u8], name: &str) -> usize {
        let content = start + delimiter.len();
        if let Some(relative) = find_subslice(&self.source[content..], delimiter) {
            content + relative + delimiter.len()
        } else {
            self.error(
                "unterminated-string",
                &format!("unterminated {name}"),
                ByteSpan::new(start, self.source.len()),
            );
            self.source.len()
        }
    }

    fn scan_java(&mut self) {
        let (mapped, invalid_unicode) = MappedBytes::java_unicode(self.source);
        for span in invalid_unicode {
            self.error(
                "invalid-unicode-escape",
                "invalid Java Unicode escape",
                span,
            );
        }
        let mut child = Scanner::child(&mapped.bytes, Language::Java, self.options.clone(), 0);
        let mut index = 0;
        while index < child.source.len() {
            if starts(child.source, index, b"//") {
                let end = line_end(child.source, index + 2);
                child.add_comment(index, end, java_line_kind(child.source, index));
                index = end;
                continue;
            }
            if starts(child.source, index, b"/*") {
                let (end, closed) = block_end(child.source, index, b"/*", b"*/", false);
                child.add_comment(index, end, java_block_kind(child.source, index));
                if !closed {
                    child.error(
                        "unterminated-comment",
                        "unterminated block comment",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            if starts(child.source, index, b"\"\"\"") {
                let (end, closed) = java_text_block_end(child.source, index);
                if !closed {
                    child.error(
                        "unterminated-string",
                        "unterminated Java text block",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            if matches!(child.source[index], b'"' | b'\'') {
                index = child.quoted_or_error(index, false, "Java literal");
                continue;
            }
            index += 1;
        }
        self.merge_mapped(child, &mapped);
    }

    fn merge_mapped(&mut self, child: Scanner<'_>, mapped: &MappedBytes) {
        for mut comment in child.comments {
            comment.span = mapped.original_span(comment.span);
            comment.span.start += self.offset;
            comment.span.end += self.offset;
            self.comments.push(comment);
        }
        for mut diagnostic in child.diagnostics {
            diagnostic.span = mapped.original_span(diagnostic.span);
            diagnostic.span.start += self.offset;
            diagnostic.span.end += self.offset;
            self.diagnostics.push(diagnostic);
        }
    }

    fn scan_ocaml(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        while index < bytes.len() && !self.stopped {
            if starts(bytes, index, b"(*") {
                let mut reach = Reach::default();
                let (end, closed) = ocaml_comment_end(bytes, index, &mut reach);
                self.consult(reach);
                self.add_comment(
                    index,
                    end,
                    if starts(bytes, index, b"(**") {
                        CommentKind::DocBlock
                    } else {
                        CommentKind::Block
                    },
                );
                if !closed {
                    self.error(
                        "unterminated-comment",
                        "unterminated OCaml comment",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            let mut reach = Reach::default();
            let quoted = ocaml_quoted_string(bytes, index, &mut reach);
            self.consult(reach);
            if let Some((end, closed)) = quoted {
                if !closed {
                    self.error(
                        "unterminated-string",
                        "unterminated OCaml quoted string",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            if bytes[index] == b'"' {
                index = self.quoted_or_error(index, true, "OCaml string");
                continue;
            }
            if bytes[index] == b'\'' {
                let mut reach = Reach::default();
                let literal = ocaml_char_start(bytes, index, &mut reach);
                self.consult(reach);
                if literal {
                    index = self.quoted_or_error(index, false, "OCaml character literal");
                    continue;
                }
            }
            if matches!(bytes[index], b'\r' | b'\n') {
                index = consume_newline(bytes, index);
                self.add_safe_checkpoint(index);
            } else {
                index += 1;
            }
        }
    }

    fn scan_python(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        while index < bytes.len() && !self.stopped {
            if bytes[index] == b'#' {
                let end = line_end(bytes, index + 1);
                self.add_comment(index, end, CommentKind::Line);
                index = end;
                continue;
            }
            if let Some((quote_start, triple, formatted, raw)) = python_string_start(bytes, index) {
                if formatted {
                    index = self.scan_python_fstring(index, quote_start, triple, raw, 0);
                } else {
                    /* NOTE: `index` rather than `quote_start`: a prefix and the quote
                     * after it are one token (Python reference 2.4.1), so an
                     * unterminated `r"` is reported from the `r`, the way the
                     * triple-quoted and f-string paths already report theirs.
                     * The single-quoted case used the generic string reader,
                     * which knows only where the quote was. */
                    index = self.scan_python_delimited(index, quote_start, triple);
                }
                continue;
            }
            if matches!(bytes[index], b'\r' | b'\n') {
                index = consume_newline(bytes, index);
                self.add_safe_checkpoint(index);
            } else {
                index += 1;
            }
        }
    }

    fn scan_python_delimited(
        &mut self,
        token_start: usize,
        quote_start: usize,
        triple: bool,
    ) -> usize {
        let bytes = self.source;
        let length = if triple { 3 } else { 1 };
        let delimiter = &bytes[quote_start..quote_start + length];
        let mut index = quote_start + length;
        while index < bytes.len() {
            if starts(bytes, index, delimiter) {
                return index + length;
            }
            if bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
            } else if !triple && matches!(bytes[index], b'\r' | b'\n') {
                self.error(
                    "unterminated-string",
                    "unterminated Python string",
                    ByteSpan::new(token_start, index),
                );
                return index;
            } else {
                index += 1;
            }
        }
        self.error(
            "unterminated-string",
            if triple {
                "unterminated Python triple-quoted string"
            } else {
                "unterminated Python string"
            },
            ByteSpan::new(token_start, index),
        );
        index
    }

    fn scan_python_fstring(
        &mut self,
        token_start: usize,
        quote_start: usize,
        triple: bool,
        _raw: bool,
        depth: usize,
    ) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "Python f-string nesting limit exceeded",
                ByteSpan::new(token_start, token_start),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let length = if triple { 3 } else { 1 };
        let delimiter = &bytes[quote_start..quote_start + length];
        let mut index = quote_start + length;
        while index < bytes.len() {
            if starts(bytes, index, delimiter) {
                return index + length;
            }
            if bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
            } else if starts(bytes, index, b"{{") || starts(bytes, index, b"}}") {
                index += 2;
            } else if bytes[index] == b'{' {
                index = self.scan_python_expression(index + 1, depth + 1);
            } else if !triple && matches!(bytes[index], b'\r' | b'\n') {
                self.error(
                    "unterminated-string",
                    "unterminated Python f-string",
                    ByteSpan::new(token_start, index),
                );
                return index;
            } else {
                index += 1;
            }
        }
        self.error(
            "unterminated-string",
            "unterminated Python f-string",
            ByteSpan::new(token_start, index),
        );
        index
    }

    fn scan_python_expression(&mut self, mut index: usize, depth: usize) -> usize {
        let bytes = self.source;
        let mut braces = 1usize;
        while index < bytes.len() {
            if bytes[index] == b'#' {
                let end = line_end(bytes, index + 1);
                self.add_comment(index, end, CommentKind::Line);
                index = end;
                continue;
            }
            if let Some((quote_start, triple, formatted, raw)) = python_string_start(bytes, index) {
                index = if formatted {
                    self.scan_python_fstring(index, quote_start, triple, raw, depth + 1)
                } else if triple {
                    self.scan_python_delimited(index, quote_start, true)
                } else {
                    /* NOTE: `index` rather than `quote_start`: a prefix and the
                     * quote after it are one token (Python reference 2.4.1), so
                     * an unterminated `r"` inside a replacement field is
                     * reported from the `r`, the way the delimited reader
                     * reports every other string and the OCaml reference
                     * reports this one. */
                    self.scan_python_delimited(index, quote_start, false)
                };
                continue;
            }
            match bytes[index] {
                b'{' => {
                    braces += 1;
                    index += 1;
                }
                b'}' => {
                    braces -= 1;
                    index += 1;
                    if braces == 0 {
                        return index;
                    }
                }
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-fstring-expression",
            "unterminated Python f-string expression",
            ByteSpan::new(index, index),
        );
        index
    }

    fn scan_toml(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        while index < bytes.len() && !self.stopped {
            match bytes[index] {
                b'#' => {
                    let end = line_end(bytes, index + 1);
                    self.add_comment(index, end, CommentKind::Line);
                    index = end;
                }
                b'"' | b'\'' => index = self.scan_toml_string(index),
                b'\r' | b'\n' => {
                    index = consume_newline(bytes, index);
                    self.add_safe_checkpoint(index);
                }
                _ => index += 1,
            }
        }
    }

    /// One TOML string beginning at its opening quote, whether it is a value
    /// or a quoted key.
    ///
    /// `"` opens a basic string, which takes `\` escapes, and `'` a literal
    /// string, which takes none, so a `\` in a literal string is a byte of it
    /// (TOML v1.0.0, String). Three of either quote open the multi-line form,
    /// where a newline is content instead of the end of an unterminated
    /// string.
    fn scan_toml_string(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let quote = bytes[start];
        let multiline = starts(bytes, start, &[quote, quote, quote]);
        let escapes = quote == b'"';
        let mut index = start + if multiline { 3 } else { 1 };
        while index < bytes.len() {
            if escapes && bytes[index] == b'\\' {
                /* NOTE: This also carries the line-ending backslash of a multi-line
                 * basic string, which continues the string across the newline
                 * it swallows: the newline is content either way, so the two
                 * need no separate rules. */
                index = (index + 2).min(bytes.len());
            } else if bytes[index] != quote {
                if !multiline && matches!(bytes[index], b'\r' | b'\n') {
                    self.error(
                        "unterminated-string",
                        "unterminated TOML string",
                        ByteSpan::new(start, index),
                    );
                    return index;
                }
                index += 1;
            } else if !multiline {
                return index + 1;
            } else {
                /* NOTE: The delimiter is three quotes, and up to two more may sit
                 * in front of it as content, so a run of three or more ends the
                 * string on its last three — after at most five of them. A
                 * sixth quote is past what the grammar lets the delimiter
                 * absorb and belongs to whatever follows the string. */
                let run = toml_quote_run(bytes, index, quote);
                if run >= 3 {
                    return index + run.min(5);
                }
                index += run;
            }
        }
        self.error(
            "unterminated-string",
            if multiline {
                "unterminated TOML multi-line string"
            } else {
                "unterminated TOML string"
            },
            ByteSpan::new(start, index),
        );
        index
    }

    /// One Lua chunk (Lua 5.4 reference manual, 3.1 Lexical Conventions).
    fn scan_lua(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        /* NOTE: The loader skips a first line that opens with `#` before it lexes
         * anything (`lauxlib.c`, `skipcomment`), which is what lets a chunk
         * carry a `#!` line. It is that one byte at that one offset: `#` is the
         * length operator everywhere else. `skipcomment` calls `skipBOM` first,
         * so the offset is behind a UTF-8 byte order mark when the file carries
         * one. The `self.offset` test is what keeps a suffix scan out of the
         * rule, and no checkpoint of a full scan falls inside the first line, so
         * the two answers cannot disagree. */
        let preamble = byte_order_mark_width(bytes);
        if self.offset == 0 && bytes.get(preamble) == Some(&b'#') {
            let end = line_end(bytes, preamble + 1);
            self.add_comment(preamble, end, CommentKind::Line);
            index = end;
        }
        while index < bytes.len() && !self.stopped {
            if starts(bytes, index, b"--") {
                index = self.scan_lua_comment(index);
                continue;
            }
            match bytes[index] {
                b'"' | b'\'' => index = self.scan_lua_short_string(index),
                b'[' => index = self.scan_lua_long_string(index),
                b'\r' | b'\n' => {
                    index = consume_newline(bytes, index);
                    self.add_safe_checkpoint(index);
                }
                _ => index += 1,
            }
        }
    }

    /// One Lua comment beginning at its `--`.
    ///
    /// A long bracket immediately after the `--` opens a long comment, which
    /// runs to the closing bracket of its own level; anything else is a short
    /// comment to the end of the line. The two differ in the byte that
    /// completes the bracket, so `--[=` is a short comment and `--[=[` is not.
    fn scan_lua_comment(&mut self, start: usize) -> usize {
        let bytes = self.source;
        if let Some(level) = long_bracket_level(bytes, start + 2) {
            let (end, closed) = long_bracket_end(bytes, start + 2 + level + 2, level);
            self.add_comment(start, end, CommentKind::Block);
            if !closed {
                self.error(
                    "unterminated-comment",
                    "unterminated Lua long comment",
                    ByteSpan::new(start, end),
                );
            }
            return end;
        }
        let end = line_end(bytes, start + 2);
        self.add_comment(start, end, lua_line_kind(bytes, start));
        end
    }

    /// One Lua long string beginning at `start`, or `start + 1` when no long
    /// bracket opens there: `a[b[1]]` indexes twice and opens nothing.
    fn scan_lua_long_string(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let Some(level) = long_bracket_level(bytes, start) else {
            return start + 1;
        };
        let (end, closed) = long_bracket_end(bytes, start + level + 2, level);
        if !closed {
            self.error(
                "unterminated-string",
                "unterminated Lua long string",
                ByteSpan::new(start, end),
            );
        }
        end
    }

    /// One Lua short string beginning at its quote.
    ///
    /// `\z` skips the whitespace that follows it, newlines included, and a
    /// backslash before a real line terminator carries that terminator into the
    /// string. Any other unescaped line terminator ends a string that was never
    /// closed. The remaining escapes — `\ddd`, `\xXX`, `\u{XXX}` and the
    /// single-character ones — carry no quote and no newline, so skipping the
    /// byte after the backslash finds the same closing quote the whole rule
    /// would.
    fn scan_lua_short_string(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let quote = bytes[start];
        let mut index = start + 1;
        while index < bytes.len() {
            if bytes[index] == b'\\' {
                let escaped = index + 1;
                if bytes.get(escaped) == Some(&b'z') {
                    index = escaped + 1;
                    while bytes.get(index).is_some_and(|byte| lua_is_space(*byte)) {
                        index += 1;
                    }
                } else if let Some(width) = lua_newline_width(bytes, escaped) {
                    index = escaped + width;
                } else {
                    index = (escaped + 1).min(bytes.len());
                }
            } else if bytes[index] == quote {
                return index + 1;
            } else if lua_newline_width(bytes, index).is_some() {
                self.error(
                    "unterminated-string",
                    "unterminated Lua string",
                    ByteSpan::new(start, index),
                );
                return index;
            } else {
                index += 1;
            }
        }
        self.error(
            "unterminated-string",
            "unterminated Lua string",
            ByteSpan::new(start, index),
        );
        index
    }

    /// One YAML stream (YAML 1.2.2 specification).
    ///
    /// The scanner is line-local but for one answer: `#` opens a comment only
    /// where white space separates it from the token in front of it (6.6), the
    /// two quoted styles (7.3.1, 7.3.2) may run over a line break and carry
    /// every `#` inside them as content, and a block scalar (8.1) swallows
    /// every following line that is more indented than the node it hangs off.
    /// That last depth is the exception: a `|` may sit on the line under the
    /// `key:` or the `-` that owns it, or behind node properties (6.9), so the
    /// owner's indentation is carried across the line break rather than read
    /// off the header's own column.
    ///
    /// That is what makes the start of a line a restart point — but only a
    /// line outside a quoted scalar, outside a block scalar body, and with no
    /// owner carried into it, because in each of those the same bytes mean
    /// something else. None of the three emits a checkpoint: a quoted scalar
    /// consumes its line breaks without offering one, the body of a block
    /// scalar is consumed by [`yaml_block_body_end`], which reports where it
    /// ended and whether that offset is the start of a line at all, and a line
    /// under a live carry is skipped outright. Where a body ends is decided by
    /// the lines *below* it, which an edit can move, so
    /// [`first_yaml_block_scalar`] withdraws every checkpoint past the first
    /// header a document opens: what this function offers is what those rules
    /// then hold it to.
    ///
    /// `valid` is a lexical answer here and nothing more. YAML has shapes a
    /// lexer cannot rule out and a parser rejects, and removing a comment can
    /// walk a file from one to the other: a comment line inside a multi-line
    /// plain scalar is a parse error while it is there, and taking it away
    /// leaves a scalar that parses and folds the two halves into one value.
    /// A comment line under a block scalar body is the same hazard read the
    /// other way — see [`lines_a_removal_must_swallow`], which is what stops
    /// the hole a removal leaves from being read back as content of the body
    /// above it. Every block scalar this walks over is recorded in
    /// [`Self::yaml_blocks`] for exactly that question.
    fn scan_yaml(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        let mut line_start = 0;
        /* INVARIANT: `separated` is whether a `#` at `index` would be separated from
         * what precedes it, which is the whole of the comment rule; `node_start`
         * is whether a node may begin here, which is what tells the block scalar
         * indicator `key: >` from the `>` inside the plain scalar `key: a > b`;
         * `token_column` is where the token being read began, so that a `: `
         * behind it can name the column the value hangs off; and `owner_column`
         * is that column once one is known. The first three are reset by the
         * line break; `owner_column` survives it while the node it names is
         * still owed one, which is the only state a restart at a line start
         * cannot reproduce — so no checkpoint is offered while it is set. */
        let mut separated = true;
        let mut node_start = true;
        let mut token_column = None;
        let mut owner_column = None;
        while index < bytes.len() && !self.stopped {
            match bytes[index] {
                b'#' if separated => {
                    let end = line_end(bytes, index + 1);
                    self.add_comment(index, end, CommentKind::Line);
                    index = end;
                }
                b'\r' | b'\n' => {
                    index = consume_newline(bytes, index);
                    line_start = index;
                    separated = true;
                    /* NOTE: A line that ends while a node is still owed — `key:`,
                     * a bare `-`, a property whose node has not come yet, and
                     * the blank and comment lines a separation may hold (6.9,
                     * 8.2.2) — hands the owner's indentation to the line below,
                     * because the `|` of the block scalar it introduces may be
                     * down there. A line that put a node on itself hands over
                     * nothing. */
                    if !node_start {
                        owner_column = None;
                    }
                    node_start = true;
                    token_column = None;
                    if owner_column.is_none() {
                        self.add_safe_checkpoint(index);
                    }
                }
                b' ' | b'\t' => {
                    index += 1;
                    separated = true;
                }
                b'|' | b'>'
                    if node_start && separated && yaml_block_header(bytes, index).is_some() =>
                {
                    let (indicator, chomping, comment, header_end) =
                        yaml_block_header(bytes, index).expect("the guard read the header");
                    if let Some(start) = comment {
                        self.add_comment(start, header_end, CommentKind::Line);
                    }
                    /* NOTE: The body is indented past the node the scalar hangs off
                     * (8.1.1.1). For `key: |` that node is the mapping, whose
                     * indentation is the column of the key; for `- |` it is the
                     * sequence, whose indentation is the column of the `-`. The
                     * header itself may sit anywhere past that owner — on a
                     * line of its own, or behind an anchor or a tag — so its
                     * own column says nothing about how deep a body line has to
                     * be, and reading it as the floor would take a body
                     * indented less than the header for the end of the scalar
                     * and its `#` lines for comments. With no owner at all the
                     * scalar is the whole document, whose indentation is one
                     * short of column zero, which leaves every line under it
                     * body. An explicit indentation indicator counts from that
                     * same owner, which is why it replaces the detected depth
                     * rather than adding to it. Detection proper reads the
                     * first non-empty line instead, and a line shallower than
                     * that but still past the owner is content of neither
                     * reading; taking it for body is the one that leaves bytes
                     * alone. */
                    let base = owner_column.map_or(0, |column| column + 1);
                    let floor = base + indicator.unwrap_or(1) - 1;
                    let (end, boundary, detected) = yaml_block_body_end(bytes, header_end, floor);
                    /* NOTE: Where this body stopped, on what terms it keeps its
                     * trailing empty lines, and how deep its content is are the
                     * whole of what `lines_a_removal_must_swallow` and
                     * `yaml_structural_trail_keeps` need from a scan: the lines
                     * under a body are the only place in YAML where the hole a
                     * removal leaves carries meaning. Recorded here rather than
                     * re-derived, because only the scan knows the column of the
                     * node the header hangs off. An explicit indicator *is* the
                     * content depth (8.1.1.1); without one the depth is
                     * detected from the first non-empty line, and a body with
                     * no non-empty line at all has none to detect, so the floor
                     * stands in for it — which is the depth the next line the
                     * scalar could take would set. */
                    self.yaml_blocks.push(YamlBlockScalar {
                        body_end: end + self.offset,
                        content_indent: indicator.map_or(detected, |_| floor),
                        chomping,
                    });
                    index = end;
                    line_start = index;
                    separated = true;
                    node_start = true;
                    token_column = None;
                    owner_column = None;
                    if boundary {
                        self.add_safe_checkpoint(index);
                    }
                }
                /* NOTE: An anchor `&name` and a tag `!tag` are node properties
                 * (6.9): they stand in front of the node they decorate rather
                 * than being one, so a node may still begin after them. That is
                 * what leaves the `|` of `key: !!str |` a block scalar header
                 * instead of a byte of a plain scalar. A property belongs to
                 * the node it decorates, so it is that node's first token and
                 * names the column a `: ` behind it hangs off. */
                b'!' | b'&' if node_start && separated => {
                    if token_column.is_none() {
                        token_column = Some(index - line_start);
                    }
                    index = yaml_property_end(bytes, index);
                    separated = false;
                }
                b'"' | b'\'' if separated || yaml_flow_opener(bytes, index) => {
                    if node_start && token_column.is_none() {
                        token_column = Some(index - line_start);
                    }
                    index = self.scan_yaml_quoted(index);
                    separated = false;
                    node_start = false;
                }
                /* NOTE: `-` is a sequence entry and `?` an explicit key only when
                 * white space or the line ends them (6.9 and 8.2): `-x` is a
                 * plain scalar, and so is `?x`. Either leaves the position a
                 * node may begin at, one column further in. */
                b'-' | b'?'
                    if node_start
                        && bytes
                            .get(index + 1)
                            .is_none_or(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n')) =>
                {
                    owner_column = Some(index - line_start);
                    token_column = None;
                    index += 1;
                    separated = false;
                }
                /* NOTE: A `:` ends a key only where white space or the line follows
                 * it (7.2), which is what leaves the `:` of `http://x` inside
                 * the plain scalar it belongs to. */
                b':' if bytes
                    .get(index + 1)
                    .is_none_or(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n')) =>
                {
                    owner_column = token_column.or(owner_column);
                    token_column = None;
                    node_start = true;
                    index += 1;
                    separated = false;
                }
                _ => {
                    if node_start {
                        if token_column.is_none() {
                            token_column = Some(index - line_start);
                        }
                        node_start = false;
                    }
                    index += 1;
                    separated = false;
                }
            }
        }
        /* NOTE: One rule needs the whole file rather than the byte in front of
         * it, so it runs once the trails are all there to read: a comment that
         * is the only thing holding a block scalar out of the kept comment
         * under it is not commentary and is kept. A scan that stopped early is
         * a partial answer the incremental engine completes from the previous
         * revision's tail, and the trail it would read is truncated, so it is
         * left alone — no checkpoint a YAML scan offers sits past the first
         * block scalar, which is what leaves the tail's own answer intact. */
        if !self.stopped && !self.yaml_blocks.is_empty() {
            let keeps = yaml_structural_trail_keeps(
                self.source,
                self.offset,
                &self.yaml_blocks,
                &self.comments,
            );
            for index in keeps {
                self.comments[index].disposition = Disposition::Keep {
                    reason: YAML_STRUCTURAL_TRAIL.to_owned(),
                };
            }
        }
    }

    /// One quoted scalar beginning at its own quote.
    ///
    /// A double-quoted scalar takes `\` escapes (YAML 1.2.2, 7.3.1) and a
    /// single-quoted one takes none, where `''` is the one way to write a
    /// quote of its own (7.3.2), so a backslash inside the second is a byte of
    /// it. Both fold over a line break, which makes the end of the file the
    /// only thing that leaves one unterminated.
    fn scan_yaml_quoted(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let quote = bytes[start];
        let mut index = start + 1;
        while index < bytes.len() {
            if quote == b'"' && bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
            } else if bytes[index] != quote {
                index += 1;
            } else if quote == b'\'' && bytes.get(index + 1) == Some(&b'\'') {
                index += 2;
            } else {
                return index + 1;
            }
        }
        self.error(
            "unterminated-string",
            if quote == b'"' {
                "unterminated YAML double-quoted scalar"
            } else {
                "unterminated YAML single-quoted scalar"
            },
            ByteSpan::new(start, index),
        );
        index
    }

    /// One PHP file (PHP manual, Language Reference: Basic syntax, Comments,
    /// Strings, and Heredoc text).
    ///
    /// A PHP file is two languages at once. It opens in inline-HTML mode,
    /// where every byte is output verbatim and nothing is a comment; `<?php`
    /// with white space or the end of the file behind it, and the short echo
    /// tag `<?=`, enter PHP mode, and `?>` returns to inline HTML. With the
    /// default `short_open_tag=Off` a bare `<?` opens nothing at all, which is
    /// what leaves an XML declaration inline text.
    ///
    /// Inline HTML is opaque in v1: an HTML `<!-- ... -->` comment in a PHP
    /// file is not reported. Reading it would mean scanning the inline halves
    /// as HTML, which is a change of what the language *is* rather than a
    /// missing arm here, so v1 leaves those bytes alone — the direction that
    /// can only keep a comment, never remove one.
    ///
    /// Which mode a byte sits in is decided entirely by the bytes in front of
    /// it, and no lexical state PHP opens is ended by anything except its own
    /// closer, so a restart in inline HTML reproduces the rest of a full scan.
    /// That is the only place a checkpoint is offered: a line break met in PHP
    /// mode is not a restart point, because the same line at the same offset
    /// means something else with an unclosed `<?php` above it. A file that is
    /// all PHP therefore rescans from the top.
    fn scan_php(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        /* NOTE: The CLI strips a `#!` line from the first line of a script before
         * the engine sees it (`php_cli.c`, which tests the first two bytes), so
         * that line is a preamble rather than the inline HTML the rest of the
         * file opens as. Unlike CPython and Lua, PHP skips no byte order mark
         * first, and neither does the kernel, so a mark in front of the `#!`
         * leaves it ordinary inline HTML — the same reason a shell script has.
         * The `self.offset` test is what keeps a suffix scan out of the rule,
         * and no checkpoint of a full scan falls inside the first line, so the
         * two answers cannot disagree. */
        if self.offset == 0 && starts(bytes, 0, b"#!") {
            let end = line_end(bytes, 2);
            self.add_comment(0, end, CommentKind::Line);
            index = end;
        }
        while index < bytes.len() && !self.stopped {
            match bytes[index] {
                b'<' => match php_open_tag(bytes, index) {
                    Some(code) => index = self.scan_php_code(code),
                    None => index += 1,
                },
                b'\r' | b'\n' => {
                    index = consume_newline(bytes, index);
                    self.add_safe_checkpoint(index);
                }
                _ => index += 1,
            }
        }
    }

    /// PHP mode, from the byte after the opening tag that entered it.
    ///
    /// Returns where inline HTML resumes: past a `?>` and the one line break
    /// it carries away with it, or the end of the file.
    fn scan_php_code(&mut self, mut index: usize) -> usize {
        let bytes = self.source;
        while index < bytes.len() {
            match bytes[index] {
                b'?' if starts(bytes, index, b"?>") => {
                    let end = index + 2;
                    /* NOTE: The closing tag token carries one line break with it
                     * (`zend_language_scanner.l`: `"?>"{NEWLINE}?`), which is
                     * what keeps a template from emitting a blank line for
                     * every block of code it holds. A CRLF pair is that one
                     * break. The byte after it starts a line of inline HTML,
                     * so it is a restart point like any other line start. */
                    if matches!(bytes.get(end), Some(b'\r' | b'\n')) {
                        let next = consume_newline(bytes, end);
                        self.add_safe_checkpoint(next);
                        return next;
                    }
                    return end;
                }
                b'/' if starts(bytes, index, b"//") => {
                    let end = php_line_comment_end(bytes, index + 2);
                    self.add_comment(index, end, CommentKind::Line);
                    index = end;
                }
                b'/' if starts(bytes, index, b"/*") => {
                    let (end, closed) = block_end(bytes, index, b"/*", b"*/", false);
                    self.add_comment(index, end, php_block_kind(bytes, index));
                    if !closed {
                        self.error(
                            "unterminated-comment",
                            "unterminated PHP block comment",
                            ByteSpan::new(index, end),
                        );
                    }
                    index = end;
                }
                /* NOTE: PHP 8.0 gave `#[` to attributes (Attributes, Attribute
                 * syntax), so a `#` with a bracket behind it opens no comment
                 * and what follows is ordinary code. */
                b'#' if bytes.get(index + 1) == Some(&b'[') => index += 1,
                b'#' => {
                    let end = php_line_comment_end(bytes, index + 1);
                    self.add_comment(index, end, CommentKind::Line);
                    index = end;
                }
                b'\'' | b'"' | b'`' => index = self.scan_php_quoted(index),
                b'<' if starts(bytes, index, b"<<<") => index = self.scan_php_heredoc(index),
                _ => index += 1,
            }
        }
        index
    }

    /// One PHP string beginning at its delimiter: `'`, `"`, or the backtick of
    /// the execution operator.
    ///
    /// A single-quoted string escapes only `\'` and `\\`, and every other
    /// backslash is a byte of it — but the byte after a backslash can never be
    /// the closing quote unless the pair *is* the `\'` escape, so skipping two
    /// finds the same closer either way. A double-quoted or backtick string
    /// takes the full escape set and interpolates: `{$...}` and `${...}` hold
    /// an expression, which [`php_interpolation_end`] skips by balancing
    /// braces, so a comment written inside one stays content. None of the
    /// three ends at a line break, so only the end of the file leaves one
    /// unterminated.
    fn scan_php_quoted(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let quote = bytes[start];
        let interpolates = quote != b'\'';
        let mut index = start + 1;
        while index < bytes.len() {
            if bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
            } else if bytes[index] == quote {
                return index + 1;
            } else if interpolates && bytes[index] == b'{' && bytes.get(index + 1) == Some(&b'$') {
                index = php_interpolation_end(bytes, index);
            } else if interpolates && bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'{') {
                index = php_interpolation_end(bytes, index + 1);
            } else {
                index += 1;
            }
        }
        self.error(
            "unterminated-string",
            match quote {
                b'\'' => "unterminated PHP single-quoted string",
                b'"' => "unterminated PHP double-quoted string",
                _ => "unterminated PHP backtick string",
            },
            ByteSpan::new(start, index),
        );
        index
    }

    /// One heredoc or nowdoc beginning at its `<<<`, or `start + 1` when those
    /// three bytes head no header at all — `$a <<< 1` is two shift operators
    /// and a number, and the conservative reading of anything the header
    /// grammar refuses is that it opened nothing.
    fn scan_php_heredoc(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let Some((label, body, nowdoc)) = php_heredoc_header(bytes, start) else {
            return start + 1;
        };
        if let Some(end) = php_heredoc_end(bytes, body, label) {
            return end;
        }
        self.error(
            "unterminated-string",
            if nowdoc {
                "unterminated PHP nowdoc"
            } else {
                "unterminated PHP heredoc"
            },
            ByteSpan::new(start, bytes.len()),
        );
        bytes.len()
    }

    /// Ruby, from the first byte of the file.
    ///
    /// The whole scanner is one state machine over three states — see
    /// [`RubyState`] — because four of Ruby's tokens are spelled with a byte
    /// that is also an operator, and only where the token stands decides
    /// which: `/` is a regular expression or a division, `%` a literal or a
    /// modulo, `?` a character literal or a ternary, and `<<` a here document
    /// or a shift. Ruby's own lexer answers those four questions from its
    /// `lex_state`, and these three states are that variable folded down to
    /// what the four questions actually read out of it.
    fn scan_ruby(&mut self) {
        let mut pending = Vec::new();
        let _ = self.scan_ruby_code(0, false, 0, &mut pending);
    }

    /// Ruby code from `index`, to the end of the file — or, when
    /// `interpolation` is set, to the `}` that balances the `#{` the caller
    /// has just consumed. Returns where it stopped.
    ///
    /// `pending` is the here documents opened on the physical line being read
    /// and not yet given a body, in the order Ruby will consume them. It is one
    /// list for the whole line rather than one per nested scan because a header
    /// may stand inside an interpolation — `puts "#{ <<EOS }"` opens a here
    /// document whose body is the line *under* that one — and because Ruby
    /// takes the bodies in header order across the whole line, so an opener
    /// written before an interpolation and one written inside it queue
    /// together. The list is drained by whichever scan reaches the line break
    /// first, which is why it has to outlive the `}` this call returns from.
    fn scan_ruby_code(
        &mut self,
        mut index: usize,
        interpolation: bool,
        depth: usize,
        pending: &mut Vec<RubyHeredoc>,
    ) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "Ruby lexical nesting limit exceeded",
                ByteSpan::new(index, index),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let offset = self.offset;
        let mut state = RubyState::Begin;
        let mut space_seen = false;
        let mut braces = usize::from(interpolation);
        /* NOTE: Where this call's own openers begin in the shared list. An
         * enclosing scan's entries sit in front of them and are that scan's to
         * report, which is what keeps one unterminated here document to one
         * diagnostic. */
        let base = pending.len();
        while index < bytes.len() && !self.stopped {
            match bytes[index] {
                b'#' => {
                    let end = line_end(bytes, index + 1);
                    self.add_comment(index, end, CommentKind::Line);
                    index = end;
                }
                b'=' if ruby_at_line_start(bytes, index, offset)
                    && ruby_embedded_document(bytes, index) =>
                {
                    let (end, closed) = ruby_embedded_document_end(bytes, index);
                    self.add_comment(index, end, CommentKind::Block);
                    if !closed {
                        self.error(
                            "unterminated-comment",
                            "unterminated Ruby embedded document",
                            ByteSpan::new(index, end),
                        );
                    }
                    index = end;
                    state = RubyState::Begin;
                    space_seen = false;
                }
                /* NOTE: Everything past the marker is the DATA section, which is
                 * not source and holds no comments. */
                b'_' if ruby_at_line_start(bytes, index, offset)
                    && ruby_data_marker(bytes, index) =>
                {
                    index = bytes.len();
                }
                b'\r' | b'\n' => {
                    index = consume_newline(bytes, index);
                    if !pending.is_empty() {
                        let opened = std::mem::take(pending);
                        match self.scan_ruby_heredoc_bodies(index, opened, depth + 1, pending) {
                            Some(end) => index = end,
                            None => {
                                index = bytes.len();
                                continue;
                            }
                        }
                    }
                    state = RubyState::Begin;
                    space_seen = false;
                    /* NOTE: The queue is drained above before a checkpoint is
                     * offered, so a restart here never lands inside a body a
                     * header on an earlier line asked for. The emptiness test
                     * says so locally rather than leaving it to that argument. */
                    if !interpolation && depth == 0 && pending.is_empty() {
                        self.add_safe_checkpoint(index);
                    }
                }
                b'\'' => {
                    index = self.scan_ruby_string(index, false, depth, pending);
                    state = RubyState::End;
                    space_seen = false;
                }
                b'"' | b'`' => {
                    index = self.scan_ruby_string(index, true, depth, pending);
                    state = RubyState::End;
                    space_seen = false;
                }
                b':' if starts(bytes, index, b"::") => {
                    index += 2;
                    state = RubyState::End;
                    space_seen = false;
                }
                b':' if matches!(bytes.get(index + 1), Some(b'\'' | b'"')) => {
                    index =
                        self.scan_ruby_string(index + 1, bytes[index + 1] == b'"', depth, pending);
                    state = RubyState::End;
                    space_seen = false;
                }
                b':' if bytes
                    .get(index + 1)
                    .is_some_and(|byte| ruby_symbol_head(*byte)) =>
                {
                    index = ruby_symbol_end(bytes, index);
                    state = RubyState::End;
                    space_seen = false;
                }
                b'?' => {
                    match ruby_character_literal_end(bytes, index) {
                        Some(end) if !matches!(state, RubyState::End | RubyState::Fname) => {
                            index = end;
                            state = RubyState::End;
                        }
                        _ => {
                            index += 1;
                            state = RubyState::Begin;
                        }
                    }
                    space_seen = false;
                }
                b'%' => {
                    match ruby_percent_header(bytes, index) {
                        Some(literal)
                            if ruby_percent_opens(
                                state,
                                space_seen,
                                bytes,
                                index,
                                literal.form,
                            ) =>
                        {
                            /* NOTE: `alias` and `undef` hold `EXPR_FNAME|EXPR_FITEM`
                             * across the whole statement rather than only up to
                             * the first name: `Ripper.lex` under Ruby 3.3.12
                             * reports that state again after the `)` of the
                             * first symbol, which is what makes
                             * `alias%s(a)%s(b # c)` two symbols and not one
                             * symbol and a modulo. */
                            let fitem = state == RubyState::Fname && literal.form == b's';
                            index = self.scan_ruby_percent(index, &literal, depth, pending);
                            state = if fitem {
                                RubyState::Fname
                            } else {
                                RubyState::End
                            };
                        }
                        _ => {
                            index += 1;
                            state = RubyState::Begin;
                        }
                    }
                    space_seen = false;
                }
                b'/' => {
                    if ruby_literal_opens(state, space_seen, bytes, index) {
                        index = self.scan_ruby_regexp(index, depth, pending);
                        state = RubyState::End;
                    } else {
                        index += 1;
                        state = RubyState::Begin;
                    }
                    space_seen = false;
                }
                b'<' if starts(bytes, index, b"<<") => {
                    match ruby_heredoc_header(bytes, index) {
                        Some((heredoc, end)) if ruby_heredoc_may_open(state, space_seen) => {
                            pending.push(heredoc);
                            index = end;
                            state = RubyState::End;
                        }
                        _ => {
                            index += 2;
                            state = RubyState::Begin;
                        }
                    }
                    space_seen = false;
                }
                b'$' => {
                    index = ruby_global_end(bytes, index);
                    state = RubyState::End;
                    space_seen = false;
                }
                b'@' => {
                    index = ruby_at_variable_end(bytes, index);
                    state = RubyState::End;
                    space_seen = false;
                }
                b'{' => {
                    braces += 1;
                    index += 1;
                    state = RubyState::Begin;
                    space_seen = false;
                }
                b'}' => {
                    index += 1;
                    if interpolation {
                        braces -= 1;
                        if braces == 0 {
                            return index;
                        }
                    }
                    state = RubyState::End;
                    space_seen = false;
                }
                b'(' | b'[' => {
                    index += 1;
                    state = RubyState::Begin;
                    space_seen = false;
                }
                b')' | b']' => {
                    index += 1;
                    state = RubyState::End;
                    space_seen = false;
                }
                /* NOTE: The method-call dot, which is also the two range
                 * operators. All three want the byte after them read as a name
                 * rather than as a literal delimiter, which is what `End` says. */
                b'.' => {
                    index += 1;
                    state = RubyState::End;
                    space_seen = false;
                }
                /* NOTE: Outside a literal a backslash only continues the line,
                 * which is white space to the grammar. No checkpoint is emitted
                 * for the break it swallows, because the statement runs on past
                 * it and a scan restarted there would not know that. */
                b'\\' if matches!(bytes.get(index + 1), Some(b'\r' | b'\n')) => {
                    index = consume_newline(bytes, index + 1);
                    space_seen = true;
                }
                b'\\' => {
                    index = (index + 2).min(bytes.len());
                    state = RubyState::End;
                    space_seen = false;
                }
                byte if byte.is_ascii_digit() => {
                    index = ruby_number_end(bytes, index);
                    state = RubyState::End;
                    space_seen = false;
                }
                byte if ruby_identifier_start(byte) => {
                    let start = index;
                    index = ruby_word_end(bytes, index);
                    state = ruby_state_after_word(&bytes[start..index]);
                    space_seen = false;
                }
                byte if ruby_is_space(byte) => {
                    index += 1;
                    space_seen = true;
                }
                _ => {
                    index += 1;
                    state = RubyState::Begin;
                    space_seen = false;
                }
            }
        }
        /* NOTE: A here document opened on a last line that has no break of its
         * own never reaches [`Self::scan_ruby_heredoc_bodies`], because that is
         * driven from the break. It is unterminated all the same, and is
         * reported from its own `<<` with the span that call would have given
         * it. Only this call's own openers are reported here — the ones from
         * `base` on — because an enclosing scan reports its own, and the list is
         * cut back to `base` so that it reports them once. Nothing is left to
         * report whenever the loop stopped at a checkpoint instead, because a
         * break empties the list before one is offered. */
        if let Some(operator) = pending.get(base).map(|heredoc| heredoc.operator) {
            self.error(
                "unterminated-heredoc",
                "unterminated Ruby here document",
                ByteSpan::new(operator, bytes.len()),
            );
            pending.truncate(base);
        }
        if interpolation {
            self.error(
                "unterminated-interpolation",
                "unterminated Ruby interpolation",
                ByteSpan::new(index, index),
            );
        }
        index
    }

    /// One Ruby string, from its delimiter: `'`, `"`, or the backtick of a
    /// command literal.
    ///
    /// A single-quoted string escapes only `\'` and `\\`, and every other
    /// backslash is a byte of it — but the byte after a backslash can never be
    /// the closing quote unless the pair *is* the `\'` escape, so skipping two
    /// finds the same closer either way. The other two take the full escape set
    /// and interpolate, and `#{ ... }` holds an expression, so a comment
    /// written inside one is a comment. None of the three ends at a line break,
    /// so only the end of the file leaves one unterminated.
    fn scan_ruby_string(
        &mut self,
        start: usize,
        interpolates: bool,
        depth: usize,
        pending: &mut Vec<RubyHeredoc>,
    ) -> usize {
        let bytes = self.source;
        let quote = bytes[start];
        let mut index = start + 1;
        while index < bytes.len() {
            if bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
            } else if bytes[index] == quote {
                return index + 1;
            } else if interpolates && starts(bytes, index, b"#{") {
                index = self.scan_ruby_code(index + 2, true, depth + 1, pending);
            } else {
                index += 1;
            }
        }
        self.error(
            "unterminated-string",
            match quote {
                b'\'' => "unterminated Ruby single-quoted string",
                b'"' => "unterminated Ruby double-quoted string",
                _ => "unterminated Ruby backtick string",
            },
            ByteSpan::new(start, index),
        );
        index
    }

    /// One `/ ... /` regular expression, from its opening slash.
    ///
    /// A `[` opens a character class, where the delimiter is one of the
    /// pattern's own bytes. That is deliberately more forgiving than Ruby's own
    /// `tokadd_string`, which ends the literal at the first unescaped `/`
    /// wherever it stands: reading `/[/]/` as one literal keeps the rest of the
    /// line inside it, which hides bytes from a removal rather than exposing
    /// them, and it is the reading a person writing the pattern meant. A
    /// pattern interpolates and may span lines, so only the end of the file
    /// leaves one unterminated.
    fn scan_ruby_regexp(
        &mut self,
        start: usize,
        depth: usize,
        pending: &mut Vec<RubyHeredoc>,
    ) -> usize {
        let bytes = self.source;
        let mut index = start + 1;
        let mut in_class = false;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index = (index + 2).min(bytes.len()),
                b'[' => {
                    in_class = true;
                    index += 1;
                }
                b']' => {
                    in_class = false;
                    index += 1;
                }
                b'/' if !in_class => return ruby_regexp_flags_end(bytes, index + 1),
                b'#' if starts(bytes, index, b"#{") => {
                    index = self.scan_ruby_code(index + 2, true, depth + 1, pending);
                }
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-string",
            "unterminated Ruby regular expression",
            ByteSpan::new(start, index),
        );
        index
    }

    /// One `%` literal, from the `%` itself, with the header
    /// [`ruby_percent_header`] read out of it.
    ///
    /// A paired delimiter nests, which is what lets `%w[a [b] c]` hold a
    /// bracket; every other delimiter closes with itself and cannot. The
    /// interpolating forms read `#{ ... }` as an expression before either
    /// delimiter is considered, so the braces of one never count towards a
    /// `%Q{...}` nesting depth.
    fn scan_ruby_percent(
        &mut self,
        start: usize,
        literal: &RubyPercent,
        depth: usize,
        pending: &mut Vec<RubyHeredoc>,
    ) -> usize {
        let bytes = self.source;
        let mut index = literal.content;
        let mut nesting = 1usize;
        while index < bytes.len() {
            if bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
                continue;
            }
            if literal.interpolates && starts(bytes, index, b"#{") {
                index = self.scan_ruby_code(index + 2, true, depth + 1, pending);
                continue;
            }
            if literal.open != literal.close && bytes[index] == literal.open {
                nesting += 1;
                index += 1;
                continue;
            }
            if bytes[index] == literal.close {
                nesting -= 1;
                index += 1;
                if nesting == 0 {
                    return if literal.form == b'r' {
                        ruby_regexp_flags_end(bytes, index)
                    } else {
                        index
                    };
                }
                continue;
            }
            index += 1;
        }
        self.error(
            "unterminated-string",
            "unterminated Ruby percent literal",
            ByteSpan::new(start, index),
        );
        index
    }

    /// Every here document opened on the line that has just ended, in the order
    /// they were opened, from the first byte of the line under it.
    ///
    /// `None` once one of them runs out of file, which is reported from the
    /// `<<` that opened it rather than from the line it swallowed. `pending` is
    /// the shared queue the caller has just emptied into `heredocs`; a body line
    /// that opens another here document fills it again, and this call reads that
    /// one from under that body line before the body around it resumes.
    fn scan_ruby_heredoc_bodies(
        &mut self,
        mut index: usize,
        heredocs: Vec<RubyHeredoc>,
        depth: usize,
        pending: &mut Vec<RubyHeredoc>,
    ) -> Option<usize> {
        for heredoc in heredocs {
            match self.scan_ruby_heredoc_body(index, &heredoc, depth, pending) {
                Some(end) => index = end,
                None => {
                    self.error(
                        "unterminated-heredoc",
                        "unterminated Ruby here document",
                        ByteSpan::new(heredoc.operator, self.source.len()),
                    );
                    return None;
                }
            }
        }
        Some(index)
    }

    /// The body of one here document, from the first byte of a line of it.
    ///
    /// Returns the offset just past the terminator line. The body is opaque:
    /// only `#{ ... }` in an interpolating form is read as code, and the
    /// terminator is looked for at the start of each of the body's own lines,
    /// so one written inside an interpolation is content like the rest of it.
    ///
    /// A body line is a physical line, so a here document header reached
    /// through one of those interpolations queues a body for the line beneath
    /// *it* — Ruby 3.3.12 reads `puts <<A` / `x #{<<B}` / `A` / `B` with `A` as
    /// B's body and not as A's terminator — and this loop drains the queue at
    /// each line break before looking for its own terminator again.
    fn scan_ruby_heredoc_body(
        &mut self,
        mut index: usize,
        heredoc: &RubyHeredoc,
        depth: usize,
        pending: &mut Vec<RubyHeredoc>,
    ) -> Option<usize> {
        let bytes = self.source;
        loop {
            if index >= bytes.len() {
                return None;
            }
            if ruby_heredoc_terminates(bytes, index, heredoc) {
                return Some(consume_newline(bytes, line_end(bytes, index)).min(bytes.len()));
            }
            while index < bytes.len() && !matches!(bytes[index], b'\r' | b'\n') {
                if heredoc.interpolates && bytes[index] == b'\\' {
                    index = if starts(bytes, index, b"\\\r\n") {
                        index + 3
                    } else {
                        (index + 2).min(bytes.len())
                    };
                } else if heredoc.interpolates && starts(bytes, index, b"#{") {
                    index = self.scan_ruby_code(index + 2, true, depth + 1, pending);
                } else {
                    index += 1;
                }
            }
            if index >= bytes.len() {
                return None;
            }
            index = consume_newline(bytes, index);
            /* NOTE: A body line is a physical line like any other, so a header
             * reached through an interpolation on it queues for the line under
             * it and is read there — before this body resumes. */
            if !pending.is_empty() {
                let opened = std::mem::take(pending);
                index = self.scan_ruby_heredoc_bodies(index, opened, depth + 1, pending)?;
            }
        }
    }

    /// One Zig source file (Zig Language Reference: Comments, Doc comments,
    /// String Literals).
    ///
    /// Zig has no block comment at all — `/*` is the division operator and
    /// then multiplication, which `std.zig.Tokenizer` reports as `slash` and
    /// `asterisk` — so this is its own small lexer rather than a
    /// [`Self::scan_c_family`] with one delimiter taken away. Everything it has
    /// to know ends at a line break: a comment runs to the end of its line, a
    /// quoted literal may not cross one, and a multiline string literal is one
    /// line of content at a time. That is what makes every line start a
    /// restart point with nothing to carry across it.
    fn scan_zig(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        while index < bytes.len() && !self.stopped {
            if starts(bytes, index, b"//") {
                let end = line_end(bytes, index + 2);
                self.add_comment(index, end, zig_line_kind(bytes, index));
                index = end;
                continue;
            }
            /* NOTE: `\\` is the whole opener of a multiline string literal line,
             * and the tokenizer takes it wherever a token may begin rather
             * than only as the first thing on a line: `const b = \\text` is
             * one `multiline_string_literal_line` to `std.zig.Tokenizer` just
             * as an indented `\\` is. Everything to the end of the line is
             * content, and the next line starts in code again, so consecutive
             * lines are separate tokens that the parser joins. A single `\` is
             * an invalid token to Zig and an ordinary byte here: nothing it
             * could open is a state a comment can hide in. */
            if starts(bytes, index, b"\\\\") {
                index = line_end(bytes, index + 2);
                continue;
            }
            match bytes[index] {
                b'"' | b'\'' => index = self.scan_zig_quoted(index),
                b'\r' | b'\n' => {
                    index = consume_newline(bytes, index);
                    self.add_safe_checkpoint(index);
                }
                _ => index += 1,
            }
        }
    }

    /// One Zig string or character literal beginning at its quote.
    ///
    /// The two are one rule: `.string_literal` and `.char_literal` of
    /// `std.zig.Tokenizer` differ only in the quote that closes them. A `\`
    /// carries the next byte into the literal, and a real line terminator ends
    /// neither — the tokenizer marks the token `invalid` at it — so a quote
    /// that never closes is reported at the line break rather than swallowing
    /// the lines below it. A `\` in front of that terminator does not carry it
    /// either, for the same reason.
    ///
    /// `@"quoted identifier"` needs no rule of its own: the `@` is an ordinary
    /// byte and the identifier that follows it is lexed as the string literal
    /// it is spelled as, which is what hides a `//` written inside one.
    fn scan_zig_quoted(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let quote = bytes[start];
        let message = if quote == b'"' {
            "unterminated Zig string"
        } else {
            "unterminated Zig character literal"
        };
        let mut index = start + 1;
        while index < bytes.len() {
            if bytes[index] == b'\\' && !matches!(bytes.get(index + 1), None | Some(b'\r' | b'\n'))
            {
                index += 2;
            } else if bytes[index] == quote {
                return index + 1;
            } else if matches!(bytes[index], b'\r' | b'\n') {
                self.error("unterminated-string", message, ByteSpan::new(start, index));
                return index;
            } else {
                index += 1;
            }
        }
        self.error("unterminated-string", message, ByteSpan::new(start, index));
        index
    }

    /// One R script (R Language Definition, 10 Parser; `?Quotes`).
    ///
    /// `#` opens a comment that runs to the end of the line and that is the
    /// whole comment grammar — there is no block form and no nesting. What
    /// makes the scanner more than a search for `#` is the four literals that
    /// carry one as content: a quoted string, a raw string, a backquoted name,
    /// and the `%...%` operator.
    ///
    /// Three of the four may cross a line break, so the start of a line is a
    /// restart point only when the scan reaches it here, at the top level. The
    /// fourth may not: `SpecialValue` in `gram.y` returns `ERROR` at a newline,
    /// which is why an unterminated `%` is reported where it is rather than
    /// swallowing the rest of the file.
    ///
    /// Measured against the interpreter over the 42 `.R` files the R 4.3.3
    /// distribution ships — its `demo/`, `doc/` and `share/R/` scripts — every
    /// one of the 1,330 comments `utils::getParseData` reports as a `COMMENT`
    /// token comes back here with the same byte span, and no file is called
    /// invalid.
    fn scan_r(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        while index < bytes.len() && !self.stopped {
            match bytes[index] {
                b'#' => {
                    let end = line_end(bytes, index + 1);
                    self.add_comment(index, end, r_line_kind(bytes, index));
                    index = end;
                }
                b'"' | b'\'' => index = self.scan_r_string(index),
                b'`' => index = self.scan_r_name(index),
                b'%' => index = self.scan_r_operator(index),
                b'\r' | b'\n' => {
                    index = consume_newline(bytes, index);
                    self.add_safe_checkpoint(index);
                }
                _ => index += 1,
            }
        }
    }

    /// One R string beginning at its quote, raw or not.
    ///
    /// A raw string is the quote behind an `r` or an `R` that begins a token
    /// ([`r_raw_string`]); everything else is the ordinary form, which takes
    /// `\` escapes and carries a line break as content, so only the matching
    /// quote or the end of the file ends one.
    ///
    /// Where the bytes look like a raw string and are not one — `r"<a>"`, whose
    /// delimiter is not a bracket — R refuses the file outright with `malformed
    /// raw string literal`. Falling back to the ordinary reading is what this
    /// does instead: it is the same fallback [`cpp_raw_string`] takes, and it
    /// hides the `#` behind a quote rather than exposing it in a file no
    /// interpreter would run.
    fn scan_r_string(&mut self, start: usize) -> usize {
        let bytes = self.source;
        if let Some((end, closed)) = r_raw_string(bytes, start) {
            if !closed {
                self.error(
                    "unterminated-string",
                    "unterminated R raw string",
                    ByteSpan::new(start - 1, end),
                );
            }
            return end;
        }
        let (end, closed) = r_delimited_end(bytes, start + 1, bytes[start]);
        if !closed {
            self.error(
                "unterminated-string",
                "unterminated R string",
                ByteSpan::new(start, end),
            );
        }
        end
    }

    /// One backquoted name beginning at its backquote.
    ///
    /// It is a quoted string in every lexical respect: `\` carries the next
    /// byte into it, a line break is content, and only the closing backquote
    /// ends it. What it is not is a string constant, so an unterminated one is
    /// reported as the identifier it was going to be.
    fn scan_r_name(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let (end, closed) = r_delimited_end(bytes, start + 1, b'`');
        if !closed {
            self.error(
                "unterminated-identifier",
                "unterminated R backquoted name",
                ByteSpan::new(start, end),
            );
        }
        end
    }

    /// One `%...%` operator beginning at its first `%`.
    ///
    /// `SpecialValue` (`gram.y`) pushes every byte up to the next `%` into the
    /// operator's name and returns `ERROR` when a line break arrives first, so
    /// the name may hold a `#`, a quote or a backquote, takes no escapes, and
    /// cannot cross a line. `%%` and `%in%` are the same rule with nothing
    /// interesting inside them.
    fn scan_r_operator(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let stop = line_end(bytes, start + 1);
        match memchr(b'%', &bytes[start + 1..stop]) {
            Some(relative) => start + relative + 2,
            None => {
                self.error(
                    "unterminated-operator",
                    "unterminated R special operator",
                    ByteSpan::new(start, stop),
                );
                stop
            }
        }
    }

    /// One Dart compilation unit (Dart Language Specification, 17.1 Comments
    /// and 17.6 Strings).
    ///
    /// Dart is a C-family syntax with three departures that decide the shape of
    /// this scanner rather than of [`Self::scan_c_family`]:
    ///
    /// * its block comment *nests*, so `/* /* */ */` is one comment;
    /// * `//!` and `/*!` document nothing — Dart's only markers are `///` and
    ///   `/**` — while `////` still does ([`dart_line_kind`]); and
    /// * `#!` at the very first byte is the script tag, and `#` is the
    ///   symbol-literal operator everywhere else.
    ///
    /// A string is the one construct that hides a comment opener, and Dart
    /// writes six of them: either quote, single-line or triple-quoted, raw or
    /// not. A raw one is the quote behind an `r`, and only where that `r`
    /// begins a token ([`dart_raw_string_prefix`]).
    ///
    /// The start of a line is a restart point only when the scan reaches it
    /// here, at the top level: a triple-quoted string, a nested block comment,
    /// and an interpolation with a comment inside it all carry a line break.
    ///
    /// Ground truth for every rule below is `scanString` of
    /// `package:_fe_analyzer_shared` as the Dart SDK 3.13.2 ships it, read for
    /// token kinds and offsets, with `dart analyze` for acceptance.
    ///
    /// Measured against that scanner over the 3,143 `.dart` files of the SDK's
    /// own `lib/` and of the packages `dart pub get` fetched beside it: all
    /// 147,988 comments it reports — its comment stream plus the `SCRIPT_TAG`
    /// token, with its UTF-16 offsets mapped back to bytes — come back here
    /// with the same byte span, and no file is called invalid.
    fn scan_dart(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        while index < bytes.len() && !self.stopped {
            /* NOTE: `tokenizeTag` reads a `#!` line only when `scanOffset == 0`, and
             * a comment on the line above one is enough to take that away, so
             * this is the first byte of the document and no other. `#`
             * elsewhere is the operator that opens a symbol literal — `#foo`,
             * `#+` — which needs no rule of its own because nothing it can be
             * followed by hides a comment. */
            if index == 0 && self.offset == 0 && starts(bytes, index, b"#!") {
                let end = line_end(bytes, index + 2);
                self.add_comment(index, end, CommentKind::Line);
                index = end;
                continue;
            }
            if starts(bytes, index, b"//") {
                let end = line_end(bytes, index + 2);
                self.add_comment(index, end, dart_line_kind(bytes, index));
                index = end;
                continue;
            }
            if starts(bytes, index, b"/*") {
                let (end, closed) = block_end(bytes, index, b"/*", b"*/", true);
                self.add_comment(index, end, dart_block_kind(bytes, index));
                if !closed {
                    self.error(
                        "unterminated-comment",
                        "unterminated Dart block comment",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            match bytes[index] {
                b'"' | b'\'' => index = self.scan_dart_string(index, 0),
                b'\r' | b'\n' => {
                    index = consume_newline(bytes, index);
                    self.add_safe_checkpoint(index);
                }
                _ => index += 1,
            }
        }
    }

    /// One Dart string beginning at its opening quote.
    ///
    /// The six forms are one rule with two switches. `triple` is three of the
    /// same quote, which makes a line break content instead of the end of the
    /// literal; `raw` is the `r` in front of it, which takes away both the `\`
    /// escape and `${` interpolation. A `\` in an ordinary string carries the
    /// next byte in — that is what hides a `\'` — but it does not carry a line
    /// terminator: `tokenizeSingleLineString` leaves `.string_literal` at the
    /// break either way, so `'x\<newline>y'` is an unterminated string and not
    /// a continuation.
    ///
    /// The closing delimiter is the first unescaped run of it, not the last:
    /// `''''x''''` is `'''` + `'x` + `'''` and then one quote left over, which
    /// is what the Dart scanner reports for those bytes.
    fn scan_dart_string(&mut self, quote: usize, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "Dart string interpolation nesting limit exceeded",
                ByteSpan::new(quote, quote),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let raw = dart_raw_string_prefix(bytes, quote);
        let start = if raw { quote - 1 } else { quote };
        let triple = bytes.get(quote + 1) == Some(&bytes[quote])
            && bytes.get(quote + 2) == Some(&bytes[quote]);
        let width = if triple { 3 } else { 1 };
        let delimiter = &bytes[quote..quote + width];
        let mut index = quote + width;
        while index < bytes.len() {
            if bytes[index..].starts_with(delimiter) {
                return index + width;
            }
            if !raw
                && bytes[index] == b'\\'
                && !matches!(bytes.get(index + 1), None | Some(b'\r' | b'\n'))
            {
                index += 2;
                continue;
            }
            if !raw && starts(bytes, index, b"${") {
                index = self.scan_dart_interpolation(index + 2, depth + 1);
                continue;
            }
            if !triple && matches!(bytes[index], b'\r' | b'\n') {
                self.error(
                    "unterminated-string",
                    dart_unterminated_string(raw, triple),
                    ByteSpan::new(start, index),
                );
                return index;
            }
            index += 1;
        }
        self.error(
            "unterminated-string",
            dart_unterminated_string(raw, triple),
            ByteSpan::new(start, index),
        );
        index
    }

    /// One `${ ... }` interpolation, beginning past its `${`.
    ///
    /// The braces of the expression are counted rather than searched for,
    /// because the expression is code: a nested string, a map literal, and a
    /// comment may all stand inside one. A comment written there is a comment —
    /// the Dart scanner attaches it to the token that follows, exactly as it
    /// does outside a string — and a `//` one runs to the end of its line while
    /// the string it sits inside carries on below.
    fn scan_dart_interpolation(&mut self, mut index: usize, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "Dart string interpolation nesting limit exceeded",
                ByteSpan::new(index, index),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let mut braces = 1usize;
        while index < bytes.len() {
            if starts(bytes, index, b"//") {
                let end = line_end(bytes, index + 2);
                self.add_comment(index, end, dart_line_kind(bytes, index));
                index = end;
                continue;
            }
            if starts(bytes, index, b"/*") {
                let (end, closed) = block_end(bytes, index, b"/*", b"*/", true);
                self.add_comment(index, end, dart_block_kind(bytes, index));
                if !closed {
                    self.error(
                        "unterminated-comment",
                        "unterminated Dart block comment",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            match bytes[index] {
                b'"' | b'\'' => index = self.scan_dart_string(index, depth + 1),
                b'{' => {
                    braces += 1;
                    index += 1;
                }
                b'}' => {
                    braces -= 1;
                    index += 1;
                    if braces == 0 {
                        return index;
                    }
                }
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-template-expression",
            "unterminated Dart string interpolation",
            ByteSpan::new(index, index),
        );
        index
    }

    /// One Swift source file (The Swift Programming Language, Lexical
    /// Structure: Comments, String Literals, Regular Expression Literals).
    ///
    /// Swift is a C-family syntax with four departures that decide the shape of
    /// this scanner rather than of [`Self::scan_c_family`]:
    ///
    /// * its block comment *nests*, so `/* /* */ */` is one comment;
    /// * `//!` and `/*!` document nothing — Swift's markers are `///` and
    ///   `/**` — while `////` still documents ([`swift_line_kind`]) and the
    ///   empty `/**/` does not ([`swift_block_kind`]);
    /// * `'` is not a delimiter at all, so an apostrophe hides nothing; and
    /// * a `/` may open a regular expression literal, which is the one
    ///   construct here that can carry a `//` without a quote in front of it.
    ///
    /// A string is the other construct that hides a comment opener, and Swift
    /// writes it four ways: single-line or multi-line, each raw or not, where
    /// raw is a run of `#` in front of the quote that also has to come back
    /// behind it ([`Self::scan_swift_string`]).
    ///
    /// The start of a line is a restart point only when the scan reaches it
    /// here, at the top level: a multi-line string, a nested block comment, an
    /// extended regular expression literal and an interpolation with a comment
    /// inside it all carry a line break.
    ///
    /// Ground truth for every rule below is the SwiftSyntax parser the Swift
    /// 6.3.3 toolchain ships (`SwiftParser.Parser.parse`, read for token kinds
    /// and the `lineComment`/`blockComment`/`docLineComment`/`docBlockComment`
    /// trivia and their UTF-8 offsets), with `swift-frontend -dump-parse
    /// -swift-version 6` for what the compiler makes of the same bytes.
    ///
    /// Measured against that parser over the 3,962 Swift files of swift-syntax,
    /// swift-format, swift-nio, swift-algorithms, swift-collections,
    /// swift-experimental-string-processing, swift-protobuf,
    /// swift-composable-architecture and the toolchain's own module interfaces:
    /// all 207,959 comments it reports come back here with the same byte span
    /// and the same kind, and no file is called invalid.
    fn scan_swift(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        while index < bytes.len() && !self.stopped {
            /* NOTE: SwiftSyntax reports a `shebang` token only at offset 0; the
             * same `#!` on line 2 comes back as `pound`, `exclamationMark` and
             * two operators. `#` elsewhere opens a compiler directive, a raw
             * string, or an extended regular expression literal, and the last
             * two are the ones that hide anything. */
            if index == 0 && self.offset == 0 && starts(bytes, index, b"#!") {
                let end = line_end(bytes, index + 2);
                self.add_comment(index, end, CommentKind::Line);
                index = end;
                continue;
            }
            if let Some(end) = self.scan_swift_lexeme(index, 0) {
                index = end;
                continue;
            }
            if matches!(bytes[index], b'\r' | b'\n') {
                index = consume_newline(bytes, index);
                self.add_safe_checkpoint(index);
            } else {
                index += 1;
            }
        }
    }

    /// One Swift comment, string, or regular expression literal beginning at
    /// `index`, or `None` when the byte there opens none of them.
    ///
    /// This is the whole of Swift's lexical surface that a comment can hide
    /// behind or be, which is why the top level and the inside of an
    /// interpolation ask one function for it: an interpolation is code, and a
    /// comment written there is a comment exactly as it is outside one.
    fn scan_swift_lexeme(&mut self, index: usize, depth: usize) -> Option<usize> {
        let bytes = self.source;
        if starts(bytes, index, b"//") {
            let end = line_end(bytes, index + 2);
            self.add_comment(index, end, swift_line_kind(bytes, index));
            return Some(end);
        }
        if starts(bytes, index, b"/*") {
            let (end, closed) = block_end(bytes, index, b"/*", b"*/", true);
            self.add_comment(index, end, swift_block_kind(bytes, index));
            if !closed {
                self.error(
                    "unterminated-comment",
                    "unterminated Swift block comment",
                    ByteSpan::new(index, end),
                );
            }
            return Some(end);
        }
        if bytes[index] == b'/' {
            let mut reach = Reach::default();
            let regex = swift_bare_regex(bytes, index, &mut reach);
            self.consult(reach);
            return regex;
        }
        if bytes[index] == b'#' {
            let mut reach = Reach::default();
            let hashes = swift_hash_run(bytes, index, &mut reach);
            self.consult(reach);
            let opener = index + hashes;
            if bytes.get(opener) == Some(&b'/') {
                return Some(self.scan_swift_extended_regex(index, hashes));
            }
            if bytes.get(opener) == Some(&b'"') {
                return Some(self.scan_swift_string(index, hashes, depth));
            }
            /* NOTE: the run opens neither, and the whole of it is taken rather
             * than one byte of it: a shorter run inside this one ends at the
             * same byte, so if that byte is neither a quote nor a slash then no
             * suffix of the run opens a literal either. Taking it whole also
             * keeps a line of `#` from being re-read once per byte. */
            return Some(opener);
        }
        /* NOTE: `'` is no delimiter in the language — the Swift book's Lexical
         * Structure has no single-quoted literal and no character literal at
         * all — but it is one in the compiler, which lexes `'...'` as a
         * `singleQuote` string so that it can offer the fix-it that turns it
         * into a `"..."` one (`Lexer.Cursor.lexStringQuote`). A file holding
         * one is rejected by `swiftc` either way, and following the lexer
         * rather than the grammar is what keeps a `//` inside such a literal
         * from being read as a comment and removed out of a file that is
         * already broken. No `'` can stand in Swift code outside a string, a
         * comment or a regular expression literal, all three of which are
         * settled above, so this costs a valid file nothing. */
        if matches!(bytes[index], b'"' | b'\'') {
            return Some(self.scan_swift_string(index, 0, depth));
        }
        None
    }

    /// One Swift string literal, beginning at the first `#` of a raw one and at
    /// the quote of any other.
    ///
    /// The four forms are one rule with two switches. `multiline` is three
    /// quotes, which makes a line break content instead of the end of the
    /// literal; `hashes` is the run of `#` in front of the quote, which is what
    /// makes a string raw. A raw string does not take away the escape and the
    /// interpolation — it *renames* them: with `n` hashes the escape is `\` and
    /// `n` hashes, so `\#n` is a newline and `\#(` opens an interpolation in a
    /// `#"..."#` literal while a bare `\(` is two characters of content. The
    /// closing delimiter is renamed with it, and needs the same `n` hashes
    /// behind the quote, which is what lets a raw string carry a quote.
    ///
    /// A `\` in an ordinary string carries the next byte in — that is what
    /// hides a `\"` — but it does not carry a line terminator, so `"x\` and a
    /// line break is an unterminated string rather than a continuation.
    fn scan_swift_string(&mut self, start: usize, hashes: usize, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "Swift string interpolation nesting limit exceeded",
                ByteSpan::new(start, start),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let quote = start + hashes;
        let delimiter = bytes[quote];
        let multiline = delimiter == b'"' && swift_multiline_string(bytes, quote, hashes);
        let width = if multiline { 3 } else { 1 };
        let mut index = quote + width;
        while index < bytes.len() {
            if let Some(end) = swift_string_close(bytes, index, delimiter, multiline, hashes) {
                return end;
            }
            if bytes[index] == b'\\' && swift_hashes_at(bytes, index + 1, hashes) {
                let escaped = index + 1 + hashes;
                if bytes.get(escaped) == Some(&b'(') {
                    index = self.scan_swift_interpolation(escaped + 1, depth + 1);
                    continue;
                }
                if !matches!(bytes.get(escaped), None | Some(b'\r' | b'\n')) {
                    index = escaped + 1;
                    continue;
                }
            }
            if !multiline && matches!(bytes[index], b'\r' | b'\n') {
                self.error(
                    "unterminated-string",
                    swift_unterminated_string(delimiter, hashes > 0, multiline),
                    ByteSpan::new(start, index),
                );
                return index;
            }
            index += 1;
        }
        self.error(
            "unterminated-string",
            swift_unterminated_string(delimiter, hashes > 0, multiline),
            ByteSpan::new(start, index),
        );
        index
    }

    /// One `\( ... )` interpolation, beginning past its opening parenthesis.
    ///
    /// The parentheses of the expression are counted rather than searched for,
    /// because the expression is code: a nested string, a tuple, a call, and a
    /// comment may all stand inside one. SwiftSyntax reports a comment written
    /// there as `lineComment` or `blockComment` trivia exactly as it does
    /// outside a string, and a `//` one runs to the end of its line while the
    /// multi-line string it sits inside carries on below.
    fn scan_swift_interpolation(&mut self, mut index: usize, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "Swift string interpolation nesting limit exceeded",
                ByteSpan::new(index, index),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let mut parentheses = 1usize;
        while index < bytes.len() {
            if let Some(end) = self.scan_swift_lexeme(index, depth) {
                index = end;
                continue;
            }
            match bytes[index] {
                b'(' => {
                    parentheses += 1;
                    index += 1;
                }
                b')' => {
                    parentheses -= 1;
                    index += 1;
                    if parentheses == 0 {
                        return index;
                    }
                }
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-interpolation",
            "unterminated Swift string interpolation",
            ByteSpan::new(index, index),
        );
        index
    }

    /// One extended regular expression literal `#/ ... /#`, beginning at its
    /// first `#`.
    ///
    /// This is the form that may carry an unescaped `/`, which is exactly why
    /// it exists: `#/https://x/#` is a regular expression with a `//` in the
    /// middle of it, and a scanner that read that `//` as a comment would
    /// delete the rest of the line. The closing delimiter is a `/` and at least
    /// the run of `#` the opener carried — extra ones are taken with it — so a
    /// `/#` inside a `##/ ... /##` literal is content, and a `\` carries the
    /// byte behind it in: `#/a\/#b/#` closes at the *second* `/#`.
    ///
    /// It is also the only form that may span lines, and only when it opens
    /// one: SwiftSyntax's `RegexLiteralLexer` enters multi-line mode when the
    /// opener is followed by blanks and then a line terminator, and in every
    /// other case a line terminator inside the pattern ends the literal
    /// unterminated. A multi-line literal that never closes gives its lines
    /// back rather than swallowing the file — the lexer resumes at the first
    /// newline, "so we don't want to skip over what is likely otherwise valid
    /// Swift code" — and that is the one read here the scan rewinds behind, so
    /// it is the one this reports.
    fn scan_swift_extended_regex(&mut self, start: usize, hashes: usize) -> usize {
        let bytes = self.source;
        let mut reach = Reach::default();
        let opener = start + hashes + 1;
        let mut probe = opener;
        while matches!(bytes.get(probe), Some(b' ' | b'\t')) {
            probe += 1;
        }
        reach.byte(probe);
        let multiline = bytes
            .get(probe)
            .is_some_and(|byte| is_line_terminator(*byte));
        let mut index = if multiline { probe } else { opener };
        while index < bytes.len() {
            if !multiline && is_line_terminator(bytes[index]) {
                break;
            }
            if bytes[index] == b'\\' {
                /* NOTE: `lexPatternCharacter` reads the escaped byte through the
                 * same switch as an unescaped one, and its line-terminator arm
                 * does not ask whether an escape carried it there, so a `\`
                 * before a line break ends a single-line literal rather than
                 * joining the line under it on. */
                let escaped = index + 1;
                if !multiline
                    && bytes
                        .get(escaped)
                        .is_some_and(|byte| is_line_terminator(*byte))
                {
                    index = escaped;
                    break;
                }
                index = (escaped + 1).min(bytes.len());
                continue;
            }
            if bytes[index] == b'/' && swift_hashes_at(bytes, index + 1, hashes) {
                let mut end = index + 1;
                while bytes.get(end) == Some(&b'#') {
                    end += 1;
                }
                /* NOTE: the run of `#` is taken whole — `tryEatEnding` consumes
                 * every one of them — so the byte that ended it is read one past
                 * where the scan resumes, and an append at the end of the
                 * document is exactly the edit that would put another `#` there. */
                reach.byte(end);
                self.consult(reach);
                return end;
            }
            index += 1;
        }
        let end = if multiline {
            /* NOTE: the search crossed every line under the opener and the scan
             * takes them back, so an append at the end of the document — which
             * could close this literal — has to withdraw the checkpoint it
             * would otherwise reuse the whole prefix from. */
            reach.end_of(bytes);
            probe
        } else {
            reach.byte(index);
            index
        };
        self.consult(reach);
        self.error(
            "unterminated-regex",
            "unterminated Swift extended regular expression literal",
            ByteSpan::new(start, end),
        );
        end
    }

    /// One C# source file (ECMA-334 6.3 Lexical analysis: comments, literals;
    /// 6.5 Pre-processing directives).
    ///
    /// C# is a C-family syntax with three departures that decide the shape of
    /// this scanner rather than of [`Self::scan_c_family`]:
    ///
    /// * a line whose first non-blank byte is `#` is a *pre-processing
    ///   directive*, and the rest of it is not ordinary code:
    ///   ([`Self::scan_csharp_directive`]);
    /// * a string is written eight ways — plain, verbatim, raw, and each of
    ///   those interpolated — and only the plain one takes a `\` escape
    ///   ([`Self::scan_csharp_string`]); and
    /// * an interpolation hole is code, so a comment written in one is a
    ///   comment ([`Self::scan_csharp_hole`]), while the format clause behind
    ///   its `:` is text again.
    ///
    /// The start of a line is a restart point only when the scan reaches it
    /// here, at the top level: a verbatim string, a multi-line raw string, a
    /// block comment and an interpolation hole all carry a line break.
    ///
    /// NOTE: a conditional section is scanned as ordinary code rather than
    /// skipped. Roslyn reports the body of an `#if` whose symbol is undefined
    /// as one `DisabledTextTrivia` blob and finds no comment in it, but which
    /// symbols a build defines is not in the file, and a comment inside `#if
    /// DEBUG` is a comment in every build that defines it. C and C++ read `#if
    /// 0` the same way here for the same reason.
    ///
    /// Ground truth for every rule below is the Roslyn lexer the .NET SDK
    /// 10.0.400 ships (`CSharpSyntaxTree.ParseText` with
    /// `LanguageVersion.Preview`, read for the comment, disabled-text and
    /// pre-processing-message trivia and their UTF-8 offsets, and for the token
    /// each literal was lexed as).
    ///
    /// Measured against that lexer over the 70,630 C# files of dotnet/runtime,
    /// dotnet/roslyn, dotnet/aspnetcore, dotnet/efcore, Newtonsoft.Json,
    /// Serilog and ImageSharp: all 1,946,012 comments it reports come back here
    /// with the same byte span and the same kind. Four of those files are
    /// called invalid — two are not C# at all, and two are the conditional
    /// section a `'` in prose costs, which
    /// `spec/fixtures/v1/hazards.json` records as
    /// `csharp-conditional-section-limitation`. 23,510 of the files were
    /// stripped of every comment and handed back to Roslyn, and all 650,360
    /// removals left a file that still parses with no error.
    fn scan_csharp(&mut self) {
        let bytes = self.source;
        /* NOTE: a byte order mark is consumed before the first line is read, so
         * the `#` behind one is still the first non-blank byte of its line and
         * still opens a directive — Roslyn reports the comment at the end of
         * `<BOM>#pragma warning disable 1591 // c` and this has to as well.
         * Only a full scan skips it: a suffix that happens to open with those
         * three bytes carries no mark, and the full scan of that document read
         * them as three ordinary bytes too. */
        let mut index = if self.offset == 0 {
            byte_order_mark_width(bytes)
        } else {
            0
        };
        /* NOTE: whether every byte of this line so far is blank, which is what
         * makes a `#` a directive rather than the bad token Roslyn reports at
         * CS1040. It is carried rather than searched for backwards because a
         * line of `#` would otherwise cost one scan of the line per byte, and
         * a restart begins at a line start, where `true` is the answer. */
        let mut blank_line = true;
        while index < bytes.len() && !self.stopped {
            /* NOTE: Roslyn reports a `ShebangDirectiveTrivia` for a `#!` at any
             * directive position and then raises CS9378 unless it stands at the
             * very first byte, which is also the only place `dotnet-script`
             * reads one. Only that first byte is the preamble here; the same
             * bytes lower down are a directive line whose message is opaque. */
            if index == 0 && self.offset == 0 && starts(bytes, index, b"#!") {
                let end = csharp_line_end(bytes, index + 2);
                self.add_comment(index, end, CommentKind::Line);
                index = end;
                blank_line = false;
                continue;
            }
            if bytes[index] == b'#' {
                index = self.scan_csharp_directive(index, blank_line);
                blank_line = false;
                continue;
            }
            if let Some(end) = self.scan_csharp_lexeme(index, 0) {
                index = end;
                blank_line = false;
                continue;
            }
            if matches!(bytes[index], b'\r' | b'\n') {
                index = consume_newline(bytes, index);
                self.add_safe_checkpoint(index);
                blank_line = true;
                continue;
            }
            /* NOTE: a restart is offered after a `\r` or a `\n` alone, though
             * three more characters end a line to this lexer. The incremental
             * engine's line rules — the CRLF pair, the preamble window — are
             * written in those two bytes, and a checkpoint after a U+2028 would
             * be a line start only one of the two agreed on. Refusing it costs
             * a rescan the chance to begin there and nothing else. */
            if let Some(width) = csharp_unicode_line_terminator_width(bytes, index) {
                index += width;
                blank_line = true;
                continue;
            }
            blank_line = blank_line && is_csharp_blank(bytes[index]);
            index += 1;
        }
    }

    /// One C# comment, string, or character literal beginning at `index`, or
    /// `None` when the byte there opens none of them.
    ///
    /// This is the whole of C#'s lexical surface that a comment can hide behind
    /// or be, which is why the top level and the inside of an interpolation
    /// hole ask one function for it: a hole is code, and a comment written
    /// there is a comment exactly as it is outside one.
    fn scan_csharp_lexeme(&mut self, index: usize, depth: usize) -> Option<usize> {
        let bytes = self.source;
        if starts(bytes, index, b"//") {
            let end = csharp_line_end(bytes, index + 2);
            self.add_comment(index, end, csharp_line_kind(bytes, index));
            return Some(end);
        }
        if starts(bytes, index, b"/*") {
            /* NOTE: ECMA-334 6.3.3: `/*` has no special meaning inside a
             * delimited comment, so the first `*/` closes it however many
             * openers stand in front. */
            let (end, closed) = block_end(bytes, index, b"/*", b"*/", false);
            self.add_comment(index, end, csharp_block_kind(bytes, index));
            if !closed {
                self.error(
                    "unterminated-comment",
                    "unterminated C# block comment",
                    ByteSpan::new(index, end),
                );
            }
            return Some(end);
        }
        if bytes[index] == b'\'' {
            return Some(self.scan_csharp_character(index));
        }
        if !matches!(bytes[index], b'"' | b'@' | b'$') {
            return None;
        }
        let mut reach = Reach::default();
        let prefix = csharp_literal_prefix(bytes, index, &mut reach);
        self.consult(reach);
        match prefix {
            Some(prefix) => Some(self.scan_csharp_string(prefix, depth)),
            /* NOTE: a run of `$` and `@` that no quote follows opens no literal,
             * and the whole of it is taken rather than one byte of it: a shorter
             * run inside this one ends at the same byte, so if that byte is no
             * quote then no suffix of the run opens a string either. `@x` is the
             * verbatim identifier of ECMA-334 6.4.3 and `$` outside a literal is
             * no C# token at all. */
            None => Some(csharp_prefix_end(bytes, index)),
        }
    }

    /// One `'x'` character literal, beginning at its opening quote.
    ///
    /// A `\` carries the byte behind it in — that is what hides a `'\''`, and
    /// Roslyn 10.0.400 lexes `'\` and a line feed and a `'` as one
    /// `CharacterLiteralToken` spanning the break — while an unescaped line
    /// terminator ends the literal unterminated, which it reports as CS1010,
    /// `Newline in constant`.
    fn scan_csharp_character(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let mut index = start + 1;
        while index < bytes.len() {
            if bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
                continue;
            }
            if bytes[index] == b'\'' {
                return index + 1;
            }
            if csharp_line_terminator_width(bytes, index).is_some() {
                self.error(
                    "unterminated-string",
                    "unterminated C# character literal",
                    ByteSpan::new(start, index),
                );
                return index;
            }
            index += 1;
        }
        self.error(
            "unterminated-string",
            "unterminated C# character literal",
            ByteSpan::new(start, index),
        );
        index
    }

    /// One pre-processing directive line, beginning at its `#`.
    ///
    /// ECMA-334 6.5.1 ends every directive with
    /// `PP_New_Line : PP_Whitespace? SINGLE_LINE_COMMENT? New_Line`, so a `//`
    /// is the one comment a directive line can carry: `/*` opens nothing there
    /// and neither does `'`. A `"` still opens a string — that is the file name
    /// of `#line` and the three arguments of `#pragma checksum` — and it takes
    /// no `\` escape and ends at the line either way, which is what keeps the
    /// `//` inside `#line 1 "a//b.cs"` out of reach.
    ///
    /// Four directives take the rest of their line as a message instead:
    /// `#error` and `#warning` carry the text a diagnostic quotes, and
    /// `#region` and `#endregion` the label an editor folds under. Roslyn lexes
    /// a `//` there as a comment only when it opens the message, so
    /// `#region // x` carries a comment and `#region x // y` does not.
    ///
    /// `line_initial` is false for a `#` that some other byte on its line came
    /// before. Roslyn raises CS1040 for that one and makes the whole rest of
    /// the line a single bad token, with no comment in it, so nothing is
    /// reported here either.
    fn scan_csharp_directive(&mut self, index: usize, line_initial: bool) -> usize {
        let bytes = self.source;
        let end = csharp_line_end(bytes, index);
        if !line_initial {
            return end;
        }
        /* NOTE: `#` and its keyword may stand apart — `PP_Kind` opens with
         * `PP_Whitespace?` — so `#  region` is the directive `#region`. */
        let mut cursor = index + 1;
        while cursor < end && is_csharp_blank(bytes[cursor]) {
            cursor += 1;
        }
        let name = cursor;
        while cursor < end && bytes[cursor].is_ascii_alphabetic() {
            cursor += 1;
        }
        if csharp_directive_takes_a_message(&bytes[name..cursor]) {
            while cursor < end && is_csharp_blank(bytes[cursor]) {
                cursor += 1;
            }
            if starts(bytes, cursor, b"//") {
                /* NOTE: the directive lexer has no documentation trivia, so a
                 * `///` on this line is the ordinary line comment Roslyn reports
                 * it as. */
                self.add_comment(cursor, end, CommentKind::Line);
            }
            return end;
        }
        /* NOTE: every other directive is lexed as tokens, so the line is walked
         * for the two that matter. A `//` cannot straddle the end of the line:
         * what ends one is never a `/`. */
        let mut token = index + 1;
        while token < end {
            if starts(bytes, token, b"//") {
                self.add_comment(token, end, CommentKind::Line);
                return end;
            }
            if bytes[token] == b'"' {
                token += 1;
                while token < end && bytes[token] != b'"' {
                    token += 1;
                }
                /* NOTE: Roslyn's `ScanStringLiteral` takes an `inDirective`
                 * flag that turns the `\` escape off, so `"a\"` closes at its
                 * second quote and the `b` behind it is a token of its own.
                 * A string left open ends at the line, with CS1010. */
                token = (token + 1).min(end);
                continue;
            }
            token += 1;
        }
        end
    }

    /// One C# string literal, beginning at the first byte of its `$` and `@`
    /// prefix and at the quote of a plain one.
    ///
    /// The eight forms are three rules and one switch. A *plain* string takes
    /// the `\` escape and ends at its line; a *verbatim* one spells its quote
    /// `""`, takes no escape, and carries line breaks; a *raw* one is opaque
    /// until a run of at least as many quotes as its opener carried comes back,
    /// and carries line breaks only when its opener ends a line. The switch is
    /// the `$` run: with none the braces are content, with `n` of them a run of
    /// `n` braces opens a hole.
    ///
    /// A plain interpolated string is the one form whose *text* stops at a line
    /// terminator while its *holes* do not: C# 11 let an expression inside one
    /// span lines, which is why a `//` comment written in a hole runs to the
    /// end of its line and the string carries on below.
    fn scan_csharp_string(&mut self, prefix: CsharpPrefix, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "C# string interpolation nesting limit exceeded",
                ByteSpan::new(prefix.start, prefix.start),
            );
            return self.source.len();
        }
        match prefix.form {
            CsharpStringForm::Raw => self.scan_csharp_raw_string(prefix, depth),
            CsharpStringForm::Verbatim => self.scan_csharp_verbatim_string(prefix, depth),
            CsharpStringForm::Plain => self.scan_csharp_plain_string(prefix, depth),
        }
    }

    /// One plain `"..."` string, interpolated when a `$` stands in front of it.
    fn scan_csharp_plain_string(&mut self, prefix: CsharpPrefix, depth: usize) -> usize {
        let bytes = self.source;
        let mut index = prefix.quote + 1;
        while index < bytes.len() {
            match bytes[index] {
                /* NOTE: a `\` carries the byte behind it in whatever it is, a
                 * line terminator included: `ScanEscapeSequence` takes one
                 * character and raises CS1009 when it spells no escape, so
                 * `"p\` and a line feed is a string that carries on below and
                 * not one left open. A CRLF pair still ends it — only the `\r`
                 * is carried — which is why this counts bytes rather than
                 * characters. */
                b'\\' => index = (index + 2).min(bytes.len()),
                b'"' => return index + 1,
                b'{' if prefix.dollars > 0 => {
                    if bytes.get(index + 1) == Some(&b'{') {
                        index += 2;
                        continue;
                    }
                    index = self.scan_csharp_hole(index + 1, prefix.dollars, depth + 1);
                }
                b'}' if prefix.dollars > 0 && bytes.get(index + 1) == Some(&b'}') => index += 2,
                _ if csharp_line_terminator_width(bytes, index).is_some() => {
                    self.error(
                        "unterminated-string",
                        csharp_unterminated_string(prefix.form, prefix.dollars > 0),
                        ByteSpan::new(prefix.start, index),
                    );
                    return index;
                }
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-string",
            csharp_unterminated_string(prefix.form, prefix.dollars > 0),
            ByteSpan::new(prefix.start, index),
        );
        index
    }

    /// One verbatim `@"..."` string, interpolated when a `$` stands with it.
    ///
    /// `""` is how such a literal spells a quote, so the closing delimiter is
    /// the first quote no second quote follows, and a `\` is content.
    fn scan_csharp_verbatim_string(&mut self, prefix: CsharpPrefix, depth: usize) -> usize {
        let bytes = self.source;
        let mut index = prefix.quote + 1;
        while index < bytes.len() {
            match bytes[index] {
                b'"' if bytes.get(index + 1) == Some(&b'"') => index += 2,
                b'"' => return index + 1,
                b'{' if prefix.dollars > 0 => {
                    if bytes.get(index + 1) == Some(&b'{') {
                        index += 2;
                        continue;
                    }
                    index = self.scan_csharp_hole(index + 1, prefix.dollars, depth + 1);
                }
                b'}' if prefix.dollars > 0 && bytes.get(index + 1) == Some(&b'}') => index += 2,
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-string",
            csharp_unterminated_string(prefix.form, prefix.dollars > 0),
            ByteSpan::new(prefix.start, index),
        );
        index
    }

    /// One raw `"""..."""` string, interpolated when a `$` run stands with it.
    ///
    /// The closing delimiter is the first run of at least as many quotes as the
    /// opener carried, and the whole run is taken: Roslyn ends
    /// `"""abc""""` at its fourth closing quote and raises CS8998 rather than
    /// leaving one behind. A run shorter than the opener's is content, which is
    /// what lets a raw string carry `"""`.
    ///
    /// The opener decides whether line breaks are content: when only blanks
    /// stand between it and the end of its line the literal is the multi-line
    /// form, and otherwise a line terminator ends it unterminated with CS8997.
    /// Roslyn ends a multi-line one at the first run all the same, raising
    /// CS9000 when that run does not stand on a line of its own, so the search
    /// here is the same one either way.
    fn scan_csharp_raw_string(&mut self, prefix: CsharpPrefix, depth: usize) -> usize {
        let bytes = self.source;
        let multiline = csharp_multiline_raw_string(bytes, prefix.quote + prefix.quotes);
        let mut index = prefix.quote + prefix.quotes;
        while index < bytes.len() {
            match bytes[index] {
                b'"' => {
                    let mut end = index;
                    while bytes.get(end) == Some(&b'"') {
                        end += 1;
                    }
                    if end - index >= prefix.quotes {
                        /* NOTE: the run is taken whole, so the byte that ended
                         * it is read one past where the scan resumes — and an
                         * append at the end of the document is exactly the edit
                         * that would put another quote there. */
                        let mut reach = Reach::default();
                        reach.byte(end);
                        self.consult(reach);
                        return end;
                    }
                    index = end;
                }
                b'{' if prefix.dollars > 0 => {
                    let mut run = index;
                    while bytes.get(run) == Some(&b'{') {
                        run += 1;
                    }
                    if run - index < prefix.dollars {
                        index = run;
                        continue;
                    }
                    /* NOTE: a run longer than the `$` run spends its first
                     * braces as content and its last `dollars` on the hole,
                     * which is how `$"""{{x}}"""` writes a literal `{` in front
                     * of one — Roslyn reports the first brace as
                     * `InterpolatedStringTextToken` and the second as
                     * `OpenBraceToken`. */
                    index = self.scan_csharp_hole(run, prefix.dollars, depth + 1);
                }
                _ if !multiline && csharp_line_terminator_width(bytes, index).is_some() => {
                    self.error(
                        "unterminated-string",
                        csharp_unterminated_string(prefix.form, prefix.dollars > 0),
                        ByteSpan::new(prefix.start, index),
                    );
                    return index;
                }
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-string",
            csharp_unterminated_string(prefix.form, prefix.dollars > 0),
            ByteSpan::new(prefix.start, index),
        );
        index
    }

    /// One interpolation hole, beginning past the `braces` opening braces.
    ///
    /// The brackets of the expression are counted rather than searched for,
    /// because the expression is code: a nested string, a collection
    /// expression, a lambda, and a comment may all stand inside one. Roslyn
    /// reports a comment written there as `SingleLineCommentTrivia` or
    /// `MultiLineCommentTrivia` exactly as it does outside a string.
    ///
    /// A `:` where no bracket is open ends the expression and opens the format
    /// clause, which is text to the end of the hole: `$"{x:D4 // n}"` holds no
    /// comment. That is Roslyn's rule down to the case that surprises a reader,
    /// `$"{global::X}"`, whose first `:` opens a format clause of its own.
    fn scan_csharp_hole(&mut self, mut index: usize, braces: usize, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "C# string interpolation nesting limit exceeded",
                ByteSpan::new(index, index),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let mut open = 0usize;
        while index < bytes.len() {
            if let Some(end) = self.scan_csharp_lexeme(index, depth) {
                index = end;
                continue;
            }
            match bytes[index] {
                b'(' | b'[' | b'{' => {
                    open += 1;
                    index += 1;
                }
                b')' | b']' => {
                    open = open.saturating_sub(1);
                    index += 1;
                }
                b'}' => {
                    if open > 0 {
                        open -= 1;
                        index += 1;
                        continue;
                    }
                    let (next, closed) = csharp_hole_close(bytes, index, braces);
                    if closed {
                        return next;
                    }
                    index = next;
                }
                b':' if open == 0 => return self.scan_csharp_format(index, braces),
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-interpolation",
            "unterminated C# string interpolation",
            ByteSpan::new(index, index),
        );
        index
    }

    /// The format clause of an interpolation hole, beginning at its `:`.
    ///
    /// Everything to the closing braces of the hole is text — Roslyn lexes it
    /// as one `InterpolatedStringTextToken` — so no comment is reported here
    /// and no nested literal is lexed.
    fn scan_csharp_format(&mut self, mut index: usize, braces: usize) -> usize {
        let bytes = self.source;
        while index < bytes.len() {
            if bytes[index] == b'}' {
                let (next, closed) = csharp_hole_close(bytes, index, braces);
                if closed {
                    return next;
                }
                index = next;
                continue;
            }
            index += 1;
        }
        self.error(
            "unterminated-interpolation",
            "unterminated C# string interpolation",
            ByteSpan::new(index, index),
        );
        index
    }

    fn scan_scala(&mut self) {
        let bytes = self.source;
        let mut index = if self.offset == 0 {
            byte_order_mark_width(bytes)
        } else {
            0
        };
        while index < bytes.len() && !self.stopped {
            /* NOTE: Scala has no `#` comment of its own, so a `#!` line is the
             * only shape a `#` is part of, and it is a preamble only at the
             * very first byte — a byte order mark permitting — which is also
             * the only place scala-cli reads one. The same bytes lower down are
             * ordinary code. */
            if self.offset == 0
                && index == byte_order_mark_width(bytes)
                && starts(bytes, index, b"#!")
            {
                let end = line_end(bytes, index + 2);
                self.add_comment(index, end, CommentKind::Line);
                index = end;
                continue;
            }
            if let Some(end) = self.scan_scala_lexeme(index, 0) {
                index = end;
                continue;
            }
            if matches!(bytes[index], b'\r' | b'\n') {
                index = consume_newline(bytes, index);
                self.add_safe_checkpoint(index);
            } else {
                index += 1;
            }
        }
    }

    /// One Scala comment, string, backquoted identifier, or XML literal
    /// beginning at `index`, or `None` when the byte there opens none of them.
    ///
    /// This is the whole of Scala's lexical surface that a comment can hide
    /// behind or be, which is why the top level and the inside of a `${ ... }`
    /// interpolation or of an XML literal's braces ask one function for it:
    /// both are code, and a comment written there is a comment exactly as it is
    /// outside one.
    fn scan_scala_lexeme(&mut self, index: usize, depth: usize) -> Option<usize> {
        let bytes = self.source;
        if starts(bytes, index, b"//") {
            let end = line_end(bytes, index + 2);
            self.add_comment(index, end, scala_line_kind(bytes, index));
            return Some(end);
        }
        if starts(bytes, index, b"/*") {
            let (end, closed) = block_end(bytes, index, b"/*", b"*/", true);
            self.add_comment(index, end, scala_block_kind(bytes, index));
            if !closed {
                self.error(
                    "unterminated-comment",
                    "unterminated Scala block comment",
                    ByteSpan::new(index, end),
                );
            }
            return Some(end);
        }
        if bytes[index] == b'"' {
            return Some(self.scan_scala_string(index, depth));
        }
        /* NOTE: a backquoted identifier may hold any bytes but a backtick,
         * `//` included — `` val `a//b` = 1 `` is one identifier — and ends at
         * its line, which is what keeps it from swallowing the source below. */
        if bytes[index] == b'`' {
            return Some(self.scala_backquoted_identifier(index));
        }
        if bytes[index] == b'<' && scala_is_xml_start(bytes, index) {
            return Some(self.scan_scala_xml(index, depth));
        }
        None
    }

    /// One Scala string literal, beginning at its opening quote.
    ///
    /// A string is interpolated exactly when an identifier stands directly
    /// before its quote — the compiler's lexer turns that identifier into
    /// `INTERPOLATIONID` — so `s"..."`, `raw"..."`, a custom interpolator such
    /// as `xml"..."`, and each of them triple-quoted are the interpolated
    /// forms, and every other quote opens a plain string whose `$` is content.
    /// Inside an interpolated string `$$` and `$"` write a literal `$` and `"`,
    /// `${ ... }` opens an expression that is code, and `$` followed by an
    /// identifier starts another expression; the compiler rejects any other
    /// `$`, and this scanner reads it as content, which costs a comment nothing
    /// and keeps a broken file scannable. A `\` carries the byte behind it in
    /// a single-line string — that is what hides a `\"` — and means nothing in
    /// a triple-quoted one.
    ///
    /// A triple-quoted string closes on the first three quotes of a run, and
    /// any further quotes of the run are part of the string's value: the
    /// lexer's `isTripleQuote` consumes the first three and `putChar`s the
    /// rest, so `"""a""""` is the string `a"` and ends after the run of four.
    /// That is where Scala parts company with Kotlin's existing scanner, which
    /// closes on the first three of a run and leaves a stray quote behind.
    fn scan_scala_string(&mut self, start: usize, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "Scala string interpolation nesting limit exceeded",
                ByteSpan::new(start, start),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let interpolated = scala_interpolator(bytes, start);
        let triple = starts(bytes, start, b"\"\"\"");
        let mut index = start + if triple { 3 } else { 1 };
        while index < bytes.len() {
            if interpolated && bytes[index] == b'$' {
                match bytes.get(index + 1) {
                    Some(b'$' | b'"') => {
                        index += 2;
                        continue;
                    }
                    Some(b'{') => {
                        index = self.scan_scala_expression(index + 2, depth + 1);
                        continue;
                    }
                    Some(&byte) if scala_identifier_start(byte) => {
                        index += 2;
                        while index < bytes.len() && scala_identifier_part(bytes[index]) {
                            index += 1;
                        }
                        continue;
                    }
                    _ => {
                        index += 1;
                        continue;
                    }
                }
            }
            if bytes[index] == b'"' {
                if triple {
                    let run = index + count_run(bytes, index, b'"');
                    if run - index >= 3 {
                        return run;
                    }
                } else {
                    return index + 1;
                }
            } else if bytes[index] == b'\\' {
                if !triple {
                    index = (index + 2).min(bytes.len());
                    continue;
                }
            } else if !triple && matches!(bytes[index], b'\r' | b'\n') {
                self.error(
                    "unterminated-string",
                    "unterminated Scala string",
                    ByteSpan::new(start, index),
                );
                return index;
            }
            index += 1;
        }
        self.error(
            "unterminated-string",
            if triple {
                "unterminated Scala multi-line string"
            } else {
                "unterminated Scala string"
            },
            ByteSpan::new(start, index),
        );
        index
    }

    /// One `${ ... }` expression or the inside of an XML literal's braces,
    /// beginning past the opening brace.
    ///
    /// The braces are counted rather than searched for, because the expression
    /// is code: a nested string, a tuple, a call, and a comment may all stand
    /// inside one, and a string or an XML literal written there is scanned as
    /// such. The compiler reports a comment written inside an interpolation as
    /// a comment exactly as it does outside a string, and a `//` one runs to
    /// the end of its line while a multi-line string carries on below.
    fn scan_scala_expression(&mut self, mut index: usize, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "Scala string interpolation nesting limit exceeded",
                ByteSpan::new(index, index),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let mut braces = 1usize;
        while index < bytes.len() {
            if let Some(end) = self.scan_scala_lexeme(index, depth) {
                index = end;
                continue;
            }
            match bytes[index] {
                b'{' => {
                    braces += 1;
                    index += 1;
                }
                b'}' => {
                    braces -= 1;
                    index += 1;
                    if braces == 0 {
                        return index;
                    }
                }
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-template-expression",
            "unterminated Scala string-template expression",
            ByteSpan::new(index, index),
        );
        index
    }

    /// One backquoted identifier, beginning at its opening backtick.
    ///
    /// The compiler's `getBackquotedIdent` reads to the next backtick and ends
    /// at a line terminator, which it reports as `unclosed quoted identifier`;
    /// this scanner follows both.
    fn scala_backquoted_identifier(&mut self, start: usize) -> usize {
        let bytes = self.source;
        let mut index = start + 1;
        while index < bytes.len() {
            if bytes[index] == b'`' {
                return index + 1;
            }
            if matches!(bytes[index], b'\r' | b'\n') {
                self.error(
                    "unterminated-identifier",
                    "unclosed quoted identifier",
                    ByteSpan::new(start, index),
                );
                return index;
            }
            index += 1;
        }
        self.error(
            "unterminated-identifier",
            "unclosed quoted identifier",
            ByteSpan::new(start, index),
        );
        index
    }

    /// One XML literal, beginning at the `<` that opened it.
    ///
    /// The compiler's *lexer* emits an `XMLSTART` token for the `<` and then
    /// hands the literal to the parser, which re-reads it with an XML scanner:
    /// element text, CDATA and processing instructions are *not* code, and a
    /// `//` in them is a byte the lexer alone would call a comment. This
    /// scanner follows the parser, so text and the construct bodies are
    /// opaque, `{ ... }` in text or an attribute is code, `<!-- ... -->` is an
    /// XML comment, and the literal ends at the close tag matching its root or
    /// at a self-closing `/>`. A literal whose root never closes is a file the
    /// parser rejects, and the rest of it is read as opaque content, which is
    /// the safe half of being wrong: a comment below a broken literal is
    /// over-kept rather than removed out of one that parses.
    fn scan_scala_xml(&mut self, start: usize, depth: usize) -> usize {
        let bytes = self.source;
        let Some((mut index, self_closing, root_start, root_end)) =
            self.scala_xml_tag(start, depth)
        else {
            return bytes.len();
        };
        if self_closing {
            return index;
        }
        let mut stack = vec![(root_start, root_end)];
        while index < bytes.len() {
            if bytes[index] == b'{' {
                index = self.scan_scala_expression(index + 1, depth + 1);
                continue;
            }
            if bytes[index] != b'<' {
                index += 1;
                continue;
            }
            if starts(bytes, index, b"<!--") {
                let Some(relative) = find_subslice(&bytes[index + 4..], b"-->") else {
                    return bytes.len();
                };
                let end = index + 4 + relative + 3;
                self.add_comment(index, end, CommentKind::HtmlComment);
                index = end;
                continue;
            }
            if starts(bytes, index, b"<![CDATA[") {
                let Some(relative) = find_subslice(&bytes[index + 9..], b"]]>") else {
                    return bytes.len();
                };
                index = index + 9 + relative + 3;
                continue;
            }
            if starts(bytes, index, b"<?") {
                let Some(relative) = find_subslice(&bytes[index + 2..], b"?>") else {
                    return bytes.len();
                };
                index = index + 2 + relative + 2;
                continue;
            }
            if starts(bytes, index, b"</") {
                let name_start = index + 2;
                if name_start < bytes.len() && xml_name_start(bytes[name_start]) {
                    let name_end = name_start + xml_name_len(&bytes[name_start..]);
                    let Some(close_end) = skip_xml_tag_tail(bytes, name_end) else {
                        return bytes.len();
                    };
                    if bytes[name_start..name_end]
                        == bytes[stack.last().unwrap().0..stack.last().unwrap().1]
                    {
                        stack.pop();
                        if stack.is_empty() {
                            return close_end;
                        }
                        index = close_end;
                        continue;
                    }
                }
                index += 2;
                continue;
            }
            if starts(bytes, index, b"<!") {
                let Some(relative) = find_subslice(&bytes[index + 2..], b">") else {
                    return bytes.len();
                };
                index = index + 2 + relative + 1;
                continue;
            }
            if index + 1 < bytes.len() && xml_name_start(bytes[index + 1]) {
                let Some((after, self_closing, name_start, name_end)) =
                    self.scala_xml_tag(index, depth)
                else {
                    return bytes.len();
                };
                if !self_closing {
                    stack.push((name_start, name_end));
                }
                index = after;
                continue;
            }
            index += 1;
        }
        bytes.len()
    }

    /// One XML tag beginning at its `<`, returning where it ends, whether it
    /// is self-closing, and the range of its name.
    fn scala_xml_tag(&mut self, start: usize, depth: usize) -> Option<(usize, bool, usize, usize)> {
        let bytes = self.source;
        let mut index = start + 1;
        if index >= bytes.len() || !xml_name_start(bytes[index]) {
            return None;
        }
        let name_start = index;
        index += xml_name_len(&bytes[index..]);
        let name_end = index;
        loop {
            if index >= bytes.len() {
                return None;
            }
            match bytes[index] {
                b'>' => return Some((index + 1, false, name_start, name_end)),
                b'/' if bytes.get(index + 1) == Some(&b'>') => {
                    return Some((index + 2, true, name_start, name_end));
                }
                b'"' | b'\'' => {
                    let quote = bytes[index];
                    index += 1;
                    while index < bytes.len() && bytes[index] != quote {
                        index += 1;
                    }
                    if index >= bytes.len() {
                        return None;
                    }
                    index += 1;
                }
                b'{' => {
                    index = self.scan_scala_expression(index + 1, depth + 1);
                }
                _ => index += 1,
            }
        }
    }

    fn scan_shell(&mut self) {
        let _ = self.scan_shell_region(0, None, 0);
    }

    fn scan_shell_region(
        &mut self,
        mut index: usize,
        terminator: Option<ShellTerminator>,
        depth: usize,
    ) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "shell lexical nesting limit exceeded",
                ByteSpan::new(index, index),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let mut heredocs: Vec<Heredoc> = Vec::new();
        let backtick_terminator = matches!(terminator, Some(ShellTerminator::Backtick(_)));
        let parenthesis_terminator = matches!(terminator, Some(ShellTerminator::Parenthesis(_)));
        let mut parentheses = usize::from(parenthesis_terminator);
        let mut word_open = false;
        let mut command_position = true;
        let mut case_states = Vec::new();
        while index < bytes.len() && !self.stopped {
            match bytes[index] {
                b'#' if !word_open => {
                    let end = line_end(bytes, index + 1);
                    self.add_comment(index, end, CommentKind::Line);
                    index = end;
                }
                b'\'' => {
                    let start = index;
                    let (end, closed) = shell_single_quote_end(bytes, index);
                    index = end;
                    if !closed {
                        self.error(
                            "unterminated-string",
                            "unterminated shell single quote",
                            ByteSpan::new(start, index),
                        );
                    }
                    word_open = true;
                    command_position = false;
                }
                b'"' => {
                    index = self.scan_shell_double_quote(index, depth + 1);
                    word_open = true;
                    command_position = false;
                }
                b'`' if backtick_terminator => return index + 1,
                b'`' => {
                    index = self.scan_shell_region(
                        index + 1,
                        Some(ShellTerminator::Backtick(index)),
                        depth + 1,
                    );
                    word_open = true;
                    command_position = false;
                }
                b'$' if bytes.get(index + 1) == Some(&b'(') => {
                    index = self.scan_shell_region(
                        index + 2,
                        Some(ShellTerminator::Parenthesis(index)),
                        depth + 1,
                    );
                    word_open = true;
                    command_position = false;
                }
                b'$' if bytes.get(index + 1) == Some(&b'\'')
                    && matches!(self.options.dialect, Dialect::Bash53 | Dialect::Zsh) =>
                {
                    index = self.quoted_or_error(index + 1, true, "shell ANSI-C quoted string");
                    word_open = true;
                    command_position = false;
                }
                b'<' if bytes.get(index + 1) == Some(&b'<') => {
                    word_open = false;
                    if bytes.get(index + 2) == Some(&b'<') {
                        index += 3;
                    } else {
                        let mut reach = Reach::default();
                        let parsed = parse_heredoc(bytes, index, &mut reach);
                        self.consult(reach);
                        if let Some((heredoc, end)) = parsed {
                            heredocs.push(heredoc);
                            index = end;
                            word_open = true;
                        } else {
                            index += 1;
                        }
                    }
                }
                b'\r' | b'\n' if !heredocs.is_empty() => {
                    index = consume_newline(bytes, index);
                    for heredoc in heredocs.drain(..) {
                        match heredoc_body_end(bytes, index, &heredoc) {
                            Some(end) => index = end,
                            None => {
                                self.error(
                                    "unterminated-heredoc",
                                    "unterminated shell heredoc",
                                    ByteSpan::new(heredoc.operator, bytes.len()),
                                );
                                return bytes.len();
                            }
                        }
                    }
                    word_open = false;
                    command_position = case_states.last() != Some(&ShellCaseState::Pattern);
                    if terminator.is_none() && case_states.is_empty() {
                        self.add_safe_checkpoint(index);
                    }
                }
                b'\r' | b'\n' => {
                    index = consume_newline(bytes, index);
                    word_open = false;
                    command_position = case_states.last() != Some(&ShellCaseState::Pattern);
                    if terminator.is_none() && case_states.is_empty() {
                        self.add_safe_checkpoint(index);
                    }
                }
                b'(' if parenthesis_terminator => {
                    parentheses += 1;
                    index += 1;
                    word_open = false;
                    command_position = case_states.last() != Some(&ShellCaseState::Pattern);
                }
                b')' if parenthesis_terminator => {
                    if parentheses == 1
                        && let Some(state @ ShellCaseState::Pattern) = case_states.last_mut()
                    {
                        *state = ShellCaseState::Body;
                        index += 1;
                        word_open = false;
                        command_position = true;
                        continue;
                    }
                    parentheses = parentheses.saturating_sub(1);
                    index += 1;
                    if parentheses == 0 {
                        return index;
                    }
                    word_open = false;
                    command_position = true;
                }
                b')' if case_states.last() == Some(&ShellCaseState::Pattern) => {
                    *case_states.last_mut().expect("case state exists") = ShellCaseState::Body;
                    index += 1;
                    word_open = false;
                    command_position = true;
                }
                b';' if case_states.last() == Some(&ShellCaseState::Body)
                    && (starts(bytes, index, b";;") || starts(bytes, index, b";&")) =>
                {
                    let width = if starts(bytes, index, b";;&") { 3 } else { 2 };
                    *case_states.last_mut().expect("case state exists") = ShellCaseState::Pattern;
                    index += width;
                    word_open = false;
                    command_position = false;
                }
                b';' | b'&' | b'|' | b'(' | b')' => {
                    index += 1;
                    word_open = false;
                    command_position = bytes[index - 1] != b'|'
                        || case_states.last() != Some(&ShellCaseState::Pattern);
                }
                b'<' | b'>' => {
                    index += 1;
                    word_open = false;
                }
                byte if byte.is_ascii_whitespace() => {
                    index += 1;
                    word_open = false;
                }
                b'\\' => {
                    if bytes.get(index + 1) == Some(&b'\r') && bytes.get(index + 2) == Some(&b'\n')
                    {
                        index += 3;
                    } else if matches!(bytes.get(index + 1), Some(b'\r' | b'\n')) {
                        index += 2;
                    } else {
                        index = (index + 2).min(bytes.len());
                        word_open = true;
                        command_position = false;
                    }
                }
                byte if !word_open && (byte.is_ascii_alphabetic() || byte == b'_') => {
                    let start = index;
                    index += 1;
                    while index < bytes.len()
                        && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                    {
                        index += 1;
                    }
                    let boundary = bytes.get(index).is_none_or(|byte| {
                        byte.is_ascii_whitespace()
                            || matches!(byte, b';' | b'&' | b'|' | b'(' | b')' | b'<' | b'>')
                    });
                    let token = &bytes[start..index];
                    if boundary && token == b"case" && command_position {
                        case_states.push(ShellCaseState::AwaitIn);
                        command_position = false;
                    } else if boundary
                        && token == b"in"
                        && case_states.last() == Some(&ShellCaseState::AwaitIn)
                    {
                        *case_states.last_mut().expect("case state exists") =
                            ShellCaseState::Pattern;
                        command_position = false;
                    } else if boundary
                        && token == b"esac"
                        && (command_position
                            || case_states.last() == Some(&ShellCaseState::Pattern))
                    {
                        let _ = case_states.pop();
                        command_position = false;
                    } else {
                        command_position = command_position && bytes.get(index) == Some(&b'=');
                    }
                    word_open = true;
                }
                _ => {
                    index += 1;
                    word_open = true;
                    command_position = false;
                }
            }
        }
        if let Some(terminator) = terminator {
            let (code, message, start) = match terminator {
                ShellTerminator::Parenthesis(start) => (
                    "unterminated-command-substitution",
                    "unterminated shell command substitution",
                    start,
                ),
                ShellTerminator::Backtick(start) => (
                    "unterminated-string",
                    "unterminated shell command substitution",
                    start,
                ),
            };
            self.error(code, message, ByteSpan::new(start, index));
        }
        index
    }

    fn scan_shell_double_quote(&mut self, start: usize, depth: usize) -> usize {
        let bytes = self.source;
        let mut index = start + 1;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index = (index + 2).min(bytes.len()),
                b'"' => return index + 1,
                b'$' if bytes.get(index + 1) == Some(&b'(') => {
                    index = self.scan_shell_region(
                        index + 2,
                        Some(ShellTerminator::Parenthesis(index)),
                        depth + 1,
                    );
                }
                b'`' => {
                    index = self.scan_shell_region(
                        index + 1,
                        Some(ShellTerminator::Backtick(index)),
                        depth + 1,
                    );
                }
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-string",
            "unterminated shell double quote",
            ByteSpan::new(start, index),
        );
        index
    }

    fn scan_sql(&mut self) {
        let bytes = self.source;
        let nested = matches!(self.options.dialect, Dialect::PostgreSql | Dialect::TSql);
        let mut index = 0;
        while index < bytes.len() && !self.stopped {
            if starts(bytes, index, b"--")
                && (self.options.dialect != Dialect::MySql
                    || mysql_dash_comment_boundary(bytes.get(index + 2).copied()))
            {
                let end = line_end(bytes, index + 2);
                self.add_comment(index, end, CommentKind::Line);
                index = end;
                continue;
            }
            if bytes[index] == b'#' && self.options.dialect == Dialect::MySql {
                let end = line_end(bytes, index + 1);
                self.add_comment(index, end, CommentKind::Line);
                index = end;
                continue;
            }
            if starts(bytes, index, b"/*") {
                let (end, closed) = block_end(bytes, index, b"/*", b"*/", nested);
                self.add_comment(index, end, CommentKind::Block);
                if !closed {
                    self.error(
                        "unterminated-comment",
                        "unterminated SQL block comment",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            if bytes[index] == b'\'' {
                let start = index;
                let backslash_escapes = self.options.dialect == Dialect::MySql
                    || (self.options.dialect == Dialect::PostgreSql
                        && postgres_escape_string_start(bytes, index));
                let (end, closed) = sql_quoted_end(bytes, index, b'\'', backslash_escapes);
                index = end;
                if !closed {
                    self.error(
                        "unterminated-string",
                        "unterminated SQL string",
                        ByteSpan::new(start, index),
                    );
                }
                continue;
            }
            if matches!(bytes[index], b'"' | b'`') {
                let mysql_string = bytes[index] == b'"' && self.options.dialect == Dialect::MySql;
                let (end, closed) = if mysql_string {
                    sql_quoted_end(bytes, index, b'"', true)
                } else {
                    sql_identifier_end(bytes, index, bytes[index])
                };
                if !closed {
                    self.error(
                        if mysql_string {
                            "unterminated-string"
                        } else {
                            "unterminated-identifier"
                        },
                        if mysql_string {
                            "unterminated MySQL quoted string"
                        } else {
                            "unterminated SQL quoted identifier"
                        },
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            if bytes[index] == b'[' && self.options.dialect == Dialect::TSql {
                let (end, closed) = sql_identifier_end(bytes, index, b']');
                if !closed {
                    self.error(
                        "unterminated-identifier",
                        "unterminated T-SQL bracket identifier",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            let dollar = (bytes[index] == b'$' && self.options.dialect == Dialect::PostgreSql)
                .then(|| {
                    let mut reach = Reach::default();
                    let quoted = sql_dollar_quote_end(bytes, index, &mut reach);
                    self.consult(reach);
                    quoted
                })
                .flatten();
            if let Some((end, closed)) = dollar {
                if !closed {
                    self.error(
                        "unterminated-string",
                        "unterminated PostgreSQL dollar-quoted string",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            let q_quote = ((bytes[index] == b'q' || bytes[index] == b'Q')
                && self.options.dialect == Dialect::Oracle)
                .then(|| {
                    let mut reach = Reach::default();
                    let quoted = oracle_q_quote_end(bytes, index, &mut reach);
                    self.consult(reach);
                    quoted
                })
                .flatten();
            if let Some((end, closed)) = q_quote {
                if !closed {
                    self.error(
                        "unterminated-string",
                        "unterminated Oracle q-quoted string",
                        ByteSpan::new(index, end),
                    );
                }
                index = end;
                continue;
            }
            if matches!(bytes[index], b'\r' | b'\n') {
                index = consume_newline(bytes, index);
                self.add_safe_checkpoint(index);
            } else {
                index += 1;
            }
        }
    }

    fn scan_javascript(&mut self) {
        let _ = self.scan_js_code(0, None, 0);
    }

    fn scan_js_code(&mut self, mut index: usize, stop_brace: Option<usize>, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "JavaScript lexical nesting limit exceeded",
                ByteSpan::new(index, index),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let mut brace_depth = stop_brace.unwrap_or(0);
        let mut regex_allowed = true;
        let mut control_parentheses = Vec::new();
        let mut pending_control_parenthesis = false;
        let mut brace_blocks = Vec::new();
        let mut statement_start = stop_brace.is_none();
        let mut pending_block = false;
        while index < bytes.len() && !self.stopped {
            if index == 0 && self.offset == 0 && starts(bytes, index, b"#!") {
                let end = js_line_end(bytes, index + 2);
                self.add_comment(index, end, CommentKind::Line);
                index = end;
                continue;
            }
            if bytes[index] == b'/' {
                match bytes.get(index + 1) {
                    Some(b'/') => {
                        let end = js_line_end(bytes, index + 2);
                        self.add_comment(index, end, line_kind(bytes, index));
                        index = end;
                        continue;
                    }
                    Some(b'*') => {
                        let (end, closed) = block_end(bytes, index, b"/*", b"*/", false);
                        self.add_comment(index, end, block_kind(bytes, index));
                        if !closed {
                            self.error(
                                "unterminated-comment",
                                "unterminated JavaScript block comment",
                                ByteSpan::new(index, end),
                            );
                        }
                        index = end;
                        continue;
                    }
                    _ => {}
                }
            }
            /* PERF: Annex B HTML-like comments are uncommon.  Guard both delimiter
             * checks by their first byte so the ordinary JavaScript hot path
             * does not perform two slice comparisons for every source byte. */
            if (bytes[index] == b'<' && starts(bytes, index, b"<!--"))
                || (bytes[index] == b'-' && js_html_close_comment(bytes, index))
            {
                let end = js_line_end(bytes, index + 3);
                self.add_comment(index, end, CommentKind::Line);
                index = end;
                continue;
            }
            if matches!(self.options.dialect, Dialect::Jsx | Dialect::Tsx)
                && regex_allowed
                && jsx_open(bytes, index)
            {
                index = self.scan_jsx_element(index, depth + 1);
                regex_allowed = false;
                pending_control_parenthesis = false;
                statement_start = false;
                pending_block = false;
                continue;
            }
            match bytes[index] {
                b'\'' | b'"' => {
                    index = self.js_quoted_or_error(index);
                    regex_allowed = false;
                    pending_control_parenthesis = false;
                    statement_start = false;
                    pending_block = false;
                }
                b'`' => {
                    index = self.scan_js_template(index, depth + 1);
                    regex_allowed = false;
                    pending_control_parenthesis = false;
                    statement_start = false;
                    pending_block = false;
                }
                b'/' if regex_allowed => {
                    if let Some(end) = js_regex_end(bytes, index) {
                        index = end;
                        regex_allowed = false;
                    } else {
                        index += 1;
                        regex_allowed = true;
                    }
                    pending_control_parenthesis = false;
                    statement_start = false;
                    pending_block = false;
                }
                b'{' => {
                    let is_block = pending_block || !regex_allowed || statement_start;
                    brace_blocks.push(is_block);
                    if stop_brace.is_some() {
                        brace_depth += 1;
                    }
                    index += 1;
                    regex_allowed = true;
                    pending_control_parenthesis = false;
                    statement_start = is_block;
                    pending_block = false;
                }
                b'}' => {
                    if stop_brace.is_some() {
                        brace_depth = brace_depth.saturating_sub(1);
                    }
                    index += 1;
                    if stop_brace.is_some() && brace_depth == 0 {
                        return index;
                    }
                    let is_block = brace_blocks.pop().unwrap_or(true);
                    regex_allowed = is_block;
                    pending_control_parenthesis = false;
                    statement_start = is_block;
                    pending_block = false;
                }
                byte if is_js_identifier_start(byte) || byte.is_ascii_digit() => {
                    let start = index;
                    index += 1;
                    while index < bytes.len() && is_js_identifier_continue(bytes[index]) {
                        index += 1;
                    }
                    let token = &bytes[start..index];
                    pending_control_parenthesis = is_js_control_keyword(token);
                    pending_block = matches!(token, b"else" | b"do" | b"try" | b"finally");
                    regex_allowed = pending_control_parenthesis
                        || matches!(
                            token,
                            b"return"
                                | b"throw"
                                | b"case"
                                | b"delete"
                                | b"void"
                                | b"typeof"
                                | b"yield"
                                | b"await"
                                | b"new"
                                | b"in"
                                | b"of"
                                | b"else"
                                | b"do"
                        );
                    statement_start = false;
                }
                b'(' => {
                    control_parentheses.push(pending_control_parenthesis);
                    pending_control_parenthesis = false;
                    index += 1;
                    regex_allowed = true;
                    statement_start = false;
                    pending_block = false;
                }
                b')' => {
                    let control = control_parentheses.pop().unwrap_or(false);
                    regex_allowed = control;
                    pending_control_parenthesis = false;
                    index += 1;
                    statement_start = control;
                    pending_block = control;
                }
                b']' => {
                    index += 1;
                    regex_allowed = false;
                    pending_control_parenthesis = false;
                    statement_start = false;
                    pending_block = false;
                }
                b'+' | b'-' if bytes.get(index + 1) == Some(&bytes[index]) => {
                    index += 2;
                    regex_allowed = false;
                    pending_control_parenthesis = false;
                    statement_start = false;
                    pending_block = false;
                }
                b'\r' | b'\n' => {
                    index = consume_newline(bytes, index);
                    if stop_brace.is_none()
                        && depth == 0
                        && regex_allowed
                        && statement_start
                        && !pending_control_parenthesis
                        && !pending_block
                        && control_parentheses.is_empty()
                        && brace_blocks.is_empty()
                    {
                        self.add_safe_checkpoint(index);
                    }
                }
                byte if js_is_space(byte) => index += 1,
                b'=' if bytes.get(index + 1) == Some(&b'>') => {
                    index += 2;
                    regex_allowed = true;
                    pending_control_parenthesis = false;
                    statement_start = true;
                    pending_block = true;
                }
                b';' => {
                    index += 1;
                    regex_allowed = true;
                    pending_control_parenthesis = false;
                    statement_start = true;
                    pending_block = false;
                }
                b':' => {
                    index += 1;
                    regex_allowed = true;
                    pending_control_parenthesis = false;
                    statement_start = brace_blocks.last().copied().unwrap_or(true);
                    pending_block = false;
                }
                _ => {
                    regex_allowed = true;
                    pending_control_parenthesis = false;
                    statement_start = false;
                    pending_block = false;
                    index += 1;
                }
            }
        }
        if stop_brace.is_some() {
            self.error(
                "unterminated-template-expression",
                "unterminated JavaScript template expression",
                ByteSpan::new(index, index),
            );
        }
        index
    }

    fn scan_jsx_element(&mut self, start: usize, depth: usize) -> usize {
        if depth > 256 {
            self.error(
                "nesting-limit",
                "JSX lexical nesting limit exceeded",
                ByteSpan::new(start, start),
            );
            return self.source.len();
        }
        let bytes = self.source;
        let mut index = start;
        let mut element_depth = 0usize;
        while index < bytes.len() {
            if bytes[index] == b'{' {
                index = self.scan_js_code(index + 1, Some(1), depth + 1);
                continue;
            }
            if bytes[index] != b'<' {
                index += 1;
                continue;
            }
            let closing = bytes.get(index + 1) == Some(&b'/');
            let opening = jsx_open(bytes, index);
            if !closing && !opening {
                index += 1;
                continue;
            }
            let mut cursor = index + if closing { 2 } else { 1 };
            let mut quote = None;
            let mut self_closing = false;
            let mut found_end = false;
            while cursor < bytes.len() {
                if let Some(active) = quote {
                    if bytes[cursor] == b'\\' {
                        cursor = (cursor + 2).min(bytes.len());
                    } else {
                        if bytes[cursor] == active {
                            quote = None;
                        }
                        cursor += 1;
                    }
                    continue;
                }
                match bytes[cursor] {
                    b'\'' | b'"' => {
                        quote = Some(bytes[cursor]);
                        cursor += 1;
                    }
                    b'{' if !closing => {
                        cursor = self.scan_js_code(cursor + 1, Some(1), depth + 1);
                    }
                    b'>' => {
                        let mut previous = cursor;
                        while previous > index && js_is_space(bytes[previous - 1]) {
                            previous -= 1;
                        }
                        self_closing = previous > index && bytes[previous - 1] == b'/';
                        cursor += 1;
                        found_end = true;
                        break;
                    }
                    _ => cursor += 1,
                }
            }
            if !found_end {
                self.error(
                    "unterminated-jsx-tag",
                    "unterminated JSX tag",
                    ByteSpan::new(index, bytes.len()),
                );
                return bytes.len();
            }
            index = cursor;
            if closing {
                element_depth = element_depth.saturating_sub(1);
                if element_depth == 0 {
                    return index;
                }
            } else if !self_closing {
                element_depth += 1;
            } else if element_depth == 0 {
                return index;
            }
        }
        self.error(
            "unterminated-jsx-element",
            "unterminated JSX element",
            ByteSpan::new(start, bytes.len()),
        );
        bytes.len()
    }

    fn scan_js_template(&mut self, start: usize, depth: usize) -> usize {
        let bytes = self.source;
        let mut index = start + 1;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index = (index + 2).min(bytes.len()),
                b'`' => return index + 1,
                b'$' if bytes.get(index + 1) == Some(&b'{') => {
                    index = self.scan_js_code(index + 2, Some(1), depth);
                }
                _ => index += 1,
            }
        }
        self.error(
            "unterminated-string",
            "unterminated JavaScript template literal",
            ByteSpan::new(start, index),
        );
        index
    }

    fn scan_html(&mut self) {
        let bytes = self.source;
        let mut index = 0;
        while index < bytes.len() && !self.stopped {
            if starts(bytes, index, b"<!--") {
                let end = if let Some(relative) = find_subslice(&bytes[index + 4..], b"-->") {
                    index + 4 + relative + 3
                } else {
                    self.error(
                        "unterminated-comment",
                        "unterminated HTML comment",
                        ByteSpan::new(index, bytes.len()),
                    );
                    bytes.len()
                };
                self.add_comment(index, end, CommentKind::HtmlComment);
                index = end;
                continue;
            }
            if bytes[index] == b'<' {
                if let Some((name, language)) = html_embedded_start(bytes, index) {
                    let Some(content_start) = html_tag_end(bytes, index) else {
                        self.error(
                            "unterminated-html-tag",
                            "unterminated HTML raw-text start tag",
                            ByteSpan::new(index, bytes.len()),
                        );
                        return;
                    };
                    let close = find_html_close(bytes, content_start, name);
                    let content_end = close.unwrap_or(bytes.len());
                    let slice = &bytes[content_start..content_end];
                    let mut child = Scanner::child(
                        slice,
                        language,
                        self.options.clone(),
                        self.offset + content_start,
                    );
                    if language == Language::JavaScript {
                        child.scan_javascript();
                    } else {
                        child.scan_c_family();
                    }
                    self.merge_child(child);
                    let Some(close) = close else {
                        self.error(
                            "unterminated-embedded-language",
                            "unterminated HTML script or style element",
                            ByteSpan::new(index, bytes.len()),
                        );
                        return;
                    };
                    let Some(element_end) = html_tag_end(bytes, close) else {
                        self.error(
                            "unterminated-html-tag",
                            "unterminated HTML raw-text end tag",
                            ByteSpan::new(close, bytes.len()),
                        );
                        return;
                    };
                    index = element_end;
                    continue;
                }
                if !html_tag_candidate(bytes, index) {
                    index += 1;
                } else if let Some(end) = html_tag_end(bytes, index) {
                    index = end;
                } else {
                    self.error(
                        "unterminated-html-tag",
                        "unterminated HTML tag",
                        ByteSpan::new(index, bytes.len()),
                    );
                    return;
                }
                continue;
            }
            if matches!(bytes[index], b'\r' | b'\n') {
                index = consume_newline(bytes, index);
                self.add_safe_checkpoint(index);
            } else {
                index += 1;
            }
        }
    }
}

/// The `keep_regex` and `remove_regex` sets of one [`ScanOptions`], compiled.
///
/// Compiling a regex set is far more expensive than matching against it, and
/// every comment scanned under one set of options is matched against the very
/// same two sets. A caller explaining a whole file compiles them once here and
/// hands them to [`explain_disposition_with`] for each of its comments.
#[derive(Clone)]
pub struct DispositionPatterns {
    keep: RegexSet,
    remove: RegexSet,
}

impl DispositionPatterns {
    /// The two sets `options` asks for, or the error the first pattern that
    /// would not compile raised.
    pub fn compile(options: &ScanOptions) -> Result<Self, regex::Error> {
        Ok(Self {
            keep: RegexSet::new(&options.keep_regex)?,
            remove: RegexSet::new(&options.remove_regex)?,
        })
    }

    /// Sets that match nothing: what a pattern list that will not compile falls
    /// back to. The scanner reports such a list as a diagnostic and then scans
    /// as though it were empty, so an explanation has to ignore it the same way
    /// to stay in step with the verdict it is accounting for.
    pub fn empty() -> Self {
        Self {
            keep: RegexSet::empty(),
            remove: RegexSet::empty(),
        }
    }
}

pub(crate) fn disposition(
    kind: CommentKind,
    options: &ScanOptions,
    raw: &[u8],
    patterns: &DispositionPatterns,
) -> Disposition {
    if options.keep_kinds.contains(&kind) || patterns.keep.is_match(raw) {
        return Disposition::Keep {
            reason: "kept by kind or regex override".into(),
        };
    }
    let hard = matches!(kind, CommentKind::Shebang | CommentKind::Encoding);
    if hard && !options.force_protected {
        return Disposition::Keep {
            reason: "required source preamble".into(),
        };
    }
    if options.remove_kinds.contains(&kind) || patterns.remove.is_match(raw) {
        return Disposition::Remove;
    }
    if options.policy == Policy::All {
        return Disposition::Remove;
    }
    if kind == CommentKind::HtmlComment {
        return Disposition::Keep {
            reason: "HTML comments are DOM-observable".into(),
        };
    }
    if matches!(
        kind,
        CommentKind::Directive | CommentKind::OptimizerHint | CommentKind::VersionComment
    ) {
        return Disposition::Keep {
            reason: "tool or language directive".into(),
        };
    }
    if kind == CommentKind::License && options.policy == Policy::Legal {
        return Disposition::Keep {
            reason: "legal policy".into(),
        };
    }
    Disposition::Remove
}

/// The index and text of the first pattern in `set` that matches `raw`.
///
/// `set` was compiled from `sources` in order, so the index addresses both.
fn first_match(set: &RegexSet, raw: &[u8], sources: &[String]) -> Option<(usize, String)> {
    let index = set.matches(raw).iter().next()?;
    let pattern = sources.get(index).cloned().unwrap_or_default();
    Some((index, pattern))
}

/// Recover the directive name of an already-classified comment from its bytes,
/// exactly as [`classify_comment`] found it.
fn directive_name_of(raw: &[u8], language: Language) -> Option<&'static str> {
    let lower = String::from_utf8_lossy(strip_comment_markers(raw)).to_ascii_lowercase();
    directive_name(lower.trim(), language, raw)
}

/// Recover the legal marker of an already-classified comment from its bytes,
/// exactly as [`classify_comment`] found it.
fn legal_marker_of(raw: &[u8]) -> Option<&'static str> {
    let lower = String::from_utf8_lossy(strip_comment_markers(raw)).to_ascii_lowercase();
    legal_marker(lower.trim())
}

/// Name the rule that decides this comment's fate.
///
/// The branches below are the branches of `disposition()` in the same order,
/// so `explain_disposition(..).action().is_remove()` always equals
/// `disposition(..).is_remove()` for the same comment and options. An
/// unparseable pattern list is ignored here as the scanner ignores it, which
/// keeps the two in step even on input the scanner has already flagged.
///
/// That agreement is with the bytes-only rule table and with nothing else. A
/// scan applies one rule no reading of `raw` can reach —
/// [`DispositionExplanation::KeptStructural`], where a YAML block scalar leans
/// on the comment that ends it — and a comment kept by *where it sits* is
/// reported here as the table alone would have it. That verdict comes only from
/// [`explain_comment`], which is handed the [`Comment`] a scan produced.
///
/// `raw` is the comment's complete bytes, delimiters included, exactly as
/// [`Comment::span`](crate::Comment::span) delimits them.
///
/// # Examples
///
/// ```
/// use ocomment_core::{
///     Action, CommentKind, DispositionExplanation, Language, Policy, ScanOptions,
///     explain_disposition,
/// };
///
/// let mut options = ScanOptions::default();
/// let why = explain_disposition(CommentKind::Line, b"// note", Language::Rust, &options);
/// assert_eq!(why.action(), Action::Remove);
/// assert!(matches!(why, DispositionExplanation::RemovedByDefault(Policy::Safe)));
///
/// options.keep_regex.push(r"^//\s*NOTE\b".into());
/// let kept = explain_disposition(CommentKind::Line, b"// NOTE: why", Language::Rust, &options);
/// assert_eq!(kept.action(), Action::Keep);
/// assert!(matches!(kept, DispositionExplanation::KeptByRegex { index: 0, .. }));
/// assert_eq!(kept.to_string(), r"kept: matched keep_regex #0 `^//\s*NOTE\b`");
/// ```
pub fn explain_disposition(
    kind: CommentKind,
    raw: &[u8],
    language: Language,
    options: &ScanOptions,
) -> DispositionExplanation {
    let patterns =
        DispositionPatterns::compile(options).unwrap_or_else(|_| DispositionPatterns::empty());
    explain_disposition_with(&patterns, kind, raw, language, options)
}

/// The same answer, against pattern sets the caller already compiled.
///
/// [`explain_disposition`] compiles `options.keep_regex` and
/// `options.remove_regex` on every call, which is once per comment for a caller
/// explaining a file. `patterns` must be [`DispositionPatterns::compile`] of the
/// same `options` — or [`DispositionPatterns::empty`] where that compile failed,
/// which is what the wrapper falls back to — and the two functions then return
/// the identical explanation.
pub fn explain_disposition_with(
    patterns: &DispositionPatterns,
    kind: CommentKind,
    raw: &[u8],
    language: Language,
    options: &ScanOptions,
) -> DispositionExplanation {
    if options.keep_kinds.contains(&kind) {
        return DispositionExplanation::KeptByKind(kind);
    }
    if let Some((index, pattern)) = first_match(&patterns.keep, raw, &options.keep_regex) {
        return DispositionExplanation::KeptByRegex { index, pattern };
    }
    let hard = matches!(kind, CommentKind::Shebang | CommentKind::Encoding);
    if hard && !options.force_protected {
        return DispositionExplanation::ProtectedPreamble;
    }
    if options.remove_kinds.contains(&kind) {
        return DispositionExplanation::RemovedByKind(kind);
    }
    if let Some((index, pattern)) = first_match(&patterns.remove, raw, &options.remove_regex) {
        return DispositionExplanation::RemovedByRegex { index, pattern };
    }
    if options.policy == Policy::All {
        return DispositionExplanation::RemovedByPolicy(options.policy);
    }
    if kind == CommentKind::HtmlComment {
        return DispositionExplanation::KeptHtml;
    }
    if matches!(
        kind,
        CommentKind::Directive | CommentKind::OptimizerHint | CommentKind::VersionComment
    ) {
        return DispositionExplanation::KeptDirective {
            kind,
            name: directive_name_of(raw, language),
        };
    }
    if kind == CommentKind::License && options.policy == Policy::Legal {
        return DispositionExplanation::KeptLicense {
            marker: legal_marker_of(raw),
        };
    }
    DispositionExplanation::RemovedByDefault(options.policy)
}

/// Name the rule that decided the fate of a comment a scan actually found.
///
/// [`explain_disposition`] accounts for every rule a comment's own bytes can
/// trigger. One rule is not one of those: a YAML block scalar leaning on the
/// comment that ends it keeps that comment because of where it sits, and no
/// amount of reading its bytes could say so. This is that answer, and for every
/// other comment it is exactly [`explain_disposition`].
///
/// `comment` must be one the scan of `raw`'s file produced, and `raw` its
/// complete bytes as [`Comment::span`](crate::Comment::span) delimits them.
///
/// # Examples
///
/// ```
/// use ocomment_core::{
///     Action, DispositionExplanation, Language, ScanOptions, explain_comment, scan,
/// };
///
/// let source = b"k: |\n  a\n# ends the block\n  # yamllint disable\nz: 1\n";
/// let report = scan(source, Language::Yaml, ScanOptions::default());
/// let comment = &report.comments[0];
/// let why = explain_comment(
///     comment,
///     &source[comment.span.start..comment.span.end],
///     Language::Yaml,
///     &ScanOptions::default(),
/// );
/// assert_eq!(why.action(), Action::Keep);
/// assert!(matches!(
///     why,
///     DispositionExplanation::KeptStructural { language: Language::Yaml }
/// ));
/// ```
pub fn explain_comment(
    comment: &Comment,
    raw: &[u8],
    language: Language,
    options: &ScanOptions,
) -> DispositionExplanation {
    let patterns =
        DispositionPatterns::compile(options).unwrap_or_else(|_| DispositionPatterns::empty());
    explain_comment_with(&patterns, comment, raw, language, options)
}

/// The same answer, against pattern sets the caller already compiled, as
/// [`explain_disposition_with`] is to [`explain_disposition`].
pub fn explain_comment_with(
    patterns: &DispositionPatterns,
    comment: &Comment,
    raw: &[u8],
    language: Language,
    options: &ScanOptions,
) -> DispositionExplanation {
    if is_yaml_structural_trail(&comment.disposition) {
        return DispositionExplanation::KeptStructural { language };
    }
    explain_disposition_with(patterns, comment.kind, raw, language, options)
}

fn java_text_block_end(source: &[u8], start: usize) -> (usize, bool) {
    let mut index = start.saturating_add(3);
    while index + 2 < source.len() {
        if starts(source, index, b"\"\"\"") {
            let mut backslashes = 0;
            let mut cursor = index;
            while cursor > start + 3 && source[cursor - 1] == b'\\' {
                backslashes += 1;
                cursor -= 1;
            }
            if backslashes % 2 == 0 {
                return (index + 3, true);
            }
        }
        index += 1;
    }
    (source.len(), false)
}

/// How many bytes of UTF-8 byte order mark `source` opens with: three, or none.
///
/// A BOM is consumed before the first line is read — CPython's `check_bom`,
/// Lua's `skipBOM` — so the line behind one is still the first line, and a
/// preamble rule that asked for byte 0 alone would miss it. The bytes stay
/// where they are; only the question `is this the first line?` skips them.
/// [`is_encoding_declaration`] has always skipped the same three.
fn byte_order_mark_width(source: &[u8]) -> usize {
    if source.starts_with(b"\xef\xbb\xbf") {
        3
    } else {
        0
    }
}

fn classify_comment(
    source: &[u8],
    language: Language,
    lexical: CommentKind,
    start: usize,
    end: usize,
    offset: usize,
) -> CommentKind {
    let raw = &source[start.min(source.len())..end.min(source.len())];
    let body = strip_comment_markers(raw);
    let lower = String::from_utf8_lossy(body).to_ascii_lowercase();
    let trimmed = lower.trim();
    if offset == 0 && start == byte_order_mark_width(source) && raw.starts_with(b"#!") {
        return CommentKind::Shebang;
    }
    if offset == 0
        && matches!(language, Language::Python | Language::Ruby)
        && is_encoding_declaration(source, start, raw)
    {
        return CommentKind::Encoding;
    }
    if language == Language::Sql && raw.starts_with(b"/*+") {
        return CommentKind::OptimizerHint;
    }
    if raw.starts_with(b"/*!") && language == Language::Sql {
        return CommentKind::VersionComment;
    }
    if legal_marker(trimmed).is_some() {
        return CommentKind::License;
    }
    if directive_name(trimmed, language, raw).is_some() {
        return CommentKind::Directive;
    }
    lexical
}

/// The document-wide half of the restart rules: C and C++ splice
/// `\<newline>` out of the input before lexing, and the remapped copy that
/// results is scanned without tracking checkpoints, so a full scan of a spliced
/// document offers no restart point beyond offset 0.
fn line_splicing_permits_restarts(source: &[u8], language: Language) -> bool {
    !matches!(language, Language::C | Language::Cpp) || !contains_line_splice(source)
}

/// The offset of the first byte in `source` that could head a YAML block
/// scalar, or [`usize::MAX`] when there is none — and for every other language,
/// which has no such construct.
///
/// This is the one lexical state whose *end* is decided by the bytes that come
/// after it: a body runs while the lines below stay indented past the node it
/// hangs off, so an edit that indents the line under a body, or appends one to
/// a document that ended with it, swallows an offset a previous revision
/// recorded as a line start. Restarting there would read the content of a
/// scalar as YAML and remove a `#` that is one of its bytes. No body can begin
/// before its own header, so every line start up to the first one is safe from
/// that whatever an edit does below it — and past it, nothing is.
///
/// The test is deliberately looser than [`Scanner::scan_yaml`]'s: any `|` or
/// `>` with the shape of a header counts, whether or not a node may begin
/// there. Refusing a restart costs a rescan; permitting a wrong one loses a
/// user's bytes.
///
/// One pass, not one per candidate: [`yaml_block_header`] reads the comment on
/// a header line to its end, but a comment is enough to make the bytes a
/// header, so that read happens at most once before this returns.
fn first_yaml_block_scalar(source: &[u8], language: Language) -> usize {
    if language != Language::Yaml {
        return usize::MAX;
    }
    let mut index = 0;
    while let Some(relative) = memchr2(b'|', b'>', &source[index..]) {
        let candidate = index + relative;
        if yaml_block_header(source, candidate).is_some() {
            return candidate;
        }
        index = candidate + 1;
    }
    usize::MAX
}

/// A checkpoint sits immediately after a line terminator, and a CRLF pair is a
/// single terminator. An edit that supplies the LF after an existing CR moves
/// the boundary one byte on, leaving the offset a previous revision recorded
/// inside the pair, where no scan of these bytes would ever resume.
fn the_line_ending_permits_a_restart(source: &[u8], offset: usize) -> bool {
    offset == 0 || source.get(offset - 1) != Some(&b'\r') || source.get(offset) != Some(&b'\n')
}

/// Preamble classification depends on the absolute offset, and the two
/// languages that declare a source encoding in a comment — Python and Ruby —
/// only recognise one while scanning from offset 0, which makes the start of
/// line 2 a restart point exactly when no encoding declaration follows. Offset
/// 0 always passes — restarting a scan there *is* the full scan.
fn the_preamble_permits_a_restart(source: &[u8], language: Language, offset: usize) -> bool {
    offset == 0
        || !matches!(language, Language::Python | Language::Ruby)
        || !is_within_first_two_lines(source, offset)
        || !line_declares_encoding(source, offset)
}

/// The restart rules for one revision of a document: a safe checkpoint promises
/// that restarting the scan there reproduces the rest of a full scan byte for
/// byte, and both halves of that promise are conditions on the bytes *around*
/// the checkpoint. The document-wide half is answered once here, when the rules
/// are built, because answering it costs a scan of the source.
///
/// The scanner consults these rules before emitting a checkpoint; the
/// incremental engine builds them for the *edited* bytes and consults them
/// again before restarting at a checkpoint the previous revision recorded.
/// Emitting a checkpoint and reusing one therefore ask one function and cannot
/// drift apart.
#[derive(Clone, Copy)]
pub(crate) struct RestartRules {
    language: Language,
    splicing_permits_restarts: bool,
    first_block_scalar: usize,
}

impl RestartRules {
    pub(crate) fn of(source: &[u8], language: Language) -> Self {
        Self {
            language,
            splicing_permits_restarts: line_splicing_permits_restarts(source, language),
            first_block_scalar: first_yaml_block_scalar(source, language),
        }
    }

    /// Whether restarting a scan of `source` — the bytes these rules were built
    /// from — at `offset` reproduces the rest of a full scan of it.
    pub(crate) fn permit_restart_at(&self, source: &[u8], offset: usize) -> bool {
        self.splicing_permits_restarts
            && offset <= self.first_block_scalar
            && the_line_ending_permits_a_restart(source, offset)
            && the_preamble_permits_a_restart(source, self.language, offset)
            && (self.language != Language::Scala
                || the_scala_xml_boundary_permits_a_restart(source, offset))
    }
}

/// Whether a Scala restart at `offset` may stand next to the byte there.
///
/// The XML literal's trigger reads the byte *behind* its `<`: the lexer's
/// `XMLSTART` needs a space, tab, line feed, `{`, `(` or `>` before it and an
/// XML name start, `!` or `?` after it, and a suffix scan begins at the
/// checkpoint with no byte behind it to read. A line that opens with a `<`
/// that could begin a literal is therefore exactly where a checkpoint cannot
/// stand: the full scan would read the literal and protect its text, and the
/// rescan from the checkpoint could not know it was a literal and would remove
/// a comment out of it. Refusing the checkpoint costs a rescan; permitting it
/// loses a user's bytes.
fn the_scala_xml_boundary_permits_a_restart(source: &[u8], offset: usize) -> bool {
    if source.get(offset) != Some(&b'<') {
        return true;
    }
    match source.get(offset + 1) {
        Some(b'!' | b'?') => false,
        Some(&byte) => !(byte.is_ascii_alphabetic() || matches!(byte, b'_') || byte >= 0x80),
        None => true,
    }
}

/// Whether classification from `offset` onwards is independent of where those
/// bytes sit in the document. The preamble rules are the only position
/// sensitive ones — a `#!` line is a shebang only at offset 0, and a Python
/// encoding declaration only inside the first two lines — so anything past the
/// first two lines classifies the same wherever an edit moves it to.
///
/// The incremental engine reuses the previous revision's report for the tail it
/// converges on, shifted by the edit's length delta. That reuse keeps the old
/// classification, so it is sound exactly while the tail is settled at both its
/// old and its new position.
pub(crate) fn preamble_is_settled(source: &[u8], offset: usize) -> bool {
    !is_within_first_two_lines(source, offset)
}

fn is_within_first_two_lines(source: &[u8], offset: usize) -> bool {
    let mut line_breaks = 0;
    let mut index = 0;
    let end = offset.min(source.len());
    while index < end {
        if source[index] == b'\r' {
            index += usize::from(source.get(index + 1) == Some(&b'\n'));
            line_breaks += 1;
            if line_breaks >= 2 {
                return false;
            }
        } else if source[index] == b'\n' {
            line_breaks += 1;
            if line_breaks >= 2 {
                return false;
            }
        }
        index += 1;
    }
    true
}

/// Whether the line beginning at `line_start` carries a source-encoding
/// declaration, and therefore a comment whose classification depends on the
/// scan starting at offset 0.
fn line_declares_encoding(source: &[u8], line_start: usize) -> bool {
    let mut index = line_start;
    while matches!(source.get(index), Some(b' ' | b'\t' | 0x0c)) {
        index += 1;
    }
    if source.get(index) != Some(&b'#') {
        return false;
    }
    let end = line_end(source, index + 1);
    is_encoding_declaration(source, index, &source[index..end])
}

/// Whether the comment beginning at `start` is a source-encoding declaration.
///
/// Python and Ruby share the phrase, down to the spelling: PEP 263 asks for
/// `coding[:=]\s*([-\w.]+)` in one of the first two lines, and Ruby's
/// `magic_comment` reads the same phrase out of the same two lines. The Emacs
/// form `# -*- coding: utf-8 -*-` satisfies both, which is why both languages
/// are written with it.
///
/// What the two do *not* share is which second line counts, and the rule here
/// is neither of theirs: any `coding:` comment on either of the first two lines
/// is a declaration, whatever stands on the line above it. Ruby reads the
/// second line only behind a `#!` line, and Python only behind a line that is
/// itself a comment or blank — so `x = 1\n# coding: us-ascii\n` names an
/// encoding to neither of them (Ruby 3.3.12 reports `__ENCODING__` as UTF-8,
/// and `tokenize.detect_encoding` reports utf-8), and this function calls it a
/// declaration all the same.
///
/// That is deliberate. Saying yes here only ever *keeps* a comment that `safe`
/// would otherwise remove, so the two ways to be wrong are not the same size:
/// a missed declaration removes the line a file's encoding is written on, and
/// an invented one leaves an ordinary comment in place. One rule for both
/// languages is also one rule that cannot drift apart between them, which is
/// what [`preamble_is_settled`] leans on when it names the first two lines the
/// only position-sensitive bytes in any document.
fn is_encoding_declaration(source: &[u8], start: usize, raw: &[u8]) -> bool {
    if !is_within_first_two_lines(source, start) || !raw.starts_with(b"#") {
        return false;
    }
    let line_start = source[..start.min(source.len())]
        .iter()
        .rposition(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(0, |position| position + 1);
    let mut prefix = &source[line_start..start.min(source.len())];
    if line_start == 0 {
        prefix = prefix.strip_prefix(b"\xef\xbb\xbf").unwrap_or(prefix);
    }
    if !prefix
        .iter()
        .all(|byte| matches!(byte, b' ' | b'\t' | 0x0c))
    {
        return false;
    }
    let body = &raw[1..];
    let Some(position) = find_subslice(body, b"coding") else {
        return false;
    };
    let mut cursor = position + b"coding".len();
    if !matches!(body.get(cursor), Some(b':' | b'=')) {
        return false;
    }
    cursor += 1;
    while matches!(body.get(cursor), Some(b' ' | b'\t')) {
        cursor += 1;
    }
    body.get(cursor)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn strip_comment_markers(raw: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = raw.len();
    for marker in [
        b"<!--".as_slice(),
        b"///",
        b"//!",
        b"//",
        b"/**",
        b"/*",
        b"(*",
        b"--",
        b"#",
    ] {
        if raw.starts_with(marker) {
            start = marker.len();
            break;
        }
    }
    for marker in [b"-->".as_slice(), b"*/", b"*)"] {
        if raw.ends_with(marker) {
            end = end.saturating_sub(marker.len());
            break;
        }
    }
    &raw[start.min(end)..end]
}

/// The legal marker `text` carries, or `None` when it carries none. The marker
/// is the phrase that matched, which is what an explanation has to quote to
/// justify calling a comment a license.
fn legal_marker(text: &str) -> Option<&'static str> {
    [
        "spdx-license-identifier",
        "copyright",
        "licensed under",
        "permission is hereby granted",
        "all rights reserved",
    ]
    .into_iter()
    .find(|marker| text.contains(marker))
}

/// The tool or language directive `text` opens, or `None` when it opens none.
/// The name is the prefix that matched, so an explanation can point at the
/// directive a reader recognises instead of at the whole comment.
fn directive_name(text: &str, language: Language, raw: &[u8]) -> Option<&'static str> {
    let compact = text.trim_start_matches(['!', '/', '*', '#', '@', ' ']);
    let common = [
        "sourcemappingurl=",
        "sourceurl=",
        "#__pure__",
        "@__pure__",
        "__pure__",
        "#__no_side_effects__",
        "__no_side_effects__",
        "ts-ignore",
        "ts-expect-error",
        "ts-nocheck",
        "ts-check",
        "eslint",
        "prettier-ignore",
        "stylelint",
        "noinspection",
        "nolint",
        "noqa",
        "type: ignore",
        "fmt:",
        "rustfmt::",
        "clang-format",
        "spotless:",
        "ktlint-disable",
        "ktlint-enable",
        "detekt:",
        "istanbul ignore",
        "c8 ignore",
        "coverage:",
        "ocomment:",
        "region",
        "endregion",
    ];
    if let Some(name) = common
        .into_iter()
        .find(|prefix| compact.starts_with(prefix))
    {
        return Some(name);
    }
    /* NOTE: `shellcheck` is out of the list above for one reason: it is the whole
     * word the tool answers to rather than the head of a longer one, so it
     * ends at a boundary instead of at a byte. Every language is still asked
     * about it, because a shell fragment is embedded in more than one of
     * them. */
    if opens_with_keyword(compact, "shellcheck") {
        return Some("shellcheck");
    }
    match language {
        Language::Go => ["go:", "+build", "line "]
            .into_iter()
            .find(|prefix| compact.starts_with(prefix)),
        Language::TypeScript => {
            (raw.starts_with(b"///") && compact.starts_with('<')).then_some("///")
        }
        Language::C | Language::Cpp => ["pragma", "line "]
            .into_iter()
            .find(|prefix| compact.starts_with(prefix)),
        Language::Python => ["pyright:", "mypy:", "ruff:", "fmt:"]
            .into_iter()
            .find(|prefix| compact.starts_with(prefix)),
        /* NOTE: A Dockerfile is detected as shell, and two of its comment lines are
         * addressed to a tool: `# syntax=` is the parser directive BuildKit
         * reads before it reads the file, and `# hadolint ignore=` turns one
         * rule of the Dockerfile linter off for the instruction below it.
         * `hadolint` is a whole word and `syntax=` is not — the frontend
         * reference follows the `=` with no space at all — so only the first
         * of the two is matched with a boundary after it. */
        Language::Shell => opens_with_keyword(compact, "hadolint")
            .then_some("hadolint")
            .or_else(|| compact.starts_with("syntax=").then_some("syntax=")),
        /* NOTE: Taplo reads two instructions out of a TOML comment: `#:schema`
         * names the JSON schema the file is validated against, and `# taplo:`
         * opens a formatter option. The first is followed by the URL after
         * whitespace, so it ends at a boundary; the second carries its own in
         * the colon. */
        Language::Toml => opens_with_keyword(compact, ":schema")
            .then_some(":schema")
            .or_else(|| compact.starts_with("taplo:").then_some("taplo:")),
        /* NOTE: `strip_comment_markers` takes the `--` off a Lua comment and
         * leaves the third dash of `---@diagnostic` behind, so the annotation
         * is read with that dash removed. `raw` is what tells it from prose:
         * the language server's annotations are the only comments that open
         * with `---@`, and `diagnostic` is the only one of them that instructs
         * a tool rather than describing a type. The four checkers below are
         * addressed as `-- <tool>:`, which carries its own boundary in the
         * colon. */
        Language::Lua => {
            if raw.starts_with(b"---@") && text.trim_start_matches('-').starts_with("@diagnostic") {
                return Some("---@diagnostic");
            }
            ["luacheck:", "selene:", "stylua:", "luacov:"]
                .into_iter()
                .find(|prefix| compact.starts_with(prefix))
        }
        /* NOTE: `@schema` is asked of `text` rather than of `compact`, because
         * `compact` is what takes the `@` off: the annotation the Helm schema
         * generator reads is spelled with it, and `schema` on its own is a
         * word any comment about a schema opens with. The three keywords below
         * it are the whole word their tool answers to and end at a boundary;
         * the four prefixes carry their own in a colon. */
        Language::Yaml => {
            if opens_with_keyword(text, "@schema") {
                return Some("@schema");
            }
            for keyword in ["yamllint", "nosec", "kics-scan"] {
                if opens_with_keyword(compact, keyword) {
                    return Some(keyword);
                }
            }
            [
                "yaml-language-server:",
                "renovate:",
                "checkov:skip",
                "trivy:ignore",
            ]
            .into_iter()
            .find(|prefix| compact.starts_with(prefix))
        }
        /* NOTE: Three of the four are asked of `text` rather than of `compact`,
         * because `compact` is what takes the `@` off, and the `@` is what
         * tells the annotation from prose about it. `@psalm-suppress` is
         * followed by the issue it silences after whitespace, so it ends at a
         * boundary; `@phpstan-ignore` and `@codeCoverageIgnore` are namespaces
         * whose members differ only in what runs on past them —
         * `-next-line`, `Start`, `End` — so a prefix is the whole rule there.
         * `phpcs:` carries its own boundary in the colon and covers `ignore`,
         * `disable`, `enable`, and `ignoreFile` alike. */
        /* NOTE: Three of these six are Ruby's own magic comments, which the
         * interpreter reads out of the head of a file: `frozen_string_literal`
         * decides whether every literal string in it is frozen,
         * `shareable_constant_value` what Ractor may share, and `warn_indent`
         * whether the parser complains about the indentation. The other three
         * are the tools every Ruby project runs — RuboCop, StandardRB, and
         * Sorbet's `# typed:` sigil. Each carries its own boundary in the
         * colon and covers the whole namespace behind it: `rubocop:disable`,
         * `:enable` and `:todo` alike. The encoding declaration is deliberately
         * absent: it is a kind of its own, classified before this runs.
         *
         * A magic comment is honoured only at the head of a file, and this is
         * asked of every comment in it. Reading one further down as an
         * instruction keeps a comment a removal would otherwise take, which is
         * the direction to be wrong in, and it is what keeps the answer
         * independent of where in the document the scan began. */
        Language::Ruby => [
            "frozen_string_literal:",
            "warn_indent:",
            "shareable_constant_value:",
            "rubocop:",
            "standard:",
            "typed:",
        ]
        .into_iter()
        .find(|prefix| compact.starts_with(prefix)),
        Language::Php => {
            if opens_with_keyword(text, "@psalm-suppress") {
                return Some("@psalm-suppress");
            }
            if text.starts_with("@phpstan-ignore") {
                return Some("@phpstan-ignore");
            }
            if text.starts_with("@codecoverageignore") {
                return Some("@codeCoverageIgnore");
            }
            compact.starts_with("phpcs:").then_some("phpcs:")
        }
        /* NOTE: `zig fmt` reads one instruction out of a comment, and it reads it
         * by equality rather than by prefix: `Render.zig` takes `"//".len()`
         * bytes off the trimmed comment, trims the white space that follows,
         * and compares the remainder with `zig fmt: off` and `zig fmt: on`.
         * So `// zig fmt: off please` turns nothing off, and neither does
         * `/// zig fmt: off` or `//// zig fmt: off` — the first leaves a `/`
         * in front of the phrase and the second two. `raw` is what tells those
         * apart, because `strip_comment_markers` takes a `///` off whole; the
         * comparison itself is against the trimmed text, which is folded to
         * lower case here where `zig fmt` is case-sensitive. Folding can only
         * keep a comment a removal would otherwise take, which is the
         * direction to be wrong in. */
        /* NOTE: The two comments an R tool reads rather than a reader. styler
         * turns its formatter off between `# styler: off` and `# styler: on`,
         * and the colon carries the marker's own boundary; covr excludes the
         * lines between `# nocov start` and `# nocov end`, and `nocov` is the
         * whole word it looks for — `start`, `end` and nothing at all all
         * follow it — so that one ends at a boundary instead. lintr's
         * `# nolint` is protected for every language already and is deliberately
         * absent here. */
        Language::R => {
            if opens_with_keyword(compact, "nocov") {
                return Some("nocov");
            }
            compact.starts_with("styler:").then_some("styler:")
        }
        Language::Zig => {
            let opens_a_plain_comment =
                raw.starts_with(b"//") && !matches!(raw.get(2), Some(b'/' | b'!'));
            (opens_a_plain_comment && matches!(text, "zig fmt: off" | "zig fmt: on"))
                .then_some("zig fmt:")
        }
        /* NOTE: Four instructions, and only one of them is addressed to a tool.
         * `// @dart = 2.12` is read by the Dart scanner itself, and it decides
         * which version of the language the file is written in, so a removal
         * that took it would change what the remaining code means
         * ([`dart_language_version`] follows that grammar). `dart format` is
         * matched by equality on the whole comment rather than by prefix,
         * because that is how `dart_style` matches it: `piece_writer.dart`
         * switches on `comment.text` against `// dart format off` and
         * `// dart format on`, so `//   dart format off` with a second space
         * and `/// dart format off` with a third slash turn nothing off —
         * measured on `dart format` from SDK 3.13.2, which reformatted both.
         * `comment.text` is trimmed at the end and not at the front, and this
         * is asked of `raw` for the reason Zig's is: `strip_comment_markers`
         * takes a `///` off whole and would leave the two spellings
         * indistinguishable. The analyzer's two ignore comments each carry
         * their own boundary in the colon and cover the whole namespace behind
         * it (`ignore_info.dart`). */
        Language::Dart => {
            if dart_language_version(raw) {
                return Some("@dart");
            }
            let phrase = raw.trim_ascii_end();
            if phrase == b"// dart format off" || phrase == b"// dart format on" {
                return Some("dart format");
            }
            ["ignore:", "ignore_for_file:"]
                .into_iter()
                .find(|prefix| compact.starts_with(prefix))
        }
        /* NOTE: Four instructions, and only one of them is addressed to a
         * formatter. `// swift-tools-version:` is the first line of a
         * `Package.swift`, and SwiftPM reads it before it reads any of the
         * manifest: it decides which version of the package description the
         * file is written against, so a removal that took it would leave a
         * package that no longer builds. The other three name the tool that
         * reads them and carry their own boundary — a colon for `swiftlint:`
         * and `swiftformat:`, and for `swift-format-ignore` the end of the
         * comment, a colon, or the `-file` that widens it to the whole file.
         * Measured on `swift-format` 6.3.3: `// swift-format-ignore` and
         * `// swift-format-ignore-file` both leave `let    a     = 1` alone,
         * and `// swift-format-ignoreish note` reformats it. `// MARK:` is
         * deliberately absent: Xcode reads it to build a jump bar, so it is
         * addressed to a reader rather than to a build, and a project that
         * wants it kept says so with a `keep_regex`. */
        Language::Swift => {
            if let Some(name) = ["swift-tools-version:", "swiftlint:", "swiftformat:"]
                .into_iter()
                .find(|prefix| compact.starts_with(prefix))
            {
                return Some(name);
            }
            let rest = compact.strip_prefix("swift-format-ignore")?;
            let tail = rest.strip_prefix("-file").unwrap_or(rest);
            (tail.is_empty()
                || tail.starts_with(':')
                || tail.starts_with(|character: char| character.is_ascii_whitespace()))
            .then_some("swift-format-ignore")
        }
        /* NOTE: Three instructions, and the first is the one a *compiler* reads.
         * Roslyn's `GeneratedCodeUtilities.BeginsWithAutoGeneratedComment`
         * searches the `//` and `/* */` comments in front of a file's first
         * token for `<auto-generated` — the legacy `<autogenerated` with it —
         * and a file it finds one in is exempt from every analyzer that opts
         * out of generated code, so a removal that took it would turn a
         * generated file into a hand-written one and light up the diagnostics
         * it was written to escape. That search is `contains` rather than a
         * prefix, and it is followed here: the `<` is the marker's own
         * boundary, and prose about generated code carries none. Roslyn asks
         * for it in the leading trivia alone and asks case-sensitively; both
         * are widened here, because a comment that merely *reads* like the
         * marker is a comment a reader meant as one.
         *
         * `// ReSharper disable` and `// ReSharper restore` bound the region an
         * inspection is turned off over, and only those two verbs are
         * instructions — the whitespace between the tool and its verb is what
         * tells them from prose that opens with the same letters.
         * `// csharpier-ignore` is matched on the whole comment rather than by
         * prefix, because that is how CSharpier matches it: measured on
         * `csharpier` 1.3.0, `// csharpier-ignore`, `// csharpier-ignore-start`
         * and `// csharpier-ignore-end` each left `int    a     =    1;`
         * unformatted, while `//  csharpier-ignore` with a second space,
         * `// csharpier-ignore some text`, `/* csharpier-ignore */` and
         * `/// csharpier-ignore` all reformatted it. */
        Language::CSharp => {
            if text.contains("<auto-generated") || text.contains("<autogenerated") {
                return Some("<auto-generated");
            }
            if let Some(rest) = compact.strip_prefix("resharper") {
                let verb = rest.trim_start_matches([' ', '\t']);
                if verb.len() < rest.len()
                    && (verb.starts_with("disable") || verb.starts_with("restore"))
                {
                    return Some("ReSharper");
                }
            }
            matches!(
                raw,
                b"// csharpier-ignore" | b"// csharpier-ignore-start" | b"// csharpier-ignore-end"
            )
            .then_some("csharpier-ignore")
        }
        /* NOTE: scala-cli reads a directive line before it reads the manifest
         * at all, and the directive is `//>` followed by a space and a name,
         * of which `using` is the one that configures the build. `compact` is
         * the comment with its markers stripped, so `//> using` is `> using`,
         * and the boundary is what keeps a comment that only opens with the
         * same letters — `//> usingless`, or `//>> using` with one `>` more —
         * from being kept as one. */
        Language::Scala => (compact == "> using"
            || compact.starts_with("> using ")
            || compact.starts_with("> using\t"))
        .then_some("//> using"),
        _ => None,
    }
}

/// Whether `text` opens with `keyword` and then ends it.
///
/// A directive named after the tool that reads it — `shellcheck`, `hadolint` —
/// is followed by the argument that tool takes, and what separates the two is
/// whitespace of the writer's choosing rather than one particular byte:
/// `# hadolint\tignore=DL3018` is the same instruction as the one written with
/// a space. Matching the bare prefix instead would read prose that merely opens
/// with those letters — `# shellcheckish note` — as an instruction as well, and
/// protect a comment that is only *about* the tool.
///
/// The end of the comment ends the keyword too. `#:schema` with its URL still
/// to be typed is the directive it is about to be, and `text` arrives trimmed,
/// so `#:schema ` reaches here as the bare word in any case: refusing the empty
/// remainder would protect the directive or not depending on a trailing space.
fn opens_with_keyword(text: &str, keyword: &str) -> bool {
    text.strip_prefix(keyword).is_some_and(|rest| {
        rest.is_empty() || rest.starts_with(|character: char| character.is_ascii_whitespace())
    })
}

/// The kind of a Java line comment.
///
/// Java has exactly one line-comment documentation marker: `///`, the Markdown
/// documentation comment JEP 467 added in JDK 23. `//!` is Rust's inner-doc
/// marker and means nothing here, so a comment opening with it is an ordinary
/// line comment — reading it as documentation would hide it from
/// [`crate::Policy::Safe`] in a language that never wrote it as one.
fn java_line_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"///") {
        CommentKind::DocLine
    } else {
        CommentKind::Line
    }
}

/// The kind of a Java block comment: `/** ... */` is the documentation comment
/// of JLS 3.7, and `/*!` — Doxygen's marker, which C and C++ do honour — is
/// not one, for the same reason [`java_line_kind`] gives.
fn java_block_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"/**") {
        CommentKind::DocBlock
    } else {
        CommentKind::Block
    }
}

/// The kind of a Dart line comment.
///
/// `tokenizeSingleLineComment` reads the byte behind `//` and sets `dartdoc`
/// when it is a third slash, then reads no further: a fourth slash leaves
/// `////` a `DartDocToken` just as `///` is one, which is where Dart parts
/// company with Lua's `----` and Zig's `////`. `//!` is Rust's inner-doc
/// marker and means nothing here, so a comment opening with it is an ordinary
/// line comment — reading it as documentation would hide it from
/// [`crate::Policy::Safe`] in a language that never wrote it as one.
fn dart_line_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"///") {
        CommentKind::DocLine
    } else {
        CommentKind::Line
    }
}

/// The kind of a Dart block comment.
///
/// `tokenizeMultiLineComment` sets `dartdoc` from the single byte behind `/*`,
/// so `/**` opens the documentation comment `dart doc` reads and `/**/` is an
/// empty one. `/*!` is Doxygen's marker, which C and C++ honour and Dart does
/// not, for the reason [`dart_line_kind`] gives.
fn dart_block_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"/**") {
        CommentKind::DocBlock
    } else {
        CommentKind::Block
    }
}

/// The kind of a Swift line comment.
///
/// Swift's documentation marker is `///`, and a fourth slash does not take it
/// away: SwiftSyntax reports `////` as `docLineComment` exactly as it reports
/// `///`, which is where Swift keeps company with Dart and parts company with
/// Lua's `----` and Zig's `////`. `//!` is Rust's inner-doc marker and means
/// nothing here, so a comment opening with it is an ordinary line comment —
/// reading it as documentation would hide it from [`crate::Policy::Safe`] in a
/// language that never wrote it as one.
fn swift_line_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"///") {
        CommentKind::DocLine
    } else {
        CommentKind::Line
    }
}

/// The kind of a Swift block comment.
///
/// `/**` opens the documentation comment DocC reads, with one exception that
/// the third `*` is not there to open anything: `/**/` is the *empty* block
/// comment, whose second `*` is the first byte of its own terminator.
/// SwiftSyntax reports `/**/` as `blockComment` and `/***/` as
/// `docBlockComment`, and this follows it. `/*!` is Doxygen's marker, which C
/// and C++ honour and Swift does not, for the reason [`swift_line_kind`] gives.
fn swift_block_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"/**") && !starts(bytes, index, b"/**/") {
        CommentKind::DocBlock
    } else {
        CommentKind::Block
    }
}

/// The name of the Swift string form that was left open, so the diagnostic says
/// which of the five a reader has to go and close. The fifth is the
/// single-quoted literal the compiler lexes and then rejects, which carries
/// neither a hash nor a multi-line spelling.
const fn swift_unterminated_string(delimiter: u8, raw: bool, multiline: bool) -> &'static str {
    if delimiter == b'\'' {
        return "unterminated Swift single-quoted string";
    }
    match (raw, multiline) {
        (true, true) => "unterminated Swift raw multiline string",
        (true, false) => "unterminated Swift raw string",
        (false, true) => "unterminated Swift multiline string",
        (false, false) => "unterminated Swift string",
    }
}

/// Whether `count` `#` bytes stand at `index`, and the byte behind them is not
/// a `#` as well.
///
/// The run has to be *exactly* `count` long for a delimiter to close: a `"#`
/// inside a `##"..."##` literal is content, and only `"##` ends it. A longer run
/// still closes it, because the extra hashes are the content that follows the
/// delimiter rather than part of it, which is why the byte behind is not
/// examined.
fn swift_hashes_at(bytes: &[u8], index: usize, count: usize) -> bool {
    (0..count).all(|offset| bytes.get(index + offset) == Some(&b'#'))
}

/// The end of a string with `hashes` hashes if its delimiter closes at
/// `index`, or `None` when it does not.
///
/// NOTE: a *raw* delimiter takes one `#` more than it needs when one is there.
/// `Lexer.Cursor.advanceIfStringDelimiter` counts pounds with the advance in
/// the loop *condition* and the count test in its body, so it consumes a
/// `hashes + 1`-th before it stops — and then `swiftc` calls the literal an
/// error, `too many '#' characters in closing delimiter`. Taking that byte too
/// is what keeps this scan standing where the lexer stands on a file that is
/// already broken; a file that closes its raw strings correctly never reaches
/// the case at all. An ordinary string takes none: the same function returns on
/// `delimiterLength == 0` before it looks at a byte, so the `#` behind the
/// closing quote of `"x"#/y/#` opens the regular expression literal that
/// follows rather than belonging to the string — which is the difference
/// between reading the `//` inside that literal as pattern and reading it as a
/// comment.
fn swift_string_close(
    bytes: &[u8],
    index: usize,
    delimiter: u8,
    multiline: bool,
    hashes: usize,
) -> Option<usize> {
    let width = if multiline { 3 } else { 1 };
    let closes = (0..width).all(|offset| bytes.get(index + offset) == Some(&delimiter))
        && swift_hashes_at(bytes, index + width, hashes);
    if !closes {
        return None;
    }
    let end = index + width + hashes;
    let extra = hashes > 0 && bytes.get(end) == Some(&b'#');
    Some(end + usize::from(extra))
}

/// The length of the run of `#` beginning at `index`.
///
/// INVARIANT: a `#` in Swift opens a compiler directive, a macro, a raw string,
/// or an extended regular expression literal, and only the last two carry a run
/// of them. What decides which is the byte behind the run, so this reads the
/// class and one byte more rather than searching for a quote that may never
/// come — the bound is what the reach is for. A `#` run holds no line
/// terminator, so the watermark it leaves never crosses the line the run stands
/// on, and the lines under it keep their checkpoints.
fn swift_hash_run(bytes: &[u8], index: usize, reach: &mut Reach) -> usize {
    let mut end = index;
    while bytes.get(end) == Some(&b'#') {
        end += 1;
    }
    /* NOTE: the byte that ended the run decided this, and a `get` that came
     * back `None` at the end of the document decided it just the same. */
    reach.byte(end);
    end - index
}

/// Whether the `/` at `index` stands where a *binary* operator does, and can
/// therefore open no regular expression literal.
///
/// The Swift book (Lexical Structure, Operators) decides this from the white
/// space around an operator: one with white space on both sides or on neither
/// is binary, and one with white space on the left alone is prefix. For that
/// rule `(`, `[` and `{` before an operator count as white space, and so do
/// `,`, `;` and `:`. A regular expression literal may only stand where a prefix
/// operator may, so `a /b/ c` opens one and `a/b/c` is three divisions —
/// measured with `swift-frontend -dump-parse -swift-version 6`, which reports a
/// `regex_literal_expr` for the first and none for the second.
///
/// The end of a block comment is white space as well: `1 /* c *//a/` opens a
/// literal, because the `/` that closes the comment is not a token the `/`
/// behind it could bind to.
fn swift_is_left_bound(bytes: &[u8], index: usize) -> bool {
    let Some(previous) = index.checked_sub(1).map(|behind| bytes[behind]) else {
        return false;
    };
    /* NOTE: the set is SwiftSyntax's `Lexer.Cursor.isLeftBound` as the Swift
     * 6.3.3 toolchain ships it, which is the book's rule with two more bytes
     * that are white space in fact: a NUL, and the second byte of a U+00A0
     * no-break space. */
    match previous {
        b' ' | b'\t' | b'\r' | b'\n' | 0 => false,
        b'(' | b'[' | b'{' | b',' | b';' | b':' => false,
        b'/' => index < 2 || bytes[index - 2] != b'*',
        0xa0 => index < 2 || bytes[index - 2] != 0xc2,
        _ => true,
    }
}

/// Whether the quotes at `quote` open a *multi-line* string literal.
///
/// Three quotes are the multi-line delimiter, with one exception that only a
/// raw string can spell: `#"""#` is the single-line raw string whose one
/// character of content is a quote, and not a multi-line literal left open.
/// SwiftSyntax decides it in `Lexer.Cursor.advanceIfMultilineStringDelimiter`,
/// and only for a raw string: from the *third* quote it reads the rest of the
/// line for a `"` carrying the opening run of `#` behind it, and a literal that
/// closes on its own line that way is a single-line one.
///
/// This is a read of the line the literal opens on and no further, and the scan
/// takes every byte of it either way — a multi-line literal reaches past the
/// line and a single-line one ends inside it, but neither hands those bytes
/// back — so there is nothing here for a checkpoint to lose.
fn swift_multiline_string(bytes: &[u8], quote: usize, hashes: usize) -> bool {
    if !starts(bytes, quote, b"\"\"\"") {
        return false;
    }
    if hashes == 0 {
        return true;
    }
    let mut cursor = quote + 2;
    while bytes
        .get(cursor)
        .is_some_and(|byte| !is_line_terminator(*byte))
    {
        if bytes[cursor] == b'"' && swift_hashes_at(bytes, cursor + 1, hashes) {
            return false;
        }
        cursor += 1;
    }
    true
}

/// The end of the bare regular expression literal `/ ... /` opening at `index`,
/// or `None` when those bytes open none.
///
/// The Swift book (Lexical Structure, Regular Expression Literals) states the
/// rule this follows: a literal *can't begin with an unescaped tab or space*,
/// and it *can't contain an unescaped forward slash, a carriage return, or a
/// line feed*. So the first unescaped `/` is the closing delimiter, the search
/// for it never leaves the line the literal opened on, and a backslash carries
/// the byte behind it in — `/a\//` is a literal whose content is `a\/`, which
/// is exactly the shape that makes this rule worth having: its last two bytes
/// spell `//`, and a scanner that read them as a comment would delete the rest
/// of the line.
///
/// Two more conditions come from the compiler rather than from the book, and
/// both were measured with `swift-frontend -dump-parse -swift-version 6`. The
/// closing delimiter may not be *preceded* by an unescaped space or tab, so
/// `/b /` opens nothing while `/a\ /` does. And when the closing delimiter
/// would be the first byte of a comment the comment wins outright, rather than
/// the literal ending one byte earlier: `/a//b/` is `/a` and then the line
/// comment `//b/`, and `/a/*b*/` is `/a` and then the block comment `/*b*/`.
///
/// What is left after all of that is a lookahead the scan rewinds behind, so it
/// reports every byte it read. It stops at the first line terminator, so the
/// reach it leaves reaches no further than the line start under it — except
/// when the document simply ends, which is a decision an append would change
/// and is recorded as such.
fn swift_bare_regex(bytes: &[u8], index: usize, reach: &mut Reach) -> Option<usize> {
    if swift_is_left_bound(bytes, index) {
        return None;
    }
    reach.byte(index + 1);
    match bytes.get(index + 1) {
        None | Some(b' ' | b'\t' | b'\r' | b'\n') => return None,
        Some(_) => {}
    }
    let mut cursor = index + 1;
    let mut blank = false;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => {
                reach.byte(cursor + 1);
                if matches!(bytes.get(cursor + 1), None | Some(b'\r' | b'\n')) {
                    return None;
                }
                blank = false;
                cursor += 2;
            }
            b'\r' | b'\n' => {
                reach.byte(cursor);
                return None;
            }
            b'/' => {
                reach.byte(cursor + 1);
                if blank || matches!(bytes.get(cursor + 1), Some(b'/' | b'*')) {
                    return None;
                }
                return Some(cursor + 1);
            }
            byte => {
                blank = matches!(byte, b' ' | b'\t');
                cursor += 1;
            }
        }
    }
    /* NOTE: no line terminator and no closing delimiter, so this read to the end
     * of the document and the scan takes those bytes back. An append is an edit
     * exactly at the end, and appending a `/` would close this literal, so the
     * checkpoint an append reuses the whole prefix from is the one to withdraw. */
    reach.end_of(bytes);
    None
}

/// Which of C#'s three string rules a literal follows.
///
/// The three differ in what closes them and in what an escape is: a plain
/// string takes `\` and ends at its line, a verbatim one spells its quote `""`
/// and carries line breaks, and a raw one is opaque until a run of at least as
/// many quotes as its opener carried comes back. Interpolation is a switch on
/// top of all three rather than a fourth rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CsharpStringForm {
    /// `"..."`, and `$"..."` with one `$`.
    Plain,
    /// `@"..."`, `$@"..."` and `@$"..."`.
    Verbatim,
    /// `"""..."""`, `$"""..."""` and every literal a run of two `$` or more
    /// opens, which Roslyn lexes as a raw one whatever its quote run holds.
    Raw,
}

/// The opener of one C# string literal: where it begins, where its quote run
/// begins and how long that run is, how many `$` stood in front of it, and
/// which of the three rules it follows.
#[derive(Clone, Copy, Debug)]
struct CsharpPrefix {
    /// The first byte of the literal, `$` and `@` included.
    start: usize,
    /// The first `"`.
    quote: usize,
    /// The length of the opening run of `"`, which is the length a closing run
    /// of a raw literal must match or beat.
    quotes: usize,
    /// How many `$` opened it, which is how many braces open one of its holes.
    /// Zero for a literal that interpolates nothing.
    dollars: usize,
    form: CsharpStringForm,
}

/// One past the run of `$` and `@` at `index`, which is where a scan that found
/// no literal behind them resumes.
fn csharp_prefix_end(bytes: &[u8], index: usize) -> usize {
    let mut end = index;
    while matches!(bytes.get(end), Some(b'$' | b'@')) {
        end += 1;
    }
    end
}

/// The literal a run of `$` and `@` and then a run of `"` opens at `index`, or
/// `None` when no quote follows the prefix and those bytes open none.
///
/// Which rule applies is decided the way Roslyn's lexer decides it, and the
/// order of the three tests is the part that is not obvious. Two `$` or more
/// make a literal raw whatever else it carries — `$$"a"` is lexed as a raw
/// interpolated string with a one-quote delimiter and CS9004 rather than as a
/// plain one — while a single `@` makes it verbatim even in front of three
/// quotes: Roslyn reports `@"""x"""` as one `StringLiteralToken` whose content
/// is `"x"`, two escaped quotes around a letter.
///
/// INVARIANT: the quote run is read whole and the byte behind it with it, which
/// is a bounded read of a run that can hold no line terminator, so the
/// watermark it leaves never reaches past the line the literal opens on.
fn csharp_literal_prefix(bytes: &[u8], index: usize, reach: &mut Reach) -> Option<CsharpPrefix> {
    let mut cursor = index;
    let mut dollars = 0;
    let mut ats = 0;
    while let Some(byte) = bytes.get(cursor) {
        match byte {
            b'$' => dollars += 1,
            b'@' => ats += 1,
            _ => break,
        }
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        /* NOTE: the byte that ended the run decided this, and a `get` that came
         * back `None` at the end of the document decided it just the same. */
        reach.byte(cursor);
        return None;
    }
    let mut quotes = 0;
    while bytes.get(cursor + quotes) == Some(&b'"') {
        quotes += 1;
    }
    reach.byte(cursor + quotes);
    let form = if dollars >= 2 {
        CsharpStringForm::Raw
    } else if ats > 0 {
        CsharpStringForm::Verbatim
    } else if quotes >= 3 {
        CsharpStringForm::Raw
    } else {
        CsharpStringForm::Plain
    };
    Some(CsharpPrefix {
        start: index,
        quote: cursor,
        quotes,
        dollars,
        form,
    })
}

/// Whether the raw string whose content begins at `content` carries its line
/// breaks, which it does when only blanks stand between its opening quote run
/// and the end of that line.
///
/// This is a read of the line the literal opens on and no further, and the scan
/// takes every byte of it either way — the blanks are content of a multi-line
/// literal and content of a single-line one alike, and the byte that ends them
/// is the closing quote run, a brace, or the line terminator the literal ends
/// at — so there is nothing here for a checkpoint to lose.
fn csharp_multiline_raw_string(bytes: &[u8], content: usize) -> bool {
    let mut cursor = content;
    while cursor < bytes.len() {
        if csharp_line_terminator_width(bytes, cursor).is_some() {
            return true;
        }
        if !is_csharp_blank(bytes[cursor]) {
            return false;
        }
        cursor += 1;
    }
    false
}

/// Where a `}` at `index` leaves an interpolation hole that `braces` braces
/// close: the end of the hole and `true` when the run is long enough, and one
/// past the run it did hold with `false` when it is not.
///
/// The run is read no further than `braces`, so the close consumes exactly what
/// it read and a run too short is consumed whole; neither hands a byte back.
fn csharp_hole_close(bytes: &[u8], index: usize, braces: usize) -> (usize, bool) {
    let mut run = index;
    while run - index < braces && bytes.get(run) == Some(&b'}') {
        run += 1;
    }
    if run - index == braces {
        (index + braces, true)
    } else {
        (run, false)
    }
}

/// The width of the line terminator at `index`, or `None` when there is none.
///
/// ECMA-334 6.3.1 writes `New_Line_Character` as five characters rather than
/// two: the carriage return and line feed every language here has, and U+0085,
/// U+2028 and U+2029 besides. Roslyn's lexer ends a `//` comment, a string and
/// a character literal at all five, which is exactly why this is not
/// [`line_end`]: a comment read on past a U+2028 would swallow the code behind
/// it and a removal would take that code with it.
fn csharp_line_terminator_width(bytes: &[u8], index: usize) -> Option<usize> {
    csharp_unicode_line_terminator_width(bytes, index)
        .or_else(|| unicode_line_terminator_width(bytes, index))
}

/// The width of the *non-ASCII* line terminator at `index`, or `None`.
///
/// U+2028 and U+2029 are the two JavaScript shares, and U+0085 the one it does
/// not: a C# source encoded in UTF-8 spells the next-line character `\xc2\x85`.
fn csharp_unicode_line_terminator_width(bytes: &[u8], index: usize) -> Option<usize> {
    match bytes.get(index) {
        Some(0xc2) if bytes.get(index + 1) == Some(&0x85) => Some(2),
        Some(0xe2)
            if bytes.get(index + 1) == Some(&0x80)
                && matches!(bytes.get(index + 2), Some(0xa8 | 0xa9)) =>
        {
            Some(3)
        }
        _ => None,
    }
}

/// One past the end of the line `index` stands on, counting every one of C#'s
/// five line terminators. See [`csharp_line_terminator_width`].
fn csharp_line_end(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && csharp_line_terminator_width(bytes, index).is_none() {
        index += 1;
    }
    index
}

/// The kind of a Scala line comment.
///
/// The comment reader of the Scala 3 compiler classifies a comment as
/// documentation when its raw text starts with `/**` (`Comment.isDocComment`),
/// and a line comment cannot, so `///` — which scaladoc does not read — and
/// `//!` are ordinary line comments, unlike Dart's and Swift's third slash.
fn scala_line_kind(_bytes: &[u8], _index: usize) -> CommentKind {
    CommentKind::Line
}

/// The kind of a Scala block comment: documentation exactly when its raw text
/// starts with `/**`, which is what `Comment.isDocComment` answers, so `/**/`
/// and `/***/` are documentation comments — their second `*` is content — and
/// `/*!` is Doxygen's marker, which Scala does not honour.
fn scala_block_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"/**") {
        CommentKind::DocBlock
    } else {
        CommentKind::Block
    }
}

/// Whether the quote at `quote` opens an *interpolated* string.
///
/// The compiler's lexer turns an identifier standing directly before a quote
/// into `INTERPOLATIONID` (`fetchToken` checks `ch == '"' && token ==
/// IDENTIFIER` after reading an identifier), and a keyword is its own token,
/// so `s"..."` and `raw"..."` and a custom interpolator interpolate and
/// `return"..."` does not. A number is not an identifier either, so the run
/// of identifier characters is read back from the quote and refused when it
/// begins with a digit.
fn scala_interpolator(bytes: &[u8], quote: usize) -> bool {
    let mut start = quote;
    while start > 0 && scala_identifier_part(bytes[start - 1]) {
        start -= 1;
    }
    if start == quote || !scala_identifier_start(bytes[start]) {
        return false;
    }
    !scala_is_keyword(&bytes[start..quote])
}

/// Whether `word` is a hard Scala keyword, which is its own token and does not
/// interpolate a string after it. Soft keywords — `as`, `derives`, `end`,
/// `extension`, `infix`, `inline`, `opaque`, `using` — are identifiers in the
/// lexer, so they are interpolators like any other.
fn scala_is_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"abstract"
            | b"case"
            | b"catch"
            | b"class"
            | b"def"
            | b"do"
            | b"else"
            | b"enum"
            | b"export"
            | b"extends"
            | b"final"
            | b"finally"
            | b"for"
            | b"given"
            | b"if"
            | b"implicit"
            | b"import"
            | b"lazy"
            | b"match"
            | b"new"
            | b"object"
            | b"open"
            | b"override"
            | b"package"
            | b"private"
            | b"protected"
            | b"return"
            | b"sealed"
            | b"then"
            | b"throw"
            | b"trait"
            | b"transparent"
            | b"try"
            | b"type"
            | b"val"
            | b"var"
            | b"while"
            | b"with"
            | b"yield"
    )
}

/// The first byte of a Scala identifier; `$` is one, because `$anonfun` is.
fn scala_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

/// A byte an identifier may carry after its first.
fn scala_identifier_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

/// Whether the `<` at `index` opens an XML literal, which is exactly where the
/// compiler's lexer emits `XMLSTART`: the byte before it is space, tab, line
/// feed, `{`, `(` or `>` — a token boundary, and not the `x` of `x<a>` — and
/// the byte after it is an XML name start, `!` or `?`. The name start is the
/// lexer's own `xml.Utility.isNameStart`, which answers to letters and `_`
/// and *not* to `:` — that is what keeps `_ <: Suite` a type bound and `x<a>`
/// a literal — and a byte with the high bit is read as a letter too, which
/// costs a comparison nothing and gives a literal a chance to protect text. A
/// byte order is read from the start of the file, so a `<` at its very
/// beginning is preceded by the space the lexer defaults to. A bare `\r` is
/// not a boundary: `<a>` after one is a comparison, exactly as the lexer
/// reads it.
fn scala_is_xml_start(bytes: &[u8], index: usize) -> bool {
    let before = if index > 0 { bytes[index - 1] } else { b' ' };
    if !matches!(before, b' ' | b'\t' | b'\n' | b'{' | b'(' | b'>') {
        return false;
    }
    match bytes.get(index + 1) {
        Some(b'!' | b'?') => true,
        Some(&byte) => byte.is_ascii_alphabetic() || matches!(byte, b'_') || byte >= 0x80,
        None => false,
    }
}

/// The first byte of an XML name: XML 1.0 `NameStartChar`, of which the ASCII
/// alphabet, `_` and `:` are the bytes a source file actually writes. A byte
/// with the high bit is read as one too — it may be a non-ASCII letter — which
/// costs a comparison nothing and gives a literal a chance to protect text.
fn xml_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':') || byte >= 0x80
}

/// The length of the XML name beginning at `index`, in bytes.
fn xml_name_len(bytes: &[u8]) -> usize {
    let mut index = 0;
    while index < bytes.len() && xml_name_char(bytes[index]) {
        index += 1;
    }
    index
}

/// A byte an XML name may carry after its first.
fn xml_name_char(byte: u8) -> bool {
    xml_name_start(byte) || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
}

/// The byte after the tail of a closing XML tag: the name already read, and
/// whatever white space and `>` end the tag, if the `>` is there at all.
fn skip_xml_tag_tail(bytes: &[u8], mut index: usize) -> Option<usize> {
    while matches!(bytes.get(index), Some(b' ' | b'\t' | b'\r' | b'\n')) {
        index += 1;
    }
    (bytes.get(index) == Some(&b'>')).then_some(index + 1)
}

/// The length of the run of `byte` beginning at `index`.
fn count_run(bytes: &[u8], index: usize, byte: u8) -> usize {
    let mut end = index;
    while bytes.get(end) == Some(&byte) {
        end += 1;
    }
    end - index
}

/// Whether `byte` is white space that a pre-processing directive may stand
/// behind, and that leaves the line it is on still blank.
///
/// ECMA-334 6.5.1 writes `PP_Whitespace` as `Whitespace_Character+`, and a
/// vertical tab and a form feed are two of those: Roslyn's `IsWhitespace`
/// answers to both, and to the space and tab a file is actually indented with.
///
/// NOTE: the `Zs` category is deliberately not here. Roslyn does count a
/// no-break space as white space in front of a directive, so a line indented
/// with one is a directive line there and an ordinary line here — which costs
/// that line the one comment a directive may carry and nothing else, where
/// reading a `#` no compiler would as a directive would cost the rest of the
/// line. No C# file is indented that way; being wrong in the direction that
/// keeps a comment is what this picks.
const fn is_csharp_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0b | 0x0c)
}

/// Whether the directive named `name` takes the rest of its line as a message.
///
/// `#error` and `#warning` carry the text their diagnostic quotes and `#region`
/// and `#endregion` the label an editor folds under, and ECMA-334 6.5.1 writes
/// all four as `Input_Character*` — every byte to the end of the line,
/// whatever it spells. Roslyn still lexes a `//` that *opens* the message as a
/// comment, which is the one case this distinction has to keep.
fn csharp_directive_takes_a_message(name: &[u8]) -> bool {
    matches!(name, b"error" | b"warning" | b"region" | b"endregion")
}

/// The name of the C# string form that was left open, so the diagnostic says
/// which of the six a reader has to go and close.
const fn csharp_unterminated_string(form: CsharpStringForm, interpolated: bool) -> &'static str {
    match (form, interpolated) {
        (CsharpStringForm::Plain, false) => "unterminated C# string",
        (CsharpStringForm::Plain, true) => "unterminated C# interpolated string",
        (CsharpStringForm::Verbatim, false) => "unterminated C# verbatim string",
        (CsharpStringForm::Verbatim, true) => "unterminated C# interpolated verbatim string",
        (CsharpStringForm::Raw, false) => "unterminated C# raw string",
        (CsharpStringForm::Raw, true) => "unterminated C# interpolated raw string",
    }
}

/// The kind of a C# line comment.
///
/// `///` is the XML documentation comment of ECMA-334 6.3.3, and a fourth slash
/// takes it back: Roslyn's lexer asks for `///` and then that the byte behind
/// is no slash, so `////` is the ordinary comment a reader rules a section off
/// with — which is where C# parts company with Dart and Swift and keeps company
/// with Java. `//!` is Rust's inner-doc marker and means nothing here.
fn csharp_line_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"///") && bytes.get(index + 3) != Some(&b'/') {
        CommentKind::DocLine
    } else {
        CommentKind::Line
    }
}

/// The kind of a C# block comment.
///
/// `/**` opens the delimited documentation comment, with two spellings that are
/// not there to open anything: `/**/` is the *empty* block comment, whose
/// second `*` is the first byte of its own terminator, and `/***` is a rule of
/// stars. Roslyn asks for `/**` and then that the byte behind is neither `*`
/// nor `/`, and reports `/**/`, `/***/` and `/*** x */` as `MultiLineComment`
/// where `/** x */` is `MultiLineDocumentationComment`. `/*!` is Doxygen's
/// marker, which C and C++ honour and C# does not.
fn csharp_block_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"/**") && !matches!(bytes.get(index + 3), Some(b'*' | b'/')) {
        CommentKind::DocBlock
    } else {
        CommentKind::Block
    }
}

/// The name of the Dart string form that was left open, so the diagnostic says
/// which of the six a reader has to go and close.
const fn dart_unterminated_string(raw: bool, triple: bool) -> &'static str {
    match (raw, triple) {
        (true, true) => "unterminated Dart raw multiline string",
        (true, false) => "unterminated Dart raw string",
        (false, true) => "unterminated Dart multiline string",
        (false, false) => "unterminated Dart string",
    }
}

/// Whether the quote at `quote` is opened by a raw-string `r`.
///
/// `tokenizeRawStringKeywordOrIdentifier` is reached from the scanner's main
/// switch, which means the `r` has to *begin a token*: an `r` that continues an
/// identifier is a letter of that identifier, and the quote behind it opens an
/// ordinary string. Only a lower-case `r` does it — `R'x'` is the identifier
/// `R` and then a string.
///
/// What decides it is therefore the run of identifier bytes ending just before
/// the `r`. An empty run means nothing precedes it and the `r` begins a token.
/// A run that begins with a letter, `_` or `$` is an identifier the `r`
/// continues. A run that begins with a digit is a number, and a number token
/// always ends before an `r` — `r` is not a digit, a hex digit, `x`, `e`, `.`
/// or `_` — so the `r` begins a token there too.
///
/// Measured on Dart SDK 3.13.2: `1r'x'` and `0x1r'x'` are `INT`/`HEXADECIMAL`
/// and then a raw `STRING`, while `xr'x'`, `_r'x'` and `$r'x'` are one
/// `IDENTIFIER` and then an ordinary `STRING`.
fn dart_raw_string_prefix(bytes: &[u8], quote: usize) -> bool {
    if quote == 0 || bytes[quote - 1] != b'r' {
        return false;
    }
    let mut cursor = quote - 1;
    while cursor > 0 && is_dart_identifier_continue(bytes[cursor - 1]) {
        cursor -= 1;
    }
    cursor == quote - 1 || bytes[cursor].is_ascii_digit()
}

/// Whether `byte` may stand inside a Dart identifier.
///
/// The grammar spells one `IDENTIFIER_START_NO_DOLLAR ::= LETTER | '_'` with
/// `'$'` allowed as well, and `IDENTIFIER_PART` adds the digits (Dart Language
/// Specification, 17.4 Identifier Reference). `LETTER` is an ASCII letter
/// there and nothing wider, so this is deliberately ASCII-only.
fn is_dart_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

/// Whether `raw` is Dart's language version comment, which the scanner itself
/// reads rather than a tool.
///
/// `tokenizeLanguageVersionOrSingleLineComment` accepts exactly two slashes —
/// a third one sends it to `tokenizeSingleLineComment` instead — then spaces,
/// `@dart` in lower case, spaces, `=`, spaces, a run of digits, `.`, a second
/// run of digits, spaces, and the end of the line. Anything else falls back to
/// an ordinary comment, so this follows the same grammar byte for byte. Only
/// the space is skipped, not the tab: the scanner compares against `$SPACE`.
///
/// The comment is honoured only ahead of the first real token of a file, and
/// this is asked of every comment in one. Reading a later one as an instruction
/// keeps a comment a removal would otherwise take, which is the direction to be
/// wrong in, and it is what keeps the answer independent of where in the
/// document the scan began.
fn dart_language_version(raw: &[u8]) -> bool {
    fn past_spaces(bytes: &[u8]) -> &[u8] {
        let taken = bytes.iter().take_while(|byte| **byte == b' ').count();
        &bytes[taken..]
    }
    fn past_digits(bytes: &[u8]) -> Option<&[u8]> {
        let taken = bytes
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        (taken > 0).then(|| &bytes[taken..])
    }
    let Some(rest) = raw.strip_prefix(b"//") else {
        return false;
    };
    if rest.first() == Some(&b'/') {
        return false;
    }
    let Some(rest) = past_spaces(rest).strip_prefix(b"@dart") else {
        return false;
    };
    let Some(rest) = past_spaces(rest).strip_prefix(b"=") else {
        return false;
    };
    let Some(rest) = past_digits(past_spaces(rest)) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(b".") else {
        return false;
    };
    let Some(rest) = past_digits(rest) else {
        return false;
    };
    past_spaces(rest).is_empty()
}

fn line_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"///") || starts(bytes, index, b"//!") {
        CommentKind::DocLine
    } else {
        CommentKind::Line
    }
}

fn block_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"/**") || starts(bytes, index, b"/*!") {
        CommentKind::DocBlock
    } else {
        CommentKind::Block
    }
}

/// The kind of a Lua short comment.
///
/// `---` opens the documentation comment LDoc and the Lua language server
/// read; a fourth dash makes the divider that separates one section of a file
/// from the next, which documents nothing and is an ordinary comment.
fn lua_line_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"---") && !starts(bytes, index, b"----") {
        CommentKind::DocLine
    } else {
        CommentKind::Line
    }
}

/// The kind of a Zig comment.
///
/// `///` documents the declaration under it and `//!` the container the file
/// is (Zig Language Reference, Doc comments), and `std.zig.Tokenizer` tags
/// them `doc_comment` and `container_doc_comment`. A fourth slash takes the
/// first back: `.doc_comment_start` falls to `.line_comment` when it meets
/// one, so `////` is an ordinary comment and only exactly three slashes
/// document anything. `//!!` stays a top-level doc comment, because the
/// tokenizer decides that one at the `!` and reads no further.
fn zig_line_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"////") {
        CommentKind::Line
    } else if starts(bytes, index, b"///") || starts(bytes, index, b"//!") {
        CommentKind::DocLine
    } else {
        CommentKind::Line
    }
}

/// The kind of an R comment.
///
/// R's parser has one comment token and calls every `#` line a `COMMENT`
/// (measured on R 4.3.3: `utils::getParseData` gives `#' doc` and `# line` the
/// same token name). `#'` is roxygen2's marker for the prose it turns into a
/// manual page, so it is documentation here for the reason Lua's `---` and
/// Zig's `///` are: the tool that reads it is what makes it one. Nothing takes
/// the marker back the way a fourth slash does in Zig — roxygen2 reads `#''`
/// and `#'#` as its own too — so the test is the two bytes and no more.
fn r_line_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"#'") {
        CommentKind::DocLine
    } else {
        CommentKind::Line
    }
}

/// Whether `byte` may continue an R name, and so cannot be followed by the `r`
/// that opens a raw string.
///
/// `SymbolValue` (`gram.y`) reads a name while the bytes are alphanumeric, `.`
/// or `_`, and it is entered on a multi-byte character as well, so every byte
/// with the high bit set counts here. Counting one that does not only refuses
/// the raw reading, which falls back to an ordinary string and hides more
/// rather than less.
fn is_r_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_') || byte >= 0x80
}

/// The end of the R raw string whose quote is at `quote`, and whether it closed
/// — or `None` when no raw string opens there at all.
///
/// The literal is `r` or `R`, the quote, a run of dashes, and one of `(`, `[`
/// or `{`; it closes on the matching bracket, the same run of dashes, and the
/// same quote (`?Quotes`; R 4.0.0 and later). The dash run is what lets the
/// closing bracket appear as content, so it is copied out of the source rather
/// than counted twice, and R puts no limit on its length — 100 dashes were
/// measured accepted on R 4.3.3.
///
/// The `r` opens the literal only where it begins a token: `xr"(a)"` is the
/// name `xr` and then an ordinary string, which is what R's lexer reads there
/// too.
fn r_raw_string(bytes: &[u8], quote: usize) -> Option<(usize, bool)> {
    let prefix = quote.checked_sub(1)?;
    if !matches!(bytes[prefix], b'r' | b'R') {
        return None;
    }
    if prefix > 0 && is_r_name_byte(bytes[prefix - 1]) {
        return None;
    }
    let mut bracket = quote + 1;
    while bytes.get(bracket) == Some(&b'-') {
        bracket += 1;
    }
    let closing = match bytes.get(bracket) {
        Some(b'(') => b')',
        Some(b'[') => b']',
        Some(b'{') => b'}',
        _ => return None,
    };
    let mut close = Vec::with_capacity(bracket - quote + 1);
    close.push(closing);
    close.extend_from_slice(&bytes[quote + 1..bracket]);
    close.push(bytes[quote]);
    Some(match find_subslice(&bytes[bracket + 1..], &close) {
        Some(relative) => (bracket + 1 + relative + close.len(), true),
        None => (bytes.len(), false),
    })
}

/// The end of the R literal that runs from `index` to the next unescaped
/// `close`, and whether that delimiter was there at all.
///
/// One function for the two quoted strings and the backquoted name, because R
/// lexes all three the same way: `\` carries the next byte in — a line break
/// included, which is why a literal that never closes runs to the end of the
/// file rather than to the end of its line — and nothing but the delimiter ends
/// them.
fn r_delimited_end(bytes: &[u8], mut index: usize, close: u8) -> (usize, bool) {
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == close {
            return (index + 1, true);
        } else {
            index += 1;
        }
    }
    (bytes.len(), false)
}

/// The level of the long bracket that opens at `index`, or `None` when none
/// does.
///
/// An opening long bracket is `[`, then a run of `=`, then `[`, and the length
/// of that run is its level (Lua 5.4 reference manual, 3.1). The second `[` is
/// what tells `[[` from the two brackets of `a[b[1]]`, so a bracket that never
/// reaches it opens nothing at all.
///
/// The closing form is the same run between `]` and `]`, which is why
/// [`long_bracket_end`] takes the level rather than a delimiter: a level-two
/// bracket carries `]]` and `]=]` as content and ends only at `]==]`.
fn long_bracket_level(bytes: &[u8], index: usize) -> Option<usize> {
    if bytes.get(index) != Some(&b'[') {
        return None;
    }
    let mut cursor = index + 1;
    while bytes.get(cursor) == Some(&b'=') {
        cursor += 1;
    }
    (bytes.get(cursor) == Some(&b'[')).then(|| cursor - index - 1)
}

/// The end of a long bracket whose content starts at `content`, and whether the
/// closing bracket of `level` was there at all.
///
/// Long brackets do not nest, so the first close at the right level ends it and
/// a run of the wrong length is content. A `]` that opens a run of the wrong
/// length is passed over rather than skipped: the bytes it ran through are `=`,
/// which can start no close of their own.
fn long_bracket_end(bytes: &[u8], content: usize, level: usize) -> (usize, bool) {
    let mut index = content.min(bytes.len());
    while let Some(relative) = memchr(b']', &bytes[index..]) {
        let close = index + relative;
        let mut cursor = close + 1;
        while bytes.get(cursor) == Some(&b'=') {
            cursor += 1;
        }
        if cursor - close - 1 == level && bytes.get(cursor) == Some(&b']') {
            return (cursor + 1, true);
        }
        index = close + 1;
    }
    (bytes.len(), false)
}

/// Whether `byte` is ECMAScript `WhiteSpace` or a `LineTerminator`, as far as
/// one byte can say (ECMA-262 12.2, 12.3). <VT> is whitespace to JavaScript and
/// is exactly what [`u8::is_ascii_whitespace`] leaves out, so asking that
/// instead reads `a\u{b}<div>` as a JSX element where the language sees a
/// comparison. The non-ASCII members — U+00A0, U+FEFF, and the `Zs` category —
/// take more than one byte and are not decided here.
fn js_is_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ')
}

/// Whether `byte` is whitespace to Lua's lexer, which is what `\z` skips. It is
/// C's `isspace` in the default locale, and so takes the vertical tab that
/// [`u8::is_ascii_whitespace`] leaves out.
fn lua_is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// The width of the line terminator at `index`, or `None` when there is none.
///
/// Lua counts `\r\n` and `\n\r` alike as one line (`llex.c`,
/// `inclinenumber`), so a backslash in front of either escapes the whole pair.
fn lua_newline_width(bytes: &[u8], index: usize) -> Option<usize> {
    match (bytes.get(index), bytes.get(index + 1)) {
        (Some(b'\r'), Some(b'\n')) | (Some(b'\n'), Some(b'\r')) => Some(2),
        (Some(b'\r' | b'\n'), _) => Some(1),
        _ => None,
    }
}

/// How many `quote` bytes run on from `start`, which is one of them.
fn toml_quote_run(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut index = start;
    while index < bytes.len() && bytes[index] == quote {
        index += 1;
    }
    index - start
}

/// Whether a quote at `index` follows a flow indicator, which is the other
/// place a scalar may begin: the quote of `[a,"b # c"]` opens one although no
/// white space precedes it (YAML 1.2.2, 7.4). Everywhere else an apostrophe or
/// a quote behind an ordinary byte is content of the plain scalar it sits in,
/// which is what keeps the one in `note: it's fine` from opening a literal
/// that would swallow the rest of the file.
fn yaml_flow_opener(bytes: &[u8], index: usize) -> bool {
    index > 0 && matches!(bytes[index - 1], b',' | b'[' | b'{')
}

/// Which trailing line breaks a block scalar keeps (YAML 1.2.2, 8.1.1.2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Chomping {
    /// `-`: the final line break and every empty line behind it are dropped.
    Strip,
    /// No indicator, and the default: the final line break stays and the empty
    /// lines behind it are dropped.
    Clip,
    /// `+`: the final line break and every empty line behind it are content,
    /// which is what makes a blank line under such a body change its value.
    Keep,
}

/// Where the node property beginning at `index` ends.
///
/// An anchor `&name` and a tag `!tag` run to the white space, the line, or the
/// flow indicator that ends them (YAML 1.2.2, 6.9 and 7.4); nothing else may
/// close one, which is what keeps `!!str` a single token.
fn yaml_property_end(bytes: &[u8], index: usize) -> usize {
    let mut cursor = index + 1;
    while cursor < bytes.len()
        && !matches!(
            bytes[cursor],
            b' ' | b'\t' | b'\r' | b'\n' | b',' | b'[' | b']' | b'{' | b'}'
        )
    {
        cursor += 1;
    }
    cursor
}

/// The block scalar header at `index`, which is its `|` or `>`: the explicit
/// indentation indicator, `None` where the header spells none out and the body
/// detects its own; the chomping indicator; where a comment on the header line
/// begins; and where that line ends.
///
/// The two readings of a missing indicator are not the same answer. The floor a
/// body line has to clear is the owner's column either way — an absent
/// indicator behaves as `1` for that — but the depth the body's *content* sits
/// at is written on the header only when the indicator is, and is otherwise
/// whatever the first non-empty line turns out to be.
///
/// `None` means the bytes are not a header at all and the indicator is content
/// of a plain scalar. That is the whole of what tells `key: >` from `key: a >
/// b`: a header is followed by its indicators, then white space, then at most
/// a comment, and then the line ends (YAML 1.2.2, 8.1.1). The comment needs
/// that white space in front of it like any other (6.6), so `key: |#c` is no
/// header either.
fn yaml_block_header(
    bytes: &[u8],
    index: usize,
) -> Option<(Option<usize>, Chomping, Option<usize>, usize)> {
    let mut cursor = index + 1;
    let mut indentation = None;
    let mut chomping = None;
    while let Some(byte) = bytes.get(cursor).copied() {
        match byte {
            b'1'..=b'9' if indentation.is_none() => indentation = Some(usize::from(byte - b'0')),
            b'+' if chomping.is_none() => chomping = Some(Chomping::Keep),
            b'-' if chomping.is_none() => chomping = Some(Chomping::Strip),
            _ => break,
        }
        cursor += 1;
    }
    let mut spaced = false;
    while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
        spaced = true;
        cursor += 1;
    }
    let comment = (spaced && bytes.get(cursor) == Some(&b'#')).then_some(cursor);
    if comment.is_some() {
        cursor = line_end(bytes, cursor);
    }
    bytes
        .get(cursor)
        .is_none_or(|byte| matches!(byte, b'\r' | b'\n'))
        .then(|| {
            (
                indentation,
                chomping.unwrap_or(Chomping::Clip),
                comment,
                cursor,
            )
        })
}

/// Where the body of the block scalar whose header line ends at `header_end`
/// ends, whether that offset is the start of a line, and how deep its content
/// turned out to sit.
///
/// A line belongs to the body while it is empty — an empty line is content of
/// the scalar (YAML 1.2.2, 8.1.1.2) whatever its indentation — or indented to
/// at least `body_min`. The first line that is neither ends it, and so does a
/// document marker in column zero (9.1.2, 9.1.3), which is what ends the body
/// of a scalar that is the whole document and therefore has no indentation to
/// fall short of.
///
/// The second of the three answers is what the caller turns into a checkpoint:
/// a body that ran out of file in the middle of a line ends nowhere a scan
/// could resume.
///
/// The third is the *detected* content indentation (8.1.1.1): the indentation
/// of the first non-empty line, which is what a parser measures every later
/// line against, and `body_min` when the body holds no non-empty line to
/// measure. It is never less than `body_min`, because a line shallower than
/// that would have ended the body instead of opening it.
fn yaml_block_body_end(bytes: &[u8], header_end: usize, body_min: usize) -> (usize, bool, usize) {
    if header_end >= bytes.len() {
        return (bytes.len(), false, body_min);
    }
    let mut index = consume_newline(bytes, header_end);
    let mut content = None;
    while index < bytes.len() {
        let (indent, blank, end) = yaml_line_shape(bytes, index);
        if !blank && (indent < body_min || yaml_document_marker(bytes, index)) {
            break;
        }
        if !blank && content.is_none() {
            content = Some(indent);
        }
        if end >= bytes.len() {
            return (bytes.len(), false, content.unwrap_or(body_min));
        }
        index = consume_newline(bytes, end);
    }
    (index, true, content.unwrap_or(body_min))
}

/// The indentation of the line beginning at `start`, whether it is empty, and
/// where it ends.
///
/// Indentation is spaces alone: a tab may not indent a line (YAML 1.2.2, 6.1),
/// so the first one ends the indentation and is content of whatever follows
/// it. A line of nothing but white space is empty even so, which is what keeps
/// a blank line inside a block scalar body from ending it.
fn yaml_line_shape(bytes: &[u8], start: usize) -> (usize, bool, usize) {
    let mut index = start;
    while bytes.get(index) == Some(&b' ') {
        index += 1;
    }
    let indent = index - start;
    let mut cursor = index;
    while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
        cursor += 1;
    }
    let blank = bytes
        .get(cursor)
        .is_none_or(|byte| matches!(byte, b'\r' | b'\n'));
    (indent, blank, line_end(bytes, cursor))
}

/// Whether the line beginning at `line_start` is a document marker: `---` or
/// `...` with the line or white space behind it. Both are read in column zero
/// alone, which is what `line_start` carries — a line with any indentation at
/// all begins with a space and matches neither.
fn yaml_document_marker(bytes: &[u8], line_start: usize) -> bool {
    (starts(bytes, line_start, b"---") || starts(bytes, line_start, b"..."))
        && bytes
            .get(line_start + 3)
            .is_none_or(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
}

/// The one comment that is all its line holds, as an index into `comments`.
///
/// `None` when the line holds none, holds one with something else on it, or
/// holds a comment that does not run to the end of the line — in each of those
/// the line survives a removal whatever else is decided about it.
fn comment_alone_on_line(
    source: &[u8],
    offset: usize,
    comments: &[Comment],
    line_start: usize,
    line_end: usize,
) -> Option<usize> {
    /* NOTE: `source` may be a suffix the scan was handed, so its indices run
     * `offset` behind the absolute spans a comment carries. Everything below
     * compares in the absolute frame and slices in the local one. */
    let (start, end) = (line_start + offset, line_end + offset);
    let index = comments
        .binary_search_by(|comment| {
            if comment.span.start < start {
                Ordering::Less
            } else if comment.span.start >= end {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .ok()?;
    let span = comments[index].span;
    (span.end == end
        && source[line_start..span.start - offset]
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t')))
    .then_some(index)
}

/// One YAML block scalar, as the two things the lines below it depend on.
///
/// Where a body ends is decided by the column of the node the header hangs
/// off, which is not written on the header's own line — `key:` on one line and
/// `|` on the next is the same scalar as `key: |`. Only a scan knows that
/// column, so this is recorded while one runs rather than re-derived from the
/// bytes afterwards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct YamlBlockScalar {
    /// The first byte past the body: the start of the first line that is not
    /// part of it, or the end of the source.
    body_end: usize,
    /// How deep the body's content sits: the explicit indentation indicator
    /// counted from the owner, or the indentation of the first non-empty line
    /// where the header spelled none out (YAML 1.2.2, 8.1.1.1). A line under
    /// the body that reaches this depth is content of it; one that does not is
    /// outside it whatever else is true, which is the difference between a
    /// trail comment a removal may take and one it may not.
    content_indent: usize,
    /// Which trailing line breaks the header asked to keep.
    chomping: Chomping,
}

/// The keep reason the scanner writes for a comment a YAML block scalar leans
/// on, and the one keep no option can overrule.
///
/// Frozen: the differential protocol compares this string byte for byte, and
/// `--explain` recognises the rule by it.
pub(crate) const YAML_STRUCTURAL_TRAIL: &str = "structural in a YAML block scalar trail";

/// Whether `disposition` is the keep [`YAML_STRUCTURAL_TRAIL`] names.
pub(crate) fn is_yaml_structural_trail(disposition: &Disposition) -> bool {
    matches!(disposition, Disposition::Keep { reason } if reason == YAML_STRUCTURAL_TRAIL)
}

/// Which comments in the trails of `blocks` no removal may take, as indices
/// into `comments`.
///
/// A block scalar body ends at the first line under it that is shallower than
/// its content (YAML 1.2.2, 8.1.1), and in a trail of whole-line comments that
/// line is a comment. Removing it — and a removal there takes the whole line,
/// which is the least a removal can leave — hands the lines under it back to
/// the body, and a line that reaches the content depth is content again. When
/// what comes back up is a comment the run keeps, no removal preserves the
/// value: the comment above it is not commentary but the thing that closes the
/// scalar, and it is kept.
///
/// Only the *first* comment of a trail can do that work, and it always can.
/// The line a body ended at is shallower than the floor and so shallower than
/// the content, so keeping it closes the scalar there and leaves everything
/// below outside — which is why one keep per block is both necessary and
/// enough, and why the deeper comments of the trail stay removable. Keeping a
/// deeper one instead would be no fix at all: it reaches the content depth
/// itself, so the body would swallow the survivor.
///
/// A trail whose every comment is removable needs none of this: with nothing
/// left standing under the body there is nothing for it to take back.
fn yaml_structural_trail_keeps(
    source: &[u8],
    offset: usize,
    blocks: &[YamlBlockScalar],
    comments: &[Comment],
) -> Vec<usize> {
    let mut keeps = Vec::new();
    for block in blocks {
        let Some(mut probe) = block.body_end.checked_sub(offset) else {
            continue;
        };
        /* INVARIANT: `shield` is the trail's first removable comment shallower
         * than the content — the one keep that would close the body — and is
         * set before any deeper line can be reached, because the line a body
         * ends at is shallower than the content by construction. */
        let mut shield = None;
        while probe < source.len() {
            let (indent, blank, end) = yaml_line_shape(source, probe);
            if blank {
                /* NOTE: An empty line is content of the body above whatever its
                 * indentation (8.1.1.2), so it neither ends the trail nor
                 * shields anything under it. */
                probe = past_terminator(source, end);
                continue;
            }
            let Some(found) = comment_alone_on_line(source, offset, comments, probe, end) else {
                /* NOTE: The first line with anything else on it is the next
                 * node, and it is not a line any removal here can move. */
                break;
            };
            if comments[found].disposition.is_remove() {
                if shield.is_none() && indent < block.content_indent {
                    shield = Some(found);
                }
            } else if indent < block.content_indent {
                /* NOTE: A surviving line shallower than the content closes the
                 * body on its own, so nothing above it is load-bearing. */
                break;
            } else {
                keeps.extend(shield);
                break;
            }
            probe = past_terminator(source, end);
        }
    }
    keeps
}

/// Apply [`yaml_structural_trail_keeps`] to comments that did not come from a
/// scan of this crate's own, which is the external hand-off of
/// [`transform_spans`](crate::transform_spans).
///
/// A scan reaches the same answer from the blocks it already walked over; this
/// is the same answer re-derived from the bytes, so the two paths cannot
/// disagree about a value.
pub(crate) fn keep_yaml_structural_trails(
    source: &[u8],
    language: Language,
    comments: &mut [Comment],
) {
    /* PERF: The same two answers `lines_a_removal_must_swallow` opens with: no
     * `|` and no `>` is no block scalar, and a file whose comments all trail
     * something has no whole-line comment to weigh. */
    if language != Language::Yaml || comments.is_empty() || memchr2(b'|', b'>', source).is_none() {
        return;
    }
    if !comments.iter().any(|comment| {
        comment.disposition.is_remove() && starts_its_line(source, comment.span.start)
    }) {
        return;
    }
    let blocks = yaml_block_scalars(source);
    for index in yaml_structural_trail_keeps(source, 0, &blocks, comments) {
        comments[index].disposition = Disposition::Keep {
            reason: YAML_STRUCTURAL_TRAIL.to_owned(),
        };
    }
}

/// Every block scalar in a YAML source, in order.
///
/// A scan of its own, so that the answer stays a function of the bytes alone
/// and an incremental rescan or an external hand-off reaches the same one with
/// no state to carry. It is the *scanner's* reading of a header, not the loose
/// one [`first_yaml_block_scalar`] uses: `key: a |+` ends a plain scalar with
/// two characters that look like a header, and reading it as one would hang a
/// phantom keep-chomped tail off a line that has no body at all.
fn yaml_block_scalars(source: &[u8]) -> Vec<YamlBlockScalar> {
    let mut scanner = Scanner::with_offset(
        source,
        Language::Yaml,
        ScanOptions::default(),
        0,
        false,
        None,
    );
    scanner.scan_yaml();
    scanner.yaml_blocks
}

/// Whether nothing but indentation stands between `start` and the beginning of
/// its line, which is the whole of what makes a comment a candidate for being
/// swallowed whole.
fn starts_its_line(source: &[u8], start: usize) -> bool {
    source[..start]
        .iter()
        .copied()
        .rev()
        .find(|byte| !matches!(byte, b' ' | b'\t'))
        .is_none_or(|byte| matches!(byte, b'\r' | b'\n'))
}

/// Where a line under a block scalar ends once its terminator is taken with it.
fn past_terminator(source: &[u8], line_end: usize) -> usize {
    if line_end >= source.len() {
        line_end
    } else {
        consume_newline(source, line_end)
    }
}

/// For each comment, the line a removal has to take whole — its terminator
/// included — instead of leaving the ordinary hole on it, or `None` where the
/// ordinary hole is right. An empty answer stands for all-`None`, which is
/// every language but YAML and nearly every YAML file.
///
/// YAML is the one language where the hole itself carries meaning, and the
/// reason is that a block scalar decides where its body ends from the lines
/// *below* it (YAML 1.2.2, 8.1.1). A whole-line comment under a body is
/// `l-trail-comments` and is not part of the value, but the hole left in its
/// place is read as one of two things:
///
/// * a line of spaces as wide as the comment, which `columns` writes, is
///   indented at least as deep as the body whenever the comment was wide
///   enough — and a line indented that deep *is* body content, so the scalar
///   silently grows a line;
/// * an empty line, which `lines` writes, is content under `|+` and `>+`,
///   where every empty line trailing a body is kept (8.1.1.2).
///
/// So every whole-line comment whose own line sits in the run of empty and
/// comment lines under a body is removed by taking the line, terminator and
/// all, under every layout — which is the line `compact` takes already. That
/// costs those lines their numbering under `lines` and their columns under
/// `columns`; the alternative costs the reader's value, and no indentation a
/// padded line could be given is safe, because the depth that would put it
/// outside one body is the depth that puts it inside the mapping the body
/// belongs to.
///
/// Under `|+` and `>+` the line is not enough on its own. The empty lines
/// *between* a removed comment and the next line are `l-comment` while the
/// comment shelters them and `l-keep-empty` once it is gone (8.1.1.2), so the
/// swallow runs on through them. The empty lines *above* the first comment are
/// already content and are left exactly where they are: a removal takes what
/// the comment was sheltering and nothing else.
///
/// The answer is a function of the source and the comments alone, so an
/// incremental rescan and an external hand-off reach the same one with no
/// state to carry.
pub(crate) fn lines_a_removal_must_swallow(
    source: &[u8],
    language: Language,
    comments: &[Comment],
) -> Vec<Option<ByteSpan>> {
    if language != Language::Yaml || comments.is_empty() {
        return Vec::new();
    }
    /* PERF: Two answers that cost almost nothing, in front of a scan of the
     * whole source. A file with no `|` and no `>` in it has no block scalar at
     * all; and a comment a body could swallow is one that is alone on its
     * line, which is a walk back over that line's indentation and no further —
     * the `# note` of `key: value # note` stops on the byte behind it. A YAML
     * file whose comments all trail something therefore never pays for the
     * scan below. */
    if memchr2(b'|', b'>', source).is_none() {
        return Vec::new();
    }
    if !comments.iter().any(|comment| {
        comment.disposition.is_remove() && starts_its_line(source, comment.span.start)
    }) {
        return Vec::new();
    }
    let blocks = yaml_block_scalars(source);
    if blocks.is_empty() {
        return Vec::new();
    }
    let mut answers = vec![None; comments.len()];
    for block in blocks {
        let mut probe = block.body_end;
        while probe < source.len() {
            let (_, blank, end) = yaml_line_shape(source, probe);
            if blank {
                /* NOTE: An empty line neither ends the run nor is taken on its
                 * own: it is content of the body above until a comment below
                 * it is removed, and only that removal may take it. */
                probe = past_terminator(source, end);
                continue;
            }
            let Some(found) = comment_alone_on_line(source, 0, comments, probe, end) else {
                /* NOTE: The first line with anything else on it is the next
                 * node, and the comments under *it* are that node's. */
                break;
            };
            if comments[found].disposition.is_remove() {
                let mut taken = past_terminator(source, end);
                if block.chomping == Chomping::Keep {
                    while taken < source.len() {
                        let (_, blank, run_end) = yaml_line_shape(source, taken);
                        if !blank {
                            break;
                        }
                        taken = past_terminator(source, run_end);
                    }
                }
                answers[found] = Some(ByteSpan::new(probe, taken));
            }
            probe = past_terminator(source, end);
        }
    }
    answers
}

fn starts(bytes: &[u8], index: usize, needle: &[u8]) -> bool {
    bytes.get(index..index.saturating_add(needle.len())) == Some(needle)
}

/// The two bytes that end a line everywhere a checkpoint may be offered.
///
/// INVARIANT: a bounded lookahead that decides a token consults this before it
/// reads one byte further: a checkpoint sits at the line start behind a
/// terminator, so a decision that crossed one would depend on bytes the
/// incremental engine is entitled to rescan on their own.
fn is_line_terminator(byte: u8) -> bool {
    matches!(byte, b'\r' | b'\n')
}

fn line_end(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && !matches!(bytes[index], b'\r' | b'\n') {
        index += 1;
    }
    index
}

pub(crate) fn unicode_line_terminator_width(bytes: &[u8], index: usize) -> Option<usize> {
    match bytes.get(index) {
        Some(b'\r') if bytes.get(index + 1) == Some(&b'\n') => Some(2),
        Some(b'\r' | b'\n') => Some(1),
        Some(0xe2)
            if bytes.get(index + 1) == Some(&0x80)
                && matches!(bytes.get(index + 2), Some(0xa8 | 0xa9)) =>
        {
            Some(3)
        }
        _ => None,
    }
}

fn js_line_end(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && unicode_line_terminator_width(bytes, index).is_none() {
        index += 1;
    }
    index
}

fn consume_newline(bytes: &[u8], index: usize) -> usize {
    if bytes.get(index) == Some(&b'\r') && bytes.get(index + 1) == Some(&b'\n') {
        index + 2
    } else {
        index + 1
    }
}

fn block_end(bytes: &[u8], start: usize, open: &[u8], close: &[u8], nested: bool) -> (usize, bool) {
    let mut index = start + open.len();
    let mut depth = 1usize;
    while index < bytes.len() {
        if nested && starts(bytes, index, open) {
            depth += 1;
            index += open.len();
        } else if starts(bytes, index, close) {
            depth -= 1;
            index += close.len();
            if depth == 0 {
                return (index, true);
            }
        } else {
            index += 1;
        }
    }
    (bytes.len(), false)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    memmem::find(haystack, needle)
}

fn rust_raw_start_at_quote(bytes: &[u8], quote: usize) -> Option<(usize, usize)> {
    let mut cursor = quote;
    while cursor > 0 && bytes[cursor - 1] == b'#' {
        cursor -= 1;
    }
    let hashes = quote - cursor;
    if cursor == 0 || bytes[cursor - 1] != b'r' {
        return None;
    }
    let mut start = cursor - 1;
    if start > 0 && matches!(bytes[start - 1], b'b' | b'c') {
        start -= 1;
    }
    if start > 0 && is_js_identifier_continue(bytes[start - 1]) {
        return None;
    }
    Some((start, hashes))
}

/// Whether the apostrophe at `index` opens a character literal rather than a
/// lifetime, told apart by a bounded lookahead.
///
/// INVARIANT: no window this reads may run past a line terminator. `scan_c_family`
/// offers a checkpoint at the line start behind every terminator, and a
/// checkpoint promises that nothing decided before it depends on bytes after
/// it — so a lookahead that read across one would let an edit on the next line
/// rewrite a token on this one while the incremental engine reused it
/// unchanged. Nothing is lost by stopping there: a Rust character literal ends
/// at the line (Rust Reference, Tokens), and `\` before a line terminator is a
/// string continuation, never a character escape, so every shape the window
/// would have reached across is already invalid Rust.
fn rust_char_start(bytes: &[u8], index: usize, reach: &mut Reach) -> bool {
    reach.byte(index + 1);
    let Some(next) = bytes.get(index + 1) else {
        return false;
    };
    if is_line_terminator(*next) {
        return false;
    }
    if *next == b'\\' {
        reach.through(line_bounded_reach(bytes, index + 2, 2));
        return bytes
            .get(index + 2)
            .is_some_and(|byte| !is_line_terminator(*byte))
            && bytes.get(index + 3..index + 4) == Some(b"'");
    }
    reach.byte(index + 2);
    if bytes.get(index + 2) == Some(&b'\'') {
        return true;
    }
    if *next & 0x80 == 0 {
        return false;
    }
    reach.through(line_bounded_reach(bytes, index + 1, 5));
    bytes[index + 1..]
        .iter()
        .take(5)
        .take_while(|byte| !is_line_terminator(**byte))
        .any(|byte| *byte == b'\'')
}

fn is_c_quote_start(bytes: &[u8], index: usize) -> bool {
    matches!(bytes[index], b'"' | b'\'')
        || (matches!(bytes[index], b'L' | b'u' | b'U')
            && matches!(bytes.get(index + 1), Some(b'"' | b'\'')))
        || (starts(bytes, index, b"u8\"") || starts(bytes, index, b"u8'"))
}

fn cpp_raw_string(bytes: &[u8], index: usize, reach: &mut Reach) -> Option<(usize, bool)> {
    let prefixes: [&[u8]; 5] = [b"R\"", b"u8R\"", b"uR\"", b"UR\"", b"LR\""];
    let Some(prefix) = prefixes.iter().find(|prefix| starts(bytes, index, prefix)) else {
        reach.through((index + 4).min(bytes.len()));
        return None;
    };
    let delimiter_start = index + prefix.len();
    /* INVARIANT: as in `ocaml_quoted_string`, and for the same reason. The
     * delimiter of a raw string literal is a run of at most 16 d-chars and the
     * `(` stands directly behind it ([lex.string]), so this reads that class
     * and one byte more rather than searching the document for a `(` that may
     * never come. An ordinary `R"` in the code then costs the lines under it
     * nothing. */
    let mut open = delimiter_start;
    while open < delimiter_start + 16 && bytes.get(open).is_some_and(|byte| is_cpp_d_char(*byte)) {
        open += 1;
    }
    /* NOTE: the byte that ended the class decided this, and a `get` that came
     * back `None` at the end of the document decided it just the same. */
    reach.byte(open);
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let mut close = Vec::with_capacity(open - delimiter_start + 2);
    close.push(b')');
    close.extend_from_slice(&bytes[delimiter_start..open]);
    close.push(b'"');
    Some(match find_subslice(&bytes[open + 1..], &close) {
        Some(relative) => {
            let end = open + 1 + relative + close.len();
            reach.through(end);
            (end, true)
        }
        /* NOTE: nothing is recorded here, and nothing is read past either: the
         * scan takes every byte this search crossed, so a rescan from a later
         * checkpoint would meet the same open literal and reach the same
         * answer. `parse_heredoc` is the one give-up path whose search the scan
         * then rewinds behind, which is why the `end_of` there is the one that
         * carries weight. */
        None => (bytes.len(), false),
    })
}

/// Whether `byte` may stand in the delimiter of a C++ raw string literal.
///
/// [lex.string]: a d-char is any member of the basic source character set
/// except space, `(`, `)`, `\`, and the control characters horizontal tab,
/// vertical tab, form feed and new-line. The vertical tab is in that list and
/// is not in `u8::is_ascii_whitespace`, so it is named here.
const fn is_cpp_d_char(byte: u8) -> bool {
    !matches!(
        byte,
        b' ' | b'(' | b')' | b'\\' | b'\t' | 0x0b | 0x0c | b'\n' | b'\r'
    )
}

fn cpp_raw_start_at_quote(bytes: &[u8], quote: usize) -> Option<usize> {
    for prefix in [b"R".as_slice(), b"u8R", b"uR", b"UR", b"LR"] {
        let Some(start) = quote.checked_sub(prefix.len()) else {
            continue;
        };
        if bytes.get(start..quote) == Some(prefix)
            && (start == 0 || !is_js_identifier_continue(bytes[start - 1]))
        {
            return Some(start);
        }
    }
    None
}

/// The end of the OCaml comment opening at `start`, and whether it closed.
///
/// A comment lexes the string and character literals inside it, so the
/// lookaheads that decide those are this one's as well: `reach` carries theirs
/// out, because a quoted-string tag search inside a comment can read past the
/// comment's own end.
fn ocaml_comment_end(bytes: &[u8], start: usize, reach: &mut Reach) -> (usize, bool) {
    let mut index = start + 2;
    let mut depth = 1;
    while index < bytes.len() {
        if starts(bytes, index, b"(*") {
            depth += 1;
            index += 2;
        } else if starts(bytes, index, b"*)") {
            depth -= 1;
            index += 2;
            if depth == 0 {
                return (index, true);
            }
        } else if bytes[index] == b'"' {
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == b'"' {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
        } else if let Some((end, _)) = ocaml_quoted_string(bytes, index, reach) {
            index = end;
        } else if bytes[index] == b'\'' && ocaml_char_start(bytes, index, reach) {
            let quote = bytes[index];
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == quote {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
        } else {
            index += 1;
        }
    }
    (bytes.len(), false)
}

fn ocaml_quoted_string(bytes: &[u8], index: usize, reach: &mut Reach) -> Option<(usize, bool)> {
    /* NOTE: the byte at `index` is the one the scan stands on and consumes
     * whichever way this goes, so reading it is no lookahead and nothing is
     * recorded for it. `scan_ocaml` asks this of every byte of a document, and
     * recording each one would push the watermark along in front of the scan
     * and take away every checkpoint the language has. */
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    /* INVARIANT: the tag of a quoted string literal is `[a-z_]*` and the `|`
     * stands directly behind it (OCaml manual, Lexical conventions), so this
     * reads the class and one byte more rather than searching the document for
     * a `|` that may never come. The bound is what the reach is for: an
     * ordinary `{` in OCaml code gives up at the first byte outside the class,
     * and the lines under it keep their checkpoints instead of losing them to
     * a search that crossed the whole file to say no. */
    let mut pipe = index + 1;
    while bytes
        .get(pipe)
        .is_some_and(|byte| byte.is_ascii_lowercase() || *byte == b'_')
    {
        pipe += 1;
    }
    /* NOTE: the byte that ended the class decided this, and a `get` that came
     * back `None` at the end of the document decided it just the same. */
    reach.byte(pipe);
    if bytes.get(pipe) != Some(&b'|') {
        return None;
    }
    let mut close = Vec::with_capacity(pipe - index + 1);
    close.push(b'|');
    close.extend_from_slice(&bytes[index + 1..pipe]);
    close.push(b'}');
    Some(match find_subslice(&bytes[pipe + 1..], &close) {
        Some(relative) => {
            let end = pipe + 1 + relative + close.len();
            reach.through(end);
            (end, true)
        }
        /* NOTE: nothing is recorded here, for the reason `cpp_raw_string`
         * gives: the scan consumes every byte this search crossed. */
        None => (bytes.len(), false),
    })
}

/// Whether the apostrophe at `index` opens an OCaml character literal.
///
/// INVARIANT: the same rule [`rust_char_start`] states — neither the two-byte
/// window for a bare character nor the eight-byte one for an escape may run
/// past a line terminator, because `scan_ocaml` offers a checkpoint at the line
/// start behind it. `'\` followed by a line terminator is an illegal backslash
/// escape (OCaml manual, Lexical conventions; `ocamlc` 5.5.0 rejects it), so
/// the escaped window gives up nothing valid by stopping. The bare window
/// crossing costs the one shape OCaml does accept — an apostrophe, a literal
/// newline, an apostrophe — which the scanner never read as a literal anyway:
/// it ends a character literal at the line, so that shape used to be reported
/// as an unterminated literal and is now simply not one.
fn ocaml_char_start(bytes: &[u8], index: usize, reach: &mut Reach) -> bool {
    reach.byte(index + 1);
    let Some(next) = bytes.get(index + 1) else {
        return false;
    };
    if is_line_terminator(*next) {
        return false;
    }
    reach.byte(index + 2);
    if bytes.get(index + 2) == Some(&b'\'') {
        return true;
    }
    if *next != b'\\' {
        return false;
    }
    reach.through(line_bounded_reach(bytes, index + 2, 6));
    bytes[index + 2..]
        .iter()
        .take(6)
        .take_while(|byte| !is_line_terminator(**byte))
        .any(|byte| *byte == b'\'')
}

fn python_string_start(bytes: &[u8], index: usize) -> Option<(usize, bool, bool, bool)> {
    if matches!(bytes[index], b'\'' | b'"') {
        return Some((
            index,
            starts(bytes, index, &[bytes[index]; 3]),
            false,
            false,
        ));
    }
    if !matches!(
        bytes[index].to_ascii_lowercase(),
        b'r' | b'u' | b'b' | b'f' | b't'
    ) {
        return None;
    }
    if index > 0 && (bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_') {
        return None;
    }
    let mut cursor = index;
    while cursor < bytes.len()
        && cursor - index < 3
        && matches!(
            bytes[cursor].to_ascii_lowercase(),
            b'r' | b'u' | b'b' | b'f' | b't'
        )
    {
        cursor += 1;
    }
    if cursor < bytes.len() && matches!(bytes[cursor], b'\'' | b'"') {
        let prefix = &bytes[index..cursor];
        Some((
            cursor,
            starts(bytes, cursor, &[bytes[cursor]; 3]),
            prefix
                .iter()
                .any(|byte| byte.eq_ignore_ascii_case(&b'f') || byte.eq_ignore_ascii_case(&b't')),
            prefix.iter().any(|byte| byte.eq_ignore_ascii_case(&b'r')),
        ))
    } else {
        None
    }
}

fn shell_single_quote_end(bytes: &[u8], start: usize) -> (usize, bool) {
    match bytes[start + 1..].iter().position(|byte| *byte == b'\'') {
        Some(relative) => (start + relative + 2, true),
        None => (bytes.len(), false),
    }
}

#[derive(Clone, Copy)]
enum ShellTerminator {
    Parenthesis(usize),
    Backtick(usize),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ShellCaseState {
    AwaitIn,
    Pattern,
    Body,
}

struct Heredoc {
    operator: usize,
    delimiter: Vec<u8>,
    strip_tabs: bool,
}

/// The here-document the `<<` at `index` opens, and the byte after its
/// delimiter word.
///
/// INVARIANT: a quoted delimiter word may legitimately span lines — `<<"EO`,
/// a line break, `F"` names the delimiter `EO\nF` — and so may an unquoted one
/// carrying a backslash-newline continuation, so this is a lookahead with no
/// line bound at all. `reach` carries out how far it read, because the paths
/// that give up (an unterminated quote, a backslash at the end of the
/// document) rewind the scan to the byte after the operator and lex those
/// bytes again from a state this parse already decided out of them. That the
/// re-lex happens to reach the same end today is a property of two lexers
/// agreeing, not a guarantee the checkpoints may rest on.
fn parse_heredoc(bytes: &[u8], index: usize, reach: &mut Reach) -> Option<(Heredoc, usize)> {
    reach.byte(index + 2);
    let strip_tabs = bytes.get(index + 2) == Some(&b'-');
    let mut cursor = index + if strip_tabs { 3 } else { 2 };
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace)
        && !matches!(bytes[cursor], b'\r' | b'\n')
    {
        cursor += 1;
    }
    reach.byte(cursor);
    let mut delimiter = Vec::new();
    let mut quote = None;
    let mut saw_word = false;
    while cursor < bytes.len() {
        reach.byte(cursor);
        let byte = bytes[cursor];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
                cursor += 1;
            } else if active == b'"' && byte == b'\\' {
                reach.byte(cursor + 1);
                let escaped = *bytes.get(cursor + 1)?;
                if escaped == b'\r' {
                    reach.byte(cursor + 2);
                }
                if escaped == b'\r' && bytes.get(cursor + 2) == Some(&b'\n') {
                    cursor += 3;
                } else if matches!(escaped, b'\r' | b'\n') {
                    cursor += 2;
                } else if matches!(escaped, b'$' | b'`' | b'"' | b'\\') {
                    delimiter.push(escaped);
                    cursor += 2;
                } else {
                    delimiter.extend_from_slice(&[b'\\', escaped]);
                    cursor += 2;
                }
            } else {
                delimiter.push(byte);
                cursor += 1;
            }
            continue;
        }
        /* NOTE: The delimiter is a word (POSIX Shell Command Language, 2.7.4),
         * and a word ends at an unquoted operator character. `>` is one:
         * `cat <<EOF>out` is a here-document named `EOF` and a redirection,
         * not a here-document named `EOF>out`. */
        if byte.is_ascii_whitespace()
            || matches!(byte, b';' | b'|' | b'&' | b'(' | b')' | b'<' | b'>')
        {
            break;
        }
        match byte {
            b'\'' | b'"' => {
                saw_word = true;
                quote = Some(byte);
                cursor += 1;
            }
            b'\\' => {
                saw_word = true;
                reach.byte(cursor + 1);
                let escaped = *bytes.get(cursor + 1)?;
                if escaped == b'\r' {
                    reach.byte(cursor + 2);
                }
                if escaped == b'\r' && bytes.get(cursor + 2) == Some(&b'\n') {
                    cursor += 3;
                } else {
                    if !matches!(escaped, b'\r' | b'\n') {
                        delimiter.push(escaped);
                    }
                    cursor += 2;
                }
            }
            _ => {
                saw_word = true;
                delimiter.push(byte);
                cursor += 1;
            }
        }
    }
    if cursor >= bytes.len() {
        reach.end_of(bytes);
    }
    if !saw_word || quote.is_some() {
        return None;
    }
    Some((
        Heredoc {
            operator: index,
            delimiter,
            strip_tabs,
        },
        cursor,
    ))
}

fn heredoc_body_end(bytes: &[u8], mut index: usize, heredoc: &Heredoc) -> Option<usize> {
    while index <= bytes.len() {
        let end = line_end(bytes, index);
        let mut line = &bytes[index..end];
        if heredoc.strip_tabs {
            let first = line
                .iter()
                .position(|byte| *byte != b'\t')
                .unwrap_or(line.len());
            line = &line[first..];
        }
        if line == heredoc.delimiter {
            return Some(if end < bytes.len() {
                consume_newline(bytes, end)
            } else {
                end
            });
        }
        if end == bytes.len() {
            break;
        }
        index = consume_newline(bytes, end);
    }
    None
}

fn sql_quoted_end(bytes: &[u8], start: usize, quote: u8, backslash_escapes: bool) -> (usize, bool) {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == quote && bytes.get(index + 1) == Some(&quote) {
            index += 2;
        } else if bytes[index] == quote {
            return (index + 1, true);
        } else if backslash_escapes && bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
        } else {
            index += 1;
        }
    }
    (index, false)
}

fn postgres_escape_string_start(bytes: &[u8], quote: usize) -> bool {
    quote > 0
        && matches!(bytes[quote - 1], b'e' | b'E')
        && (quote == 1 || !is_js_identifier_continue(bytes[quote - 2]))
}

fn mysql_dash_comment_boundary(next: Option<u8>) -> bool {
    next.is_none_or(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

fn sql_identifier_end(bytes: &[u8], start: usize, close: u8) -> (usize, bool) {
    let actual_close = if bytes[start] == b'[' { b']' } else { close };
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == actual_close && bytes.get(index + 1) == Some(&actual_close) {
            index += 2;
        } else if bytes[index] == actual_close {
            return (index + 1, true);
        } else {
            index += 1;
        }
    }
    (index, false)
}

fn sql_dollar_quote_end(bytes: &[u8], start: usize, reach: &mut Reach) -> Option<(usize, bool)> {
    /* INVARIANT: as in `ocaml_quoted_string`, and for the same reason. The tag
     * of a dollar-quoted string is empty or an identifier —
     * `[A-Za-z_][A-Za-z0-9_]*` (PostgreSQL 4.1.2.4) — and the second `$`
     * stands directly behind it, so this reads that class and one byte more.
     * An ordinary `$` in a query then costs the lines under it nothing. */
    let mut second = start + 1;
    if bytes
        .get(second)
        .is_some_and(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'_'))
    {
        second += 1;
        while bytes
            .get(second)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            second += 1;
        }
    }
    reach.byte(second);
    if bytes.get(second) != Some(&b'$') {
        return None;
    }
    let delimiter = &bytes[start..=second];
    Some(match find_subslice(&bytes[second + 1..], delimiter) {
        Some(relative) => {
            let end = second + 1 + relative + delimiter.len();
            reach.through(end);
            (end, true)
        }
        /* NOTE: nothing is recorded here, for the reason `cpp_raw_string`
         * gives: the scan consumes every byte this search crossed. */
        None => (bytes.len(), false),
    })
}

fn oracle_q_quote_end(bytes: &[u8], start: usize, reach: &mut Reach) -> Option<(usize, bool)> {
    reach.byte(start + 1);
    if bytes.get(start + 1) != Some(&b'\'') {
        return None;
    }
    reach.byte(start + 2);
    let open = *bytes.get(start + 2)?;
    let close = match open {
        b'[' => b']',
        b'{' => b'}',
        b'(' => b')',
        b'<' => b'>',
        other => other,
    };
    let token = [close, b'\''];
    Some(match find_subslice(&bytes[start + 3..], &token) {
        Some(relative) => {
            let end = start + 3 + relative + 2;
            reach.through(end);
            (end, true)
        }
        /* NOTE: nothing is recorded here, for the reason `cpp_raw_string`
         * gives: the scan consumes every byte this search crossed. */
        None => (bytes.len(), false),
    })
}

/// Whether the `-->` at `index` closes an HTML-like comment: ECMA-262 12.5
/// makes one of a `-->` that nothing but white space precedes on its line.
///
/// U+FEFF is `<ZWNBSP>`, which 12.2 lists among `WhiteSpace` wherever it sits
/// and however many of it there are — the start of a file is only the most
/// common place to meet one. It takes three bytes, which is why the prefix is
/// walked rather than handed to [`js_is_space`] byte by byte.
fn js_html_close_comment(bytes: &[u8], index: usize) -> bool {
    if !starts(bytes, index, b"-->") {
        return false;
    }
    let mut cursor = bytes[..index]
        .iter()
        .rposition(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(0, |position| position + 1);
    while cursor < index {
        if starts(bytes, cursor, b"\xef\xbb\xbf") {
            cursor += 3;
        } else if js_is_space(bytes[cursor]) {
            cursor += 1;
        } else {
            return false;
        }
    }
    true
}

fn js_regex_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    let mut class = false;
    while index < bytes.len() {
        if unicode_line_terminator_width(bytes, index).is_some() {
            return None;
        }
        match bytes[index] {
            b'\\' => {
                let escaped = index + 1;
                if unicode_line_terminator_width(bytes, escaped).is_some() {
                    return None;
                }
                index = (index + 2).min(bytes.len());
            }
            b'[' => {
                class = true;
                index += 1;
            }
            b']' => {
                class = false;
                index += 1;
            }
            b'/' if !class => {
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphabetic() || bytes[index] == b'_')
                {
                    index += 1;
                }
                return Some(index);
            }
            _ => index += 1,
        }
    }
    None
}

fn is_js_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$') || byte & 0x80 != 0
}
fn is_js_identifier_continue(byte: u8) -> bool {
    is_js_identifier_start(byte) || byte.is_ascii_digit()
}

fn is_js_control_keyword(token: &[u8]) -> bool {
    matches!(
        token,
        b"if" | b"while" | b"for" | b"with" | b"switch" | b"catch"
    )
}

fn jsx_open(bytes: &[u8], index: usize) -> bool {
    bytes.get(index) == Some(&b'<')
        && bytes
            .get(index + 1)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'>' | b'_'))
}

fn html_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    let mut quote = None;
    while index < bytes.len() {
        if let Some(active) = quote {
            if bytes[index] == active {
                quote = None;
            }
        } else if matches!(bytes[index], b'\'' | b'"') {
            quote = Some(bytes[index]);
        } else if bytes[index] == b'>' {
            return Some(index + 1);
        }
        index += 1;
    }
    None
}

fn html_tag_candidate(bytes: &[u8], start: usize) -> bool {
    match bytes.get(start + 1).copied() {
        Some(byte) if byte.is_ascii_alphabetic() || matches!(byte, b'!' | b'?') => true,
        Some(b'/') => bytes
            .get(start + 2)
            .is_some_and(|byte| byte.is_ascii_alphabetic()),
        _ => false,
    }
}

fn html_embedded_start(bytes: &[u8], start: usize) -> Option<(&'static [u8], Language)> {
    let rest = &bytes[start..];
    if starts_ascii_case(rest, b"<script") && tag_boundary(rest.get(7).copied()) {
        Some((b"script", Language::JavaScript))
    } else if starts_ascii_case(rest, b"<style") && tag_boundary(rest.get(6).copied()) {
        Some((b"style", Language::Css))
    } else {
        None
    }
}

fn find_html_close(bytes: &[u8], content_start: usize, name: &[u8]) -> Option<usize> {
    let mut close = Vec::with_capacity(name.len() + 2);
    close.extend_from_slice(b"</");
    close.extend_from_slice(name);
    let mut cursor = content_start;
    while cursor + close.len() <= bytes.len() {
        let relative = find_ascii_case(&bytes[cursor..], &close)?;
        let candidate = cursor + relative;
        if tag_boundary(bytes.get(candidate + close.len()).copied()) {
            return Some(candidate);
        }
        cursor = candidate + close.len();
    }
    None
}

/// The offset PHP mode begins at when an opening tag starts at `index`, or
/// `None` when none does.
///
/// `<?php` is matched without regard to case and has to be followed by white
/// space or the end of the file (`zend_language_scanner.l`:
/// `"<?php"([ \t]|{NEWLINE})`), so `<?phpinfo()` is inline text. `<?=` is the
/// short echo tag and needs nothing behind it. A bare `<?` opens nothing,
/// because `short_open_tag` is off by default — which is what leaves `<?xml`
/// an XML declaration in the output rather than the start of a program.
fn php_open_tag(bytes: &[u8], index: usize) -> Option<usize> {
    if starts(bytes, index, b"<?=") {
        return Some(index + 3);
    }
    let rest = bytes.get(index..)?;
    if starts_ascii_case(rest, b"<?php")
        && rest
            .get(5)
            .is_none_or(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        return Some(index + 5);
    }
    None
}

/// Where a PHP `//` or `#` comment ends: at the line break, or at a closing
/// tag, whichever comes first (PHP manual, Comments — "the closing tag breaks
/// out of PHP mode"). The `?>` is not part of the comment.
fn php_line_comment_end(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len()
        && !matches!(bytes[index], b'\r' | b'\n')
        && !starts(bytes, index, b"?>")
    {
        index += 1;
    }
    index
}

/// The kind of a PHP block comment.
///
/// The tokenizer makes a documentation comment of `/**` only when white space
/// follows it — its rule is `"/*"|"/**"{WHITESPACE}`, and the longer
/// alternative is what sets `T_DOC_COMMENT` — so `/**/` and `/**text*/` are
/// ordinary block comments. `/*!` is Doxygen's marker and means nothing to
/// PHP's own tooling, so it is an ordinary comment too.
fn php_block_kind(bytes: &[u8], index: usize) -> CommentKind {
    if starts(bytes, index, b"/**")
        && bytes
            .get(index + 3)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        CommentKind::DocBlock
    } else {
        CommentKind::Block
    }
}

/// The offset just past the `}` that closes the interpolation opening at
/// `brace`, or the end of the file when none does.
///
/// The complex syntax `{$...}` holds a PHP expression, which the engine lexes
/// as ordinary code. This balances its braces instead, skipping over the two
/// things inside one that can carry a brace of their own — a nested string and
/// a comment — so `"{$a['}']}"` ends where PHP ends it. Nothing else in an
/// expression can, which is what makes the count right rather than merely
/// close.
///
/// What it does *not* do is report the comment it skipped: reading one out of a
/// string would mean running the whole lexer inside one, and v1 leaves those
/// bytes alone instead.
fn php_interpolation_end(bytes: &[u8], brace: usize) -> usize {
    let mut depth = 1usize;
    let mut index = brace + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            quote @ (b'\'' | b'"' | b'`') => {
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    index = if bytes[index] == b'\\' {
                        (index + 2).min(bytes.len())
                    } else {
                        index + 1
                    };
                }
            }
            b'/' if starts(bytes, index, b"/*") => {
                index = block_end(bytes, index, b"/*", b"*/", false).0;
                continue;
            }
            b'/' if starts(bytes, index, b"//") => {
                index = php_line_comment_end(bytes, index + 2);
                continue;
            }
            b'#' if bytes.get(index + 1) != Some(&b'[') => {
                index = php_line_comment_end(bytes, index + 1);
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    index.min(bytes.len())
}

/// Whether `byte` may open a PHP label: a letter, `_`, or any byte from `0x80`
/// up (PHP manual, Variables — the label grammar is byte-oriented and takes
/// the whole upper half of the range).
fn php_label_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// Whether `byte` may continue a PHP label: [`php_label_start`] and the
/// digits.
fn php_label_continue(byte: u8) -> bool {
    php_label_start(byte) || byte.is_ascii_digit()
}

/// The label, the offset its body starts at, and whether it is a nowdoc, for
/// the heredoc header opening at `start`; `None` when those three bytes head
/// no header.
///
/// The header is `<<<`, blanks, the label — bare, or quoted with `'` for a
/// nowdoc or `"` for a heredoc — and then the line break, with nothing else
/// allowed in between (`zend_language_scanner.l`:
/// `"<<<"{TABS_AND_SPACES}({LABEL}|(['"]{LABEL}['"])){NEWLINE}`). The body
/// begins on the next line.
fn php_heredoc_header(bytes: &[u8], start: usize) -> Option<(&[u8], usize, bool)> {
    let mut cursor = start + 3;
    while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
        cursor += 1;
    }
    let quote = match bytes.get(cursor) {
        Some(byte @ (b'\'' | b'"')) => Some(*byte),
        _ => None,
    };
    if quote.is_some() {
        cursor += 1;
    }
    let label_start = cursor;
    if !bytes.get(cursor).is_some_and(|byte| php_label_start(*byte)) {
        return None;
    }
    while bytes
        .get(cursor)
        .is_some_and(|byte| php_label_continue(*byte))
    {
        cursor += 1;
    }
    let label = &bytes[label_start..cursor];
    if let Some(quote) = quote {
        if bytes.get(cursor) != Some(&quote) {
            return None;
        }
        cursor += 1;
    }
    if !matches!(bytes.get(cursor), Some(b'\r' | b'\n')) {
        return None;
    }
    Some((label, consume_newline(bytes, cursor), quote == Some(b'\'')))
}

/// The offset just past the closing label of the body starting at `index`, or
/// `None` when no line closes it.
///
/// Since PHP 7.3 the closing label may be indented by blanks and may be
/// followed by anything that cannot continue a label — `;`, `,`, `)`, an
/// operator, the line break, or the end of the file (PHP manual, Heredoc
/// text). A byte that *can* continue one leaves the line ordinary body, which
/// is what keeps `EOTX` from ending an `EOT`.
fn php_heredoc_end(bytes: &[u8], mut index: usize, label: &[u8]) -> Option<usize> {
    loop {
        let mut cursor = index;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
            cursor += 1;
        }
        if starts(bytes, cursor, label)
            && !bytes
                .get(cursor + label.len())
                .is_some_and(|byte| php_label_continue(*byte))
        {
            return Some(cursor + label.len());
        }
        let end = line_end(bytes, index);
        if end >= bytes.len() {
            return None;
        }
        index = consume_newline(bytes, end);
    }
}

/// Where a Ruby token may begin, which is what decides whether `/`, `%`, `?`
/// and `<<` open a literal or are the operator spelled with the same byte.
///
/// This is Ruby's own `lex_state` folded onto the three answers those four
/// questions read out of it: `IS_BEG()`, `IS_END()`, and the `IS_ARG()` in
/// between, where a bare word may be a method about to take a command argument
/// and only the spacing around the byte says which. Ruby's lexer tells a local
/// variable from a method name by the symbol table it is building, which a
/// scanner has not got, so every bare word lands in [`Self::Argument`]: `a /b/`
/// is read as a regular expression where Ruby, knowing `a` to be a variable,
/// reads a division. That reading keeps more bytes inside a literal than the
/// parser would, which loses no comment that is one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RubyState {
    /// A value is expected: the start of a file, or just past an operator, a
    /// comma, an opening bracket, a keyword that opens an expression, or a
    /// line break.
    Begin,
    /// Just past a bare word, which may be a method taking a command argument.
    Argument,
    /// Just past an operand: a literal, a `)`, `]` or `}`, a variable, or one
    /// of the keywords that finishes an expression.
    End,
    /// Just past `alias` or `undef`, where Ruby stands in
    /// `EXPR_FNAME|EXPR_FITEM`. It answers every question [`Self::End`]
    /// answers, and one differently: `parse_percent` opens a symbol literal on
    /// `%s` there, spacing or none.
    Fname,
}

/// The header of one Ruby `%` literal: what closes it, whether it nests, and
/// whether it interpolates.
#[derive(Clone, Copy)]
struct RubyPercent {
    /// The letter naming the form, or `Q` for the bare `%(...)`.
    form: u8,
    /// The opening delimiter, equal to `close` where the delimiter does not
    /// pair and so does not nest.
    open: u8,
    /// The delimiter that ends the literal.
    close: u8,
    /// The offset of the first byte of the content.
    content: usize,
    /// Whether a `#{ ... }` inside it is an expression.
    interpolates: bool,
}

/// One Ruby here document, as the lines under the one that opened it need it.
struct RubyHeredoc {
    /// Where the `<<` sits, which is what an unterminated body is reported
    /// from.
    operator: usize,
    /// The terminator word, without the quotes that may have written it.
    label: Vec<u8>,
    /// `<<-` and `<<~` let the terminator line be indented; a bare `<<` wants
    /// it at column zero.
    indented: bool,
    /// A single-quoted terminator turns interpolation off; every other form
    /// leaves it on.
    interpolates: bool,
}

/// Ruby's `is_identchar` for the first byte of a name: a letter, `_`, or the
/// lead byte of a character outside ASCII, which Ruby takes as a name byte
/// wholesale.
fn ruby_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || !byte.is_ascii()
}

/// The same for every byte after the first, which a digit may also be.
fn ruby_identifier_continue(byte: u8) -> bool {
    ruby_identifier_start(byte) || byte.is_ascii_digit()
}

/// White space that separates Ruby tokens without ending a line. The vertical
/// tab and the form feed are in it, as `rb_isspace` has them; the two line
/// terminators are handled on their own, because they finish a statement.
fn ruby_is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0b | 0x0c)
}

/// Past the name at `index`.
fn ruby_identifier_end(bytes: &[u8], mut index: usize) -> usize {
    while bytes
        .get(index)
        .is_some_and(|byte| ruby_identifier_continue(*byte))
    {
        index += 1;
    }
    index
}

/// Past the bare word at `index`, including the `?` or `!` that may end a
/// method name.
///
/// Ruby's lexer takes a trailing `?` or `!` into the name unless a `=` follows
/// it, which is what tells `x.empty?` from the ternary `x ? y : z` — and, in
/// the other direction, keeps `a != b` a comparison rather than a call of a
/// method named `a!`.
fn ruby_word_end(bytes: &[u8], index: usize) -> usize {
    let end = ruby_identifier_end(bytes, index + 1);
    if matches!(bytes.get(end), Some(b'?' | b'!')) && bytes.get(end + 1) != Some(&b'=') {
        end + 1
    } else {
        end
    }
}

/// Past the numeric literal at `index`.
///
/// The digits, the `_` separators, the radix letters and the `r` and `i`
/// suffixes are one run of name bytes; a `.` joins the run only when a digit
/// follows it, which is what keeps `1.times` a method call.
fn ruby_number_end(bytes: &[u8], mut index: usize) -> usize {
    loop {
        index = ruby_identifier_end(bytes, index);
        if bytes.get(index) == Some(&b'.') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
            index += 1;
            continue;
        }
        return index;
    }
}

/// The state a bare word leaves the lexer in.
///
/// The three lists are Ruby's own keyword table folded onto [`RubyState`].
/// `def`, `alias` and `undef` refuse a literal because the name that follows
/// one may be spelled `/` or `%` — `def /(other)` defines division; `class`
/// and `module` are in the first list for the mirror-image reason, that `class
/// <<self` is a singleton class and never a here document. `alias` and `undef`
/// take [`RubyState::Fname`] rather than [`RubyState::End`] because they leave
/// Ruby in `EXPR_FNAME|EXPR_FITEM`, which is one answer wider. `super`,
/// `yield`, `not` and `defined?` are in none of the three, which leaves them
/// where Ruby has them: a command that may take an argument.
///
/// [`RubyState::End`] is a coarser answer than either reason asked for, and it
/// is worth naming what the coarseness costs. What is measured is this table
/// against `Ripper.lex`, keyword by keyword: `End` answers all four of the
/// state machine's questions at once, so it also decides `/`, `%` and `?`, and
/// the four readings below are the ones where the answer it gives is not
/// Ruby's.
///
/// * After `def`, `End` is Ruby's own answer for `/` and for every percent
///   literal, which `EXPR_FNAME` reads as the method names they are. `def` has
///   no exception: `def%s(foo)` is `on_op "%"` then `on_ident "s"` to Ruby
///   3.3.12, the modulo this table reads as well.
/// * After `class` and `module`, `End` is not Ruby's answer either:
///   `EXPR_CLASS` expects a value, so Ruby opens a literal there. `class
///   /x # c/` is one regular expression to Ruby 3.3.12 (`Ripper.lex` gives
///   `on_regexp_beg`) and a division with a comment behind it here.
/// * `?` diverges after all five — `class ?# x` and `def ?# x` are `on_CHAR
///   "?#"` to Ruby and a comment opener here.
///
/// Both of those are spellings Ruby itself refuses: `class` and `module` take a
/// constant, a `::` or a `<<` in any program that parses, and `def ?# x` is a
/// syntax error, so both are reachable only where the scan is already reading a
/// broken file, and buying them back with a state of their own — one that keeps
/// the literal readings and refuses only the here document — would add a state
/// to the machine for no program that runs.
///
/// [`RubyState::Fname`] is the state that *is* worth its keep, and `%s` after
/// `alias` or `undef` is what buys it: `alias%s(baz # x) %s(bar)` is a file
/// Ruby runs, and `Ripper.lex` under Ruby 3.3.12 gives `on_symbeg "%s("` in
/// state `FNAME|FITEM` for both names with `baz # x` an `on_tstring_content`.
/// Reading that `#` as a comment would remove bytes Ruby has inside a symbol,
/// which is the one direction this scanner may not take. The rule is about the
/// delimiter and not only the state — `alias` goes on refusing `/`, `<<`, `%w`
/// and `%q` in the same breath it accepts `%s` (`alias%w[a]` is `on_op "%"` to
/// Ripper) — so [`ruby_percent_opens`] asks it rather than
/// [`ruby_literal_opens`] alone.
///
/// `<<` itself is not an entry on that list, and what keeps it off is not
/// this function. A header [`RubyState::End`] refuses is a shift, which reads
/// no fewer bytes into a literal than Ruby does; a header it allows queues a
/// body, and
/// [`Scanner::scan_ruby_code`] queues it for the physical line the header
/// stands on wherever that header was written — before an interpolation, inside
/// one, inside a nested one, or inside an interpolation on another here
/// document's body line. A queue that stopped at an interpolation boundary
/// would read a whole here document body as code, which would be one more
/// reading that takes fewer bytes into a literal than Ruby does, and it is the
/// one the corpus cases named `ruby-heredoc-*-interpolation` hold shut.
///
/// NOTE: `End` is as far as that first half reaches, and `def`, `alias` and
/// `undef` are where it stops. MRI's `parser_yylex` tries `heredoc_identifier`
/// on `<<` unless the lexer state is `EXPR_DOT|EXPR_CLASS`, unless `IS_END()`,
/// or unless it is an `IS_ARG()` with no white space in front — and
/// `EXPR_FNAME` is in none of those three, so the state `def` leaves Ruby in,
/// and the `EXPR_FNAME|EXPR_FITEM` that `alias` and `undef` do, still reach a
/// here document. This table answers `End` for `def` and [`RubyState::Fname`]
/// for the other two, and both refuse the header: `def <<EOS` is a shift here
/// and a here-document header to MRI, which is the direction — fewer bytes into
/// a literal than Ruby takes — that the rest of this file refuses. What bounds
/// it is what bounds the `class` and `module` readings above: no program that
/// runs is written that way, because `def` is followed by a method name and
/// `<<EOS` is not one. `class` and `module` are not part of this exception at
/// all — `EXPR_CLASS` is named in that guard, which is what makes `class
/// <<self` a singleton class rather than a here document. Unlike every other
/// reading in this comment, the `def <<EOS` one is argued from MRI's `parse.y`
/// alone and has not been put to `Ripper.lex`: no Ruby 3.3 was available where
/// it was written.
fn ruby_state_after_word(token: &[u8]) -> RubyState {
    match token {
        b"end" | b"self" | b"nil" | b"true" | b"false" | b"redo" | b"retry" | b"__FILE__"
        | b"__LINE__" | b"__ENCODING__" | b"def" | b"class" | b"module" => RubyState::End,
        b"alias" | b"undef" => RubyState::Fname,
        b"if" | b"unless" | b"while" | b"until" | b"case" | b"when" | b"in" | b"and" | b"or"
        | b"return" | b"break" | b"next" | b"then" | b"do" | b"else" | b"elsif" | b"begin"
        | b"ensure" | b"rescue" | b"for" => RubyState::Begin,
        _ => RubyState::Argument,
    }
}

/// Whether the delimiter at `index` opens a literal rather than being the
/// operator spelled with the same byte.
///
/// Ruby's rule for both `/` and `%` (`parse_slash`, `parse_percent`) is one
/// rule: where a value is expected the byte always opens a literal; after an
/// operand it never does; and in between it opens one exactly when white space
/// stands before it and none behind it, which is what tells the command
/// argument of `puts /x/` from the division in `a / b`. `/=` and `%=` are
/// recognised before that last test, so an assignment operator is never read as
/// a literal outside value position.
fn ruby_literal_opens(state: RubyState, space_seen: bool, bytes: &[u8], index: usize) -> bool {
    match state {
        RubyState::Begin => true,
        RubyState::End | RubyState::Fname => false,
        RubyState::Argument => {
            space_seen
                && bytes.get(index + 1).is_some_and(|byte| {
                    *byte != b'=' && !ruby_is_space(*byte) && !matches!(byte, b'\r' | b'\n')
                })
        }
    }
}

/// Whether a `<<` where the lexer stands may open a here document.
///
/// Ruby's `parser_yylex`: never after an operand, and after a bare word only
/// when white space stands in front of it — which is why `a << b` is a shift
/// and `a <<b` is the here document that spacing exists to avoid.
fn ruby_heredoc_may_open(state: RubyState, space_seen: bool) -> bool {
    match state {
        RubyState::Begin => true,
        RubyState::Argument => space_seen,
        RubyState::End | RubyState::Fname => false,
    }
}

/// Whether the `%` at `index` opens the percent literal `form` names.
///
/// [`ruby_literal_opens`] answers it everywhere but one: `parse_percent` tests
/// `IS_lex_state(EXPR_FNAME | EXPR_FITEM)` before it reaches the spacing rule
/// and opens a symbol literal on `%s` there, so `alias%s(a)` and `alias %s(a)`
/// open one alike. Only `s` does; `%w`, `%q` and the rest fall through to the
/// ordinary answer, which is `false` in that state.
fn ruby_percent_opens(
    state: RubyState,
    space_seen: bool,
    bytes: &[u8],
    index: usize,
    form: u8,
) -> bool {
    (state == RubyState::Fname && form == b's')
        || ruby_literal_opens(state, space_seen, bytes, index)
}

/// Whether `index` is the first byte of a line, which is where Ruby's two
/// column-zero markers — `=begin` and `__END__` — are recognised.
///
/// A byte order mark is consumed before the first line is read, so the byte
/// behind one still opens the first line. That clause is asked only of a scan
/// starting at offset zero, because the first byte a suffix scan is handed
/// opens a line whatever it is, and a mark cannot stand there in the document
/// the suffix came from.
fn ruby_at_line_start(bytes: &[u8], index: usize, offset: usize) -> bool {
    if index == 0 || (offset == 0 && index == byte_order_mark_width(bytes)) {
        return true;
    }
    match bytes[index - 1] {
        b'\n' => true,
        b'\r' => bytes.get(index) != Some(&b'\n'),
        _ => false,
    }
}

/// Whether a `=begin` at `index` opens an embedded document.
///
/// Ruby's `word_match_p`: the word ends at white space or at the end of the
/// file, so `=beginner` is the `=` operator and a name.
fn ruby_embedded_document(bytes: &[u8], index: usize) -> bool {
    starts(bytes, index, b"=begin") && ruby_word_boundary(bytes, index + b"=begin".len())
}

/// Where the embedded document opened at `start` ends, and whether its `=end`
/// was there at all.
///
/// The document runs to the end of the `=end` line, whose remaining bytes Ruby
/// skips along with the rest of it, and both markers stand at column zero.
fn ruby_embedded_document_end(bytes: &[u8], start: usize) -> (usize, bool) {
    let mut index = line_end(bytes, start);
    while index < bytes.len() {
        index = consume_newline(bytes, index);
        if starts(bytes, index, b"=end") && ruby_word_boundary(bytes, index + b"=end".len()) {
            return (line_end(bytes, index + b"=end".len()), true);
        }
        index = line_end(bytes, index);
    }
    (bytes.len(), false)
}

/// Whether `index` is past the end of a word: white space, a line break, or the
/// end of the file.
fn ruby_word_boundary(bytes: &[u8], index: usize) -> bool {
    bytes
        .get(index)
        .is_none_or(|byte| ruby_is_space(*byte) || matches!(byte, b'\r' | b'\n'))
}

/// Whether a `__END__` alone on its line begins the DATA section at `index`.
///
/// Ruby's `whole_match_p`: the marker is the whole line, so `__END__ x` is an
/// ordinary name and the source runs on past it.
fn ruby_data_marker(bytes: &[u8], index: usize) -> bool {
    starts(bytes, index, b"__END__")
        && matches!(
            bytes.get(index + b"__END__".len()),
            None | Some(b'\r' | b'\n')
        )
}

/// Whether the byte behind a `:` makes it the head of a symbol rather than the
/// operator of a ternary or the colon of a hash label.
fn ruby_symbol_head(byte: u8) -> bool {
    ruby_identifier_start(byte) || matches!(byte, b'@' | b'$') || ruby_symbol_operator(byte)
}

/// The characters an operator method is spelled with, which is how a symbol
/// naming one — `:<=>`, `:[]=`, `:+@` — is written.
fn ruby_symbol_operator(byte: u8) -> bool {
    matches!(
        byte,
        b'+' | b'-' | b'*' | b'/' | b'%' | b'<' | b'>' | b'=' | b'!' | b'~' | b'^' | b'&' | b'|'
    ) || matches!(byte, b'[' | b']' | b'@')
}

/// Past the symbol a `:` at `index` opens.
///
/// A symbol is a name — with the `@`, `@@` or `$` of a variable in front of it
/// where one is meant — or one of the operator methods, which is read here as
/// the run of characters those are spelled with rather than as a table of
/// them: a run that names no method is a syntax error either way, and reading
/// it as one symbol keeps the byte after it out of the literal path.
fn ruby_symbol_end(bytes: &[u8], index: usize) -> usize {
    let mut cursor = index + 1;
    match bytes.get(cursor) {
        Some(b'$') => return ruby_global_end(bytes, cursor),
        Some(b'@') => return ruby_at_variable_end(bytes, cursor),
        Some(byte) if ruby_identifier_start(*byte) => return ruby_word_end(bytes, cursor),
        _ => {}
    }
    while bytes
        .get(cursor)
        .is_some_and(|byte| ruby_symbol_operator(*byte))
    {
        cursor += 1;
    }
    cursor
}

/// Past the global variable a `$` at `index` opens, or past the `$` alone.
///
/// Ruby's `parse_gvar`: a name, a digit run, `-` and one character, or one of
/// the punctuation names. `$"` and `$'` are two of those names, which is what
/// keeps the quote in either from opening a string, and `$/` and `$\` two more.
/// `#` is not one of them — the reference refuses that spelling outright — so
/// `$#` is a `$` on its own, and the byte behind it opens the comment it opens
/// everywhere else in the language.
fn ruby_global_end(bytes: &[u8], index: usize) -> usize {
    let Some(byte) = bytes.get(index + 1).copied() else {
        return index + 1;
    };
    if ruby_identifier_start(byte) {
        return ruby_identifier_end(bytes, index + 2);
    }
    if byte.is_ascii_digit() {
        let mut end = index + 2;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        return end;
    }
    if byte == b'-' {
        return (index + 3).min(bytes.len());
    }
    if matches!(
        byte,
        b'~' | b'*' | b'$' | b'?' | b'!' | b'@' | b'/' | b'\\' | b';' | b',' | b'.' | b'='
    ) || matches!(byte, b':' | b'<' | b'>' | b'"' | b'&' | b'`' | b'\'' | b'+')
    {
        return index + 2;
    }
    index + 1
}

/// Past the instance or class variable a `@` at `index` opens, or past the `@`
/// alone where no name follows it.
fn ruby_at_variable_end(bytes: &[u8], index: usize) -> usize {
    let mut cursor = index + 1;
    if bytes.get(cursor) == Some(&b'@') {
        cursor += 1;
    }
    ruby_identifier_end(bytes, cursor)
}

/// The end of the character literal a `?` at `question` opens, or `None` when
/// those bytes are the ternary operator.
///
/// Ruby's `parse_qmark`: white space behind the `?` makes it the operator; a
/// character outside ASCII is a literal whole; an ASCII letter, digit or `_`
/// with another name byte behind it is the operator again, which is what keeps
/// `a ?bc : d` a ternary; and everything else — an escape, or one punctuation
/// byte — is a literal.
fn ruby_character_literal_end(bytes: &[u8], question: usize) -> Option<usize> {
    let index = question + 1;
    let byte = *bytes.get(index)?;
    if ruby_is_space(byte) || matches!(byte, b'\r' | b'\n') {
        return None;
    }
    if !byte.is_ascii() {
        return Some((index + ruby_character_width(byte)).min(bytes.len()));
    }
    if byte == b'\\' {
        /* NOTE: `\u{...}` is the one escape whose length the bytes after it
         * decide; every other one ends within a byte or two of name bytes,
         * which are read as the name they look like and cannot open anything. */
        if bytes.get(index + 1) == Some(&b'u') && bytes.get(index + 2) == Some(&b'{') {
            let mut cursor = index + 3;
            while bytes.get(cursor).is_some_and(|byte| *byte != b'}') {
                cursor += 1;
            }
            return Some((cursor + 1).min(bytes.len()));
        }
        return Some((index + 2).min(bytes.len()));
    }
    if ruby_identifier_continue(byte)
        && bytes
            .get(index + 1)
            .is_some_and(|next| ruby_identifier_continue(*next))
    {
        return None;
    }
    Some(index + 1)
}

/// How many bytes the UTF-8 sequence headed by `byte` takes, or one where it
/// heads none. A trailing byte read on its own is a name byte here, which opens
/// nothing, so a miscount costs nothing but a token boundary.
fn ruby_character_width(byte: u8) -> usize {
    match byte {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    }
}

/// Past the option letters that may follow a regular expression — `i`, `m`,
/// `x`, `o`, `n`, `e`, `s`, `u`.
///
/// They are read as a run of ASCII letters rather than as that set: a letter
/// that is not an option is a syntax error either way, and taking it here
/// leaves the lexer where the letters ended rather than in the middle of them.
fn ruby_regexp_flags_end(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).is_some_and(u8::is_ascii_alphabetic) {
        index += 1;
    }
    index
}

/// The header of the `%` literal at `start`, or `None` when those bytes head
/// none and the `%` is the modulo operator.
///
/// Ruby's `parse_percent`: the byte after the `%` is the delimiter unless it is
/// alphanumeric, in which case it names the form and the byte after *that* is
/// the delimiter. A delimiter is any ASCII byte that is not alphanumeric, the
/// space of `% a ` included. `(`, `[`, `{` and `<` pair with their closer and
/// nest; every other delimiter closes with itself.
fn ruby_percent_header(bytes: &[u8], start: usize) -> Option<RubyPercent> {
    let first = *bytes.get(start + 1)?;
    let (form, delimiter, content) = if first.is_ascii_alphanumeric() {
        if !matches!(
            first,
            b'q' | b'Q' | b'w' | b'W' | b'i' | b'I' | b's' | b'r' | b'x'
        ) {
            return None;
        }
        (first, *bytes.get(start + 2)?, start + 3)
    } else {
        (b'Q', first, start + 2)
    };
    if delimiter.is_ascii_alphanumeric() || !delimiter.is_ascii() {
        return None;
    }
    let close = match delimiter {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        b'<' => b'>',
        _ => delimiter,
    };
    Some(RubyPercent {
        form,
        open: delimiter,
        close,
        content,
        interpolates: matches!(form, b'Q' | b'W' | b'I' | b'r' | b'x'),
    })
}

/// The here document a `<<` at `index` opens, and where its header ends, or
/// `None` when those two bytes open none.
///
/// Ruby's `heredoc_identifier`: an optional `-` or `~`, then a quoted
/// terminator or a bare word. The bare word is a run of `is_identchar` bytes
/// from its very first one, which is a wider set than a name may start with: a
/// digit is an identchar, so `<<2` is a here document terminated by a line
/// reading `2` and `<<9x` one terminated by `9x`. Refusing digits would read
/// the body as code, which is the one direction that invents a comment out of
/// bytes Ruby has inside a string, so they are taken. Whether the `<<` stands
/// where one may open at all is [`ruby_heredoc_may_open`]'s question, and it is
/// what still leaves `a[0] <<2` and `p 1 <<2` the shift they are. A quoted
/// terminator that runs past the end of its line opens nothing.
fn ruby_heredoc_header(bytes: &[u8], index: usize) -> Option<(RubyHeredoc, usize)> {
    let mut cursor = index + 2;
    let indented = matches!(bytes.get(cursor), Some(b'-' | b'~'));
    if indented {
        cursor += 1;
    }
    let quote = match bytes.get(cursor)? {
        b'\'' => Some(b'\''),
        b'"' => Some(b'"'),
        b'`' => Some(b'`'),
        byte if ruby_identifier_continue(*byte) => None,
        _ => return None,
    };
    let (label, end) = match quote {
        Some(quote) => {
            let start = cursor + 1;
            let mut end = start;
            loop {
                match bytes.get(end) {
                    Some(byte) if *byte == quote => break,
                    None | Some(b'\r' | b'\n') => return None,
                    Some(_) => end += 1,
                }
            }
            (bytes[start..end].to_vec(), end + 1)
        }
        None => {
            let end = ruby_identifier_end(bytes, cursor + 1);
            (bytes[cursor..end].to_vec(), end)
        }
    };
    Some((
        RubyHeredoc {
            operator: index,
            label,
            indented,
            interpolates: quote != Some(b'\''),
        },
        end,
    ))
}

/// Whether the line beginning at `index` is `heredoc`'s terminator.
///
/// Ruby's `whole_match_p`: the terminator is the whole line, with leading white
/// space skipped only for the `<<-` and `<<~` forms.
fn ruby_heredoc_terminates(bytes: &[u8], index: usize, heredoc: &RubyHeredoc) -> bool {
    let mut probe = index;
    if heredoc.indented {
        while bytes.get(probe).is_some_and(|byte| ruby_is_space(*byte)) {
            probe += 1;
        }
    }
    starts(bytes, probe, &heredoc.label)
        && matches!(
            bytes.get(probe + heredoc.label.len()),
            None | Some(b'\r' | b'\n')
        )
}

fn starts_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle))
}
fn find_ascii_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}
fn tag_boundary(byte: Option<u8>) -> bool {
    byte.is_none_or(|byte| byte.is_ascii_whitespace() || matches!(byte, b'>' | b'/'))
}

fn contains_line_splice(bytes: &[u8]) -> bool {
    let mut cursor = 0;
    while let Some(relative) = memchr(b'\\', &bytes[cursor..]) {
        let index = cursor + relative;
        if bytes.get(index + 1) == Some(&b'\n')
            || (bytes.get(index + 1) == Some(&b'\r') && bytes.get(index + 2) == Some(&b'\n'))
        {
            return true;
        }
        cursor = index + 1;
    }
    false
}

fn next_c_family_trigger(bytes: &[u8], start: usize, language: Language) -> Option<usize> {
    let remaining = bytes.get(start..)?;
    let primary = match language {
        Language::Go => remaining
            .iter()
            .position(|byte| matches!(byte, b'/' | b'"' | b'\'' | b'`')),
        _ => memchr3(b'/', b'"', b'\'', remaining),
    }?;
    Some(start + primary)
}

struct MappedBytes {
    bytes: Vec<u8>,
    origins: Vec<ByteSpan>,
    original_len: usize,
}

impl MappedBytes {
    fn without_c_line_splices(source: &[u8]) -> Self {
        let mut bytes = Vec::with_capacity(source.len());
        let mut origins = Vec::with_capacity(source.len());
        let mut index = 0;
        while index < source.len() {
            if starts(source, index, b"\\\r\n") {
                index += 3;
                continue;
            }
            if starts(source, index, b"\\\n") {
                index += 2;
                continue;
            }
            bytes.push(source[index]);
            origins.push(ByteSpan::new(index, index + 1));
            index += 1;
        }
        Self {
            bytes,
            origins,
            original_len: source.len(),
        }
    }

    fn java_unicode(source: &[u8]) -> (Self, Vec<ByteSpan>) {
        let mut bytes = Vec::with_capacity(source.len());
        let mut origins = Vec::with_capacity(source.len());
        let mut invalid = Vec::new();
        let mut index = 0;
        let mut slash_run = 0usize;
        let mut last_was_escape = false;
        while index < source.len() {
            let eligible = source[index] == b'\\' && (last_was_escape || slash_run & 1 == 0);
            if eligible {
                let mut cursor = index + 1;
                while source.get(cursor) == Some(&b'u') {
                    cursor += 1;
                }
                if cursor > index + 1 {
                    if cursor + 4 <= source.len()
                        && let Some(value) = hex4(&source[cursor..cursor + 4])
                    {
                        if value <= 0x7f {
                            bytes.push(value as u8);
                            origins.push(ByteSpan::new(index, cursor + 4));
                            if value as u8 == b'\\' {
                                slash_run += 1;
                                last_was_escape = true;
                            } else {
                                slash_run = 0;
                                last_was_escape = false;
                            }
                            index = cursor + 4;
                            continue;
                        }
                        if let Some(character) = char::from_u32(value as u32) {
                            let mut encoded = [0; 4];
                            for byte in character.encode_utf8(&mut encoded).as_bytes() {
                                bytes.push(*byte);
                                origins.push(ByteSpan::new(index, cursor + 4));
                            }
                        } else {
                            /* NOTE: Java Unicode escapes are UTF-16 code units, so a
                             * lone surrogate is lexically valid even though it
                             * has no standalone UTF-8 representation. */
                            bytes.push(0x80);
                            origins.push(ByteSpan::new(index, cursor + 4));
                        }
                        slash_run = 0;
                        last_was_escape = false;
                        index = cursor + 4;
                        continue;
                    }
                    invalid.push(ByteSpan::new(index, (cursor + 4).min(source.len())));
                }
            }
            bytes.push(source[index]);
            origins.push(ByteSpan::new(index, index + 1));
            if source[index] == b'\\' {
                slash_run += 1;
            } else {
                slash_run = 0;
            }
            last_was_escape = false;
            index += 1;
        }
        (
            Self {
                bytes,
                origins,
                original_len: source.len(),
            },
            invalid,
        )
    }

    fn original_span(&self, span: ByteSpan) -> ByteSpan {
        if span.is_empty() {
            let point = self
                .origins
                .get(span.start)
                .map_or(self.original_len, |origin| origin.start);
            return ByteSpan::new(point, point);
        }
        let start = self
            .origins
            .get(span.start)
            .map_or(self.original_len, |origin| origin.start);
        let end = if span.end == self.bytes.len() {
            self.original_len
        } else {
            self.origins
                .get(span.end.saturating_sub(1))
                .map_or(self.original_len, |origin| origin.end)
        };
        ByteSpan::new(start, end)
    }
}

fn hex4(bytes: &[u8]) -> Option<u16> {
    let mut value = 0u16;
    for byte in bytes {
        value = value.checked_mul(16)?
            + match byte {
                b'0'..=b'9' => (byte - b'0') as u16,
                b'a'..=b'f' => (byte - b'a' + 10) as u16,
                b'A'..=b'F' => (byte - b'A' + 10) as u16,
                _ => return None,
            };
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comments(source: &[u8], language: Language) -> ScanReport {
        scan(source, language, ScanOptions::default())
    }

    #[test]
    fn rust_nested_and_raw() {
        let report = comments(
            br##"r#"// nope"# /* one /* two */ end */ // yes"##,
            Language::Rust,
        );
        assert!(report.valid);
        assert_eq!(report.comments.len(), 2);
    }

    #[test]
    fn javascript_regex_and_template_expression() {
        let report = comments(
            br#"const x = /\/\/*not/; `text // no ${1 /* yes */}`; // yes"#,
            Language::JavaScript,
        );
        assert_eq!(report.comments.len(), 2);
    }

    #[test]
    fn java_unicode_delimiter() {
        let report = comments(br"int x; \u002f\u002f hi\nint y;", Language::Java);
        assert_eq!(report.comments.len(), 1);
        assert_eq!(
            &br"int x; \u002f\u002f hi\nint y;"
                [report.comments[0].span.start..report.comments[0].span.end],
            br"\u002f\u002f hi\nint y;"
        );
    }

    /// [`parse_heredoc`] is a lookahead with no line bound — a quoted
    /// delimiter word may carry a line terminator as content, so `<<"EO`, a
    /// break, `F"` names the delimiter `EO\nF` — and every path that gives up
    /// rewinds the scan to the byte after the operator and lexes those bytes
    /// again from a state this parse already decided out of them. The reach it
    /// reports is what withdraws the checkpoints in between, so it is asserted
    /// here against the parse itself rather than only through a document whose
    /// checkpoints might move for some other reason.
    #[test]
    fn a_heredoc_delimiter_parse_reports_every_byte_it_consulted() {
        let quoted = b"cat <<\"EO\nF\"\nx\n";
        assert_eq!(quoted[9], b'\n');
        assert_eq!(quoted[11], b'"');
        let mut reach = Reach::default();
        let (heredoc, end) =
            parse_heredoc(quoted, 4, &mut reach).expect("a quoted delimiter word spanning a line");
        assert_eq!(heredoc.delimiter, b"EO\nF");
        assert_eq!(end, 12);
        /* NOTE: one past the line terminator that ended the word, and so past
         * the closing quote on line 2 the parse had to cross to reach it. A
         * checkpoint at the line start at 10 would sit inside that reading. */
        assert_eq!(reach, Reach(13));
        assert!(
            reach.0 > 11,
            "{reach:?} does not cover the closing quote at 11"
        );

        /* NOTE: A plain delimiter word ends at its line, and the parse says so:
         * the terminator that ended the word is the last byte it consulted, and
         * the line start behind it is left standing. */
        let plain = b"cat <<EOF\nx\n";
        assert_eq!(plain[9], b'\n');
        let mut reach = Reach::default();
        let (heredoc, end) = parse_heredoc(plain, 4, &mut reach).expect("a plain delimiter word");
        assert_eq!(heredoc.delimiter, b"EOF");
        assert_eq!(end, 9);
        assert_eq!(reach, Reach(10));
        assert!(
            reach.0 <= 10,
            "{reach:?} reaches past the line the delimiter word ends on"
        );
    }

    /// A tag search a character class bounds reads that class and one byte
    /// more, never the whole document.
    ///
    /// INVARIANT: the reach a lookahead reports withdraws every checkpoint at
    /// or under it, so a search that runs to the end of the file on an
    /// ordinary byte costs the rest of the document its restart points. An
    /// OCaml quoted-string tag is `[a-z_]*` and is followed by `|` (OCaml
    /// manual, Lexical conventions); a PostgreSQL dollar-quote tag is an
    /// identifier or nothing and is followed by `$` (PostgreSQL 4.1.2.4); a C++
    /// raw-string delimiter is at most 16 d-chars and is followed by `(`
    /// ([lex.string]). Each search therefore gives up at the first byte outside
    /// its class, and an ordinary `{`, `$` or `R"` in the code leaves the lines
    /// under it their checkpoints.
    #[test]
    fn a_class_bounded_tag_search_keeps_the_checkpoints_under_it() {
        // NOTE: `{aa` opens no quoted string: the tag class stops at the line
        // NOTE: terminator, which is not the `|` a tag needs behind it. The
        // NOTE: byte the scan stands on is not recorded, so asking this of
        // NOTE: every byte of the document does not drag the watermark along
        // NOTE: behind the scan: the `{` on line 1 is the only thing that reads
        // NOTE: ahead here, and the lines under it keep the watermark it left.
        let ocaml = b"let x = {aa\n(* c *)\ny\n";
        assert_eq!(
            scan_checkpoint_watermarks(ocaml, Language::Ocaml, ScanOptions::default()),
            [(0, 0), (12, 12), (20, 12), (22, 12)]
        );

        // NOTE: `R"x y"` opens no raw string: the d-char class stops at the
        // NOTE: space, which is not the `(` a delimiter needs behind it. The
        // NOTE: search that used to answer that read the whole document looking
        // NOTE: for a `(`, which took every checkpoint below it away.
        let cpp = b"char* s = R\"x y\";\n// c\nz\n";
        assert_eq!(cpp[13], b' ');
        assert_eq!(
            scan_checkpoint_watermarks(cpp, Language::Cpp, ScanOptions::default()),
            [(0, 0), (18, 14), (23, 14), (25, 14)]
        );

        // NOTE: `a$b` opens no dollar-quoted string: the tag class stops at the
        // NOTE: space, which is not the `$` a tag needs behind it.
        let postgres = b"select a$b from t\n-- c\nx\n";
        assert_eq!(
            scan_checkpoint_watermarks(
                postgres,
                Language::Sql,
                ScanOptions {
                    dialect: Dialect::PostgreSql,
                    ..Default::default()
                }
            ),
            [(0, 0), (18, 11), (23, 11), (25, 11)]
        );

        // NOTE: An Oracle q-quote reads one delimiter byte and no more, so a
        // NOTE: bare `q` in the code costs nothing either.
        let oracle = b"select q from t\n-- c\nx\n";
        assert_eq!(
            scan_checkpoint_watermarks(
                oracle,
                Language::Sql,
                ScanOptions {
                    dialect: Dialect::Oracle,
                    ..Default::default()
                }
            ),
            [(0, 0), (16, 9), (21, 9), (23, 9)]
        );
    }

    /// What Swift's three lookaheads cost the lines under them.
    ///
    /// INVARIANT: two of them are bounded and one is not, and the difference is
    /// whether the scan rewinds behind what they read. A `#` run stops at the
    /// first byte outside its class, and a bare `/ ... /` may hold no line
    /// terminator (The Swift Programming Language, Lexical Structure), so
    /// neither can reach past the line it stands on and the lines under it keep
    /// their checkpoints. An extended `#/ ... /#` that opens a multi-line
    /// literal and never closes is the one that cannot be bounded: it reads to
    /// the end of the document, hands the lines back, and lexes them again as
    /// code — so every checkpoint under it is withdrawn, the end of the
    /// document included, because appending a `/#` would close it.
    #[test]
    fn swift_lookaheads_pay_for_what_they_read() {
        // NOTE: `#if` opens no literal: the run of `#` stops at the `i`, which
        // NOTE: is neither the quote of a raw string nor the slash of a regular
        // NOTE: expression literal.
        let directive = b"let a = #if x\n// c\ny\n";
        assert_eq!(
            scan_checkpoint_watermarks(directive, Language::Swift, ScanOptions::default()),
            [(0, 0), (14, 10), (19, 10), (21, 10)]
        );

        // NOTE: `1 / 2` opens no literal either, and gives up at the space one
        // NOTE: byte behind the slash rather than reading the line at all.
        let division = b"let a = 1 / 2\n// c\nx\n";
        assert_eq!(
            scan_checkpoint_watermarks(division, Language::Swift, ScanOptions::default()),
            [(0, 0), (14, 12), (19, 12), (21, 12)]
        );

        // NOTE: `(/x y` opens none: the closing delimiter would be preceded by
        // NOTE: an unescaped space, and there is none on the line in any case.
        // NOTE: The search reads the terminator to know it must stop there,
        // NOTE: which leaves the line start behind it standing.
        let candidate = b"let a = (/x y\n// c\nz\n";
        assert_eq!(candidate[13], b'\n');
        assert_eq!(
            scan_checkpoint_watermarks(candidate, Language::Swift, ScanOptions::default()),
            [(0, 0), (14, 14), (19, 14), (21, 14)]
        );

        // NOTE: the unbounded one, and the only checkpoint left is the offset a
        // NOTE: restart there *is* the full scan from.
        let unterminated = b"let a = #/\n// c\nz\n";
        assert_eq!(
            scan_checkpoint_watermarks(unterminated, Language::Swift, ScanOptions::default()),
            [(0, 0)]
        );
    }

    /// Every C# lookahead is bounded by the run it is reading, so the line
    /// starts under one keep their restart points.
    #[test]
    fn csharp_lookaheads_pay_for_what_they_read() {
        // NOTE: `@x` opens no literal: the run of `$` and `@` stops at the `x`,
        // NOTE: which is not the quote every string form needs. The byte that
        // NOTE: ended the run is the one byte read past where the scan resumes.
        let identifier = b"var a = @x;\n// c\ny\n";
        assert_eq!(identifier[9], b'x');
        assert_eq!(
            scan_checkpoint_watermarks(identifier, Language::CSharp, ScanOptions::default()),
            [(0, 0), (12, 10), (17, 10), (19, 10)]
        );

        // NOTE: a raw string's closing run is taken whole, so the byte that
        // NOTE: ended it is read one past the resume point — and the read that
        // NOTE: decided the literal spans lines costs nothing on top, because
        // NOTE: the scan takes every byte of that line either way.
        let raw = b"var a = \"\"\"\n  x\n  \"\"\";\n// c\ny\n";
        assert_eq!(raw[21], b';');
        assert_eq!(
            scan_checkpoint_watermarks(raw, Language::CSharp, ScanOptions::default()),
            [(0, 0), (23, 22), (28, 22), (30, 22)]
        );

        // NOTE: a directive line is a plain forward scan of one line, so it
        // NOTE: reports nothing and the line under it keeps its restart point.
        let directive = b"#if x // c\nvar a = 1;\n";
        assert_eq!(
            scan_checkpoint_watermarks(directive, Language::CSharp, ScanOptions::default()),
            [(0, 0), (11, 0), (22, 0)]
        );

        // NOTE: a verbatim string that never closes swallows the file, and the
        // NOTE: only checkpoint left is the offset a restart there *is* the full
        // NOTE: scan from.
        let unterminated = b"var a = @\"open\n// c\nz\n";
        assert_eq!(
            scan_checkpoint_watermarks(unterminated, Language::CSharp, ScanOptions::default()),
            [(0, 0)]
        );
    }

    #[test]
    fn shell_heredoc_is_opaque() {
        let report = comments(b"cat <<EOF\n# data\nEOF\n# comment\n", Language::Shell);
        assert_eq!(report.comments.len(), 1);
    }

    #[test]
    fn regex_overrides_apply_to_complete_comment_bytes() {
        let source = b"// KEEP this\n// REMOVE this\n// ordinary\n";
        let report = scan(
            source,
            Language::C,
            ScanOptions {
                policy: Policy::Legal,
                keep_regex: vec!["KEEP".into()],
                remove_regex: vec!["REMOVE".into()],
                ..Default::default()
            },
        );
        assert!(matches!(
            report.comments[0].disposition,
            Disposition::Keep { .. }
        ));
        assert!(report.comments[1].disposition.is_remove());
        assert!(report.comments[2].disposition.is_remove());
    }

    #[test]
    fn html_embeds_javascript_but_protects_html() {
        let report = comments(
            b"<!--keep--><script>let x=1;//remove\n</script>",
            Language::Html,
        );
        assert_eq!(report.comments.len(), 2);
        assert!(!report.comments[0].disposition.is_remove());
        assert!(report.comments[1].disposition.is_remove());
    }

    /// A Scala checkpoint may not stand directly before a `<` that could open
    /// an XML literal: the literal's trigger reads the byte *behind* its `<`,
    /// and a rescan that begins there has no byte behind it to read. A line
    /// that opens with one therefore offers no restart point, while a line
    /// that opens with a `<` no literal could begin at keeps its checkpoint.
    #[test]
    fn a_scala_xml_boundary_keeps_its_checkpoint_away_from_the_literal() {
        // NOTE: `<a>` after a newline is an XML literal: the checkpoint after
        // NOTE: the newline is refused, and the text of the literal is opaque
        // NOTE: in a full scan and in a rescan from any earlier checkpoint.
        let xml = b"x\n<a>// text</a>\ny\n";
        assert_eq!(
            scan_checkpoint_watermarks(xml, Language::Scala, ScanOptions::default()),
            [(0, 0), (17, 0), (19, 0)]
        );
        let (_, checkpoints) =
            scan_with_checkpoints(xml, Language::Scala, ScanOptions::default(), 0);
        assert_eq!(checkpoints, vec![0, 17, 19]);
        for point in &checkpoints {
            let (suffix, _) = scan_with_checkpoints(
                &xml[*point..],
                Language::Scala,
                ScanOptions::default(),
                *point,
            );
            assert!(suffix.comments.is_empty(), "restarting at {point}");
        }

        // NOTE: `<1` cannot begin a literal (a digit is no XML name start), so
        // NOTE: the line keeps its checkpoint and the `//` under it is a
        // NOTE: comment in a full scan and in a rescan from it alike.
        let operator = b"x\n<1 + 2\n// c\ny\n";
        assert_eq!(
            scan_checkpoint_watermarks(operator, Language::Scala, ScanOptions::default()),
            [(0, 0), (2, 0), (9, 0), (14, 0), (16, 0)]
        );
        let (_, checkpoints) =
            scan_with_checkpoints(operator, Language::Scala, ScanOptions::default(), 0);
        assert_eq!(checkpoints, vec![0, 2, 9, 14, 16]);
        for point in &checkpoints {
            let (suffix, _) = scan_with_checkpoints(
                &operator[*point..],
                Language::Scala,
                ScanOptions::default(),
                *point,
            );
            assert!(
                suffix
                    .comments
                    .iter()
                    .all(|comment| comment.span.start >= *point),
                "restarting at {point}"
            );
        }

        // NOTE: `<` after a bare `\r` is no literal (a `\r` is not a trigger
        // NOTE: byte), so the checkpoint the `\r` earns is refused all the
        // NOTE: same: a rescan from it would not know the `\r` was there.
        let cr = b"\r<?php //go:build\nz\n";
        assert_eq!(
            scan_checkpoint_watermarks(cr, Language::Scala, ScanOptions::default()),
            [(0, 0), (18, 0), (20, 0)]
        );
    }
}
