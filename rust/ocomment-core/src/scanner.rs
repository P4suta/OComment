use crate::{
    ByteSpan, Comment, CommentKind, Diagnostic, Dialect, Disposition, DispositionExplanation,
    Language, Policy, ScanOptions, ScanReport, Severity,
};
use memchr::{memchr, memchr2, memchr3, memmem};
use regex::bytes::RegexSet;

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
    match language {
        Language::Rust
        | Language::C
        | Language::Cpp
        | Language::Go
        | Language::Kotlin
        | Language::Css
        | Language::Jsonc => scanner.scan_c_family(),
        Language::Java => scanner.scan_java(),
        Language::JavaScript | Language::TypeScript => scanner.scan_javascript(),
        Language::Ocaml => scanner.scan_ocaml(),
        Language::Python => scanner.scan_python(),
        Language::Shell => scanner.scan_shell(),
        Language::Html => scanner.scan_html(),
        Language::Sql => scanner.scan_sql(),
        Language::Unknown => scanner.error(
            "unknown-language",
            "a language is required",
            ByteSpan::new(0, 0),
        ),
    }
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
    restart_rules: RestartRules,
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
            restart_rules: RestartRules::of(source, language),
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
            restart_rules: RestartRules::of(source, language),
        }
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
    fn checkpoint_is_restartable(&self, local: usize) -> bool {
        self.offset > 0 || self.restart_rules.permit_restart_at(self.source, local)
    }

    fn add_safe_checkpoint(&mut self, local: usize) {
        if !self.track_checkpoints || !self.checkpoint_is_restartable(local) {
            return;
        }
        let absolute = self.offset + local;
        if self.safe_checkpoints.last().copied() != Some(absolute) {
            self.safe_checkpoints.push(absolute);
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
                if bytes[index] == b'\'' && rust_char_start(bytes, index) {
                    return Some(self.quoted_or_error(index, false, "character literal"));
                }
            }
            Language::C | Language::Cpp => {
                if self.language == Language::Cpp
                    && bytes[index] == b'"'
                    && let Some(raw_start) = cpp_raw_start_at_quote(bytes, index)
                    && let Some((end, closed)) = cpp_raw_string(bytes, raw_start)
                {
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
                if bytes[index] == b'"' {
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
                child.add_comment(index, end, line_kind(child.source, index));
                index = end;
                continue;
            }
            if starts(child.source, index, b"/*") {
                let (end, closed) = block_end(child.source, index, b"/*", b"*/", false);
                child.add_comment(index, end, block_kind(child.source, index));
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
                let (end, closed) = ocaml_comment_end(bytes, index);
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
            if let Some((end, closed)) = ocaml_quoted_string(bytes, index) {
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
            if bytes[index] == b'\'' && ocaml_char_start(bytes, index) {
                index = self.quoted_or_error(index, false, "OCaml character literal");
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
                } else if triple {
                    index = self.scan_python_delimited(index, quote_start, true);
                } else {
                    index = self.quoted_or_error(quote_start, false, "Python string");
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
                    self.quoted_or_error(quote_start, false, "Python string")
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
                    } else if let Some((heredoc, end)) = parse_heredoc(bytes, index) {
                        heredocs.push(heredoc);
                        index = end;
                        word_open = true;
                    } else {
                        index += 1;
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
            if bytes[index] == b'$'
                && self.options.dialect == Dialect::PostgreSql
                && let Some((end, closed)) = sql_dollar_quote_end(bytes, index)
            {
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
            if (bytes[index] == b'q' || bytes[index] == b'Q')
                && self.options.dialect == Dialect::Oracle
                && let Some((end, closed)) = oracle_q_quote_end(bytes, index)
            {
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
                byte if byte.is_ascii_whitespace() => index += 1,
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
                        while previous > index && bytes[previous - 1].is_ascii_whitespace() {
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
    if offset == 0 && start == 0 && raw.starts_with(b"#!") {
        return CommentKind::Shebang;
    }
    if offset == 0
        && language == Language::Python
        && is_python_encoding_declaration(source, start, raw)
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

/// A checkpoint sits immediately after a line terminator, and a CRLF pair is a
/// single terminator. An edit that supplies the LF after an existing CR moves
/// the boundary one byte on, leaving the offset a previous revision recorded
/// inside the pair, where no scan of these bytes would ever resume.
fn the_line_ending_permits_a_restart(source: &[u8], offset: usize) -> bool {
    offset == 0 || source.get(offset - 1) != Some(&b'\r') || source.get(offset) != Some(&b'\n')
}

/// Preamble classification depends on the absolute offset, and Python only
/// recognises an encoding declaration while scanning from offset 0, which makes
/// the start of line 2 a restart point exactly when no encoding declaration
/// follows. Offset 0 always passes — restarting a scan there *is* the full
/// scan.
fn the_preamble_permits_a_restart(source: &[u8], language: Language, offset: usize) -> bool {
    offset == 0
        || language != Language::Python
        || !is_within_first_two_lines(source, offset)
        || !python_line_declares_encoding(source, offset)
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
}

impl RestartRules {
    pub(crate) fn of(source: &[u8], language: Language) -> Self {
        Self {
            language,
            splicing_permits_restarts: line_splicing_permits_restarts(source, language),
        }
    }

    /// Whether restarting a scan of `source` — the bytes these rules were built
    /// from — at `offset` reproduces the rest of a full scan of it.
    pub(crate) fn permit_restart_at(&self, source: &[u8], offset: usize) -> bool {
        self.splicing_permits_restarts
            && the_line_ending_permits_a_restart(source, offset)
            && the_preamble_permits_a_restart(source, self.language, offset)
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

/// Whether the line beginning at `line_start` carries a Python encoding
/// declaration, and therefore a comment whose classification depends on the
/// scan starting at offset 0.
fn python_line_declares_encoding(source: &[u8], line_start: usize) -> bool {
    let mut index = line_start;
    while matches!(source.get(index), Some(b' ' | b'\t' | 0x0c)) {
        index += 1;
    }
    if source.get(index) != Some(&b'#') {
        return false;
    }
    let end = line_end(source, index + 1);
    is_python_encoding_declaration(source, index, &source[index..end])
}

fn is_python_encoding_declaration(source: &[u8], start: usize, raw: &[u8]) -> bool {
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
fn opens_with_keyword(text: &str, keyword: &str) -> bool {
    text.strip_prefix(keyword)
        .is_some_and(|rest| rest.starts_with(|character: char| character.is_ascii_whitespace()))
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

fn starts(bytes: &[u8], index: usize, needle: &[u8]) -> bool {
    bytes.get(index..index.saturating_add(needle.len())) == Some(needle)
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

fn rust_char_start(bytes: &[u8], index: usize) -> bool {
    let Some(next) = bytes.get(index + 1) else {
        return false;
    };
    if *next == b'\\' {
        return bytes.get(index + 3..index + 4) == Some(b"'");
    }
    bytes.get(index + 2) == Some(&b'\'')
        || (*next & 0x80 != 0 && bytes[index + 1..].iter().take(5).any(|byte| *byte == b'\''))
}

fn is_c_quote_start(bytes: &[u8], index: usize) -> bool {
    matches!(bytes[index], b'"' | b'\'')
        || (matches!(bytes[index], b'L' | b'u' | b'U')
            && matches!(bytes.get(index + 1), Some(b'"' | b'\'')))
        || (starts(bytes, index, b"u8\"") || starts(bytes, index, b"u8'"))
}

fn cpp_raw_string(bytes: &[u8], index: usize) -> Option<(usize, bool)> {
    let prefixes: [&[u8]; 5] = [b"R\"", b"u8R\"", b"uR\"", b"UR\"", b"LR\""];
    let prefix = prefixes
        .iter()
        .find(|prefix| starts(bytes, index, prefix))?;
    let delimiter_start = index + prefix.len();
    let open = bytes[delimiter_start..]
        .iter()
        .position(|byte| *byte == b'(')?
        + delimiter_start;
    if open - delimiter_start > 16
        || bytes[delimiter_start..open]
            .iter()
            .any(|byte| byte.is_ascii_whitespace() || matches!(byte, b'\\' | b')'))
    {
        return None;
    }
    let mut close = Vec::with_capacity(open - delimiter_start + 2);
    close.push(b')');
    close.extend_from_slice(&bytes[delimiter_start..open]);
    close.push(b'"');
    Some(match find_subslice(&bytes[open + 1..], &close) {
        Some(relative) => (open + 1 + relative + close.len(), true),
        None => (bytes.len(), false),
    })
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

fn ocaml_comment_end(bytes: &[u8], start: usize) -> (usize, bool) {
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
        } else if let Some((end, _)) = ocaml_quoted_string(bytes, index) {
            index = end;
        } else if bytes[index] == b'\'' && ocaml_char_start(bytes, index) {
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

fn ocaml_quoted_string(bytes: &[u8], index: usize) -> Option<(usize, bool)> {
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    let pipe = bytes[index + 1..].iter().position(|byte| *byte == b'|')? + index + 1;
    if !bytes[index + 1..pipe]
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || *byte == b'_')
    {
        return None;
    }
    let mut close = Vec::with_capacity(pipe - index + 1);
    close.push(b'|');
    close.extend_from_slice(&bytes[index + 1..pipe]);
    close.push(b'}');
    Some(match find_subslice(&bytes[pipe + 1..], &close) {
        Some(relative) => (pipe + 1 + relative + close.len(), true),
        None => (bytes.len(), false),
    })
}

fn ocaml_char_start(bytes: &[u8], index: usize) -> bool {
    bytes.get(index + 2) == Some(&b'\'')
        || (bytes.get(index + 1) == Some(&b'\\')
            && bytes[index + 2..].iter().take(6).any(|byte| *byte == b'\''))
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

fn parse_heredoc(bytes: &[u8], index: usize) -> Option<(Heredoc, usize)> {
    let strip_tabs = bytes.get(index + 2) == Some(&b'-');
    let mut cursor = index + if strip_tabs { 3 } else { 2 };
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace)
        && !matches!(bytes[cursor], b'\r' | b'\n')
    {
        cursor += 1;
    }
    let mut delimiter = Vec::new();
    let mut quote = None;
    let mut saw_word = false;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
                cursor += 1;
            } else if active == b'"' && byte == b'\\' {
                let escaped = *bytes.get(cursor + 1)?;
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
        if byte.is_ascii_whitespace() || matches!(byte, b';' | b'|' | b'&' | b'(' | b')' | b'<') {
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
                let escaped = *bytes.get(cursor + 1)?;
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

fn sql_dollar_quote_end(bytes: &[u8], start: usize) -> Option<(usize, bool)> {
    let second = bytes[start + 1..].iter().position(|byte| *byte == b'$')? + start + 1;
    let tag = &bytes[start + 1..second];
    if !tag.is_empty()
        && (!matches!(tag[0], b'a'..=b'z' | b'A'..=b'Z' | b'_')
            || !tag[1..]
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_'))
    {
        return None;
    }
    let delimiter = &bytes[start..=second];
    Some(match find_subslice(&bytes[second + 1..], delimiter) {
        Some(relative) => (second + 1 + relative + delimiter.len(), true),
        None => (bytes.len(), false),
    })
}

fn oracle_q_quote_end(bytes: &[u8], start: usize) -> Option<(usize, bool)> {
    if bytes.get(start + 1) != Some(&b'\'') {
        return None;
    }
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
        Some(relative) => (start + 3 + relative + 2, true),
        None => (bytes.len(), false),
    })
}

fn js_html_close_comment(bytes: &[u8], index: usize) -> bool {
    if !starts(bytes, index, b"-->") {
        return false;
    }
    let line_start = bytes[..index]
        .iter()
        .rposition(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(0, |position| position + 1);
    let line_prefix = &bytes[line_start..index];
    let prefix = line_prefix
        .strip_prefix(b"\xef\xbb\xbf")
        .unwrap_or(line_prefix);
    prefix.iter().all(u8::is_ascii_whitespace)
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
        Language::Jsonc => memchr2(b'/', b'"', remaining),
        Language::Go => remaining
            .iter()
            .position(|byte| matches!(byte, b'/' | b'"' | b'\'' | b'`')),
        Language::Css | Language::Kotlin | Language::Rust | Language::C | Language::Cpp => {
            memchr3(b'/', b'"', b'\'', remaining)
        }
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
}
