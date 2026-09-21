use crate::{
    Action, ByteSpan, Comment, CommentKind, Edit, ExternalSpanError, Language, Layout,
    PreparedScanner, ScanReport, SourceMap, TransformOptions, TransformPlan, TransformResult,
    scanner::{
        disposition, keep_yaml_structural_trails, lines_a_removal_must_swallow, scan,
        unicode_line_terminator_width,
    },
};
use std::borrow::Cow;
use unicode_width::UnicodeWidthChar;

/// Scan `source` and produce the bytes a removal would write.
///
/// This is [`scan`] followed by the edits its report calls for. Nothing is
/// written anywhere: the caller gets the new bytes, the edits that made them,
/// the report they were decided from, and a
/// [`SourceMap`](crate::SourceMap) between the two.
///
/// A source the scanner reported invalid — an unterminated comment or string —
/// is returned byte for byte with no edits at all, unless
/// [`ScanOptions::force_invalid`](crate::ScanOptions::force_invalid) is set.
///
/// # Examples
///
/// ```
/// use ocomment_core::{Language, TransformOptions, transform};
///
/// let result = transform(
///     b"let x = 1; // note\n",
///     Language::Rust,
///     TransformOptions::default(),
/// );
/// assert_eq!(result.output, b"let x = 1; \n");
/// assert_eq!(result.report.comments.len(), 1);
/// assert_eq!(result.edits.len(), 1);
///
/// // An unterminated comment leaves the file alone.
/// let broken = transform(b"x /* no end", Language::C, TransformOptions::default());
/// assert!(!broken.report.valid);
/// assert_eq!(broken.output, b"x /* no end");
/// ```
pub fn transform(source: &[u8], language: Language, options: TransformOptions) -> TransformResult {
    transform_plan(source, language, options).finish(source)
}

/// Scan `source` and compute its edits without building output bytes or a
/// source map.
///
/// This is the lazy counterpart of [`transform`]. It is useful for checkers
/// and report writers that only need the report or edit list.
pub fn transform_plan(
    source: &[u8],
    language: Language,
    options: TransformOptions,
) -> TransformPlan {
    let force_invalid = options.scan.force_invalid;
    let report = scan(source, language, options.scan);
    plan_report(source, report, options.layout, force_invalid)
}

impl PreparedScanner {
    /// Scan and plan edits with this scanner's already-compiled policy.
    pub fn transform_plan(
        &self,
        source: &[u8],
        language: Language,
        layout: Layout,
    ) -> TransformPlan {
        let report = self.scan(source, language);
        plan_report(source, report, layout, self.options().force_invalid)
    }

    /// Produce a complete transformation with this scanner's compiled policy.
    pub fn transform(&self, source: &[u8], language: Language, layout: Layout) -> TransformResult {
        self.transform_plan(source, language, layout).finish(source)
    }

    /// Validate externally supplied spans and plan their edits with this
    /// scanner's already-compiled policy.
    pub fn transform_spans_plan(
        &self,
        source: &[u8],
        language: Language,
        spans: &[(ByteSpan, CommentKind)],
        layout: Layout,
    ) -> Result<TransformPlan, ExternalSpanError> {
        let report = self.scan_spans(source, language, spans)?;
        Ok(plan_report(
            source,
            report,
            layout,
            self.options().force_invalid,
        ))
    }

    /// Validate and classify externally supplied spans without planning edits.
    pub fn scan_spans(
        &self,
        source: &[u8],
        language: Language,
        spans: &[(ByteSpan, CommentKind)],
    ) -> Result<ScanReport, ExternalSpanError> {
        external_report(source, language, spans, self)
    }
}

/// Transform a scanner's already-classified comment spans using the same
/// policy, layout, edit validation, and source-map engine as built-in scans.
///
/// This is the safe hand-off point for declarative or WASM scanners. Spans
/// must be non-empty, sorted, non-overlapping, and contained in `source`.
///
/// The report that comes back carries no diagnostics and is always valid: the
/// external scanner, not this crate, judged whether the source lexed.
///
/// # Errors
///
/// Returns [`ExternalSpanError`] naming the first span that reaches past the
/// end of `source`, covers no bytes, or starts before its predecessor ends,
/// or reporting a `keep_regex`/`remove_regex` entry that would not compile.
/// Nothing is transformed when validation fails.
///
/// # Examples
///
/// ```
/// use ocomment_core::{
///     ByteSpan, CommentKind, ExternalSpanError, Language, TransformOptions, transform_spans,
/// };
///
/// let source = b"a/* ordinary */b/* directive */";
/// let result = transform_spans(
///     source,
///     Language::Unknown,
///     &[
///         (ByteSpan::new(1, 15), CommentKind::Block),
///         (ByteSpan::new(16, source.len()), CommentKind::Directive),
///     ],
///     TransformOptions::default(),
/// )
/// .unwrap();
/// // The same policy the built-in scanners get: the directive is kept.
/// assert_eq!(result.output, b"a b/* directive */");
///
/// let bad = transform_spans(
///     source,
///     Language::Unknown,
///     &[(ByteSpan::new(2, source.len() + 1), CommentKind::Block)],
///     TransformOptions::default(),
/// );
/// assert!(matches!(bad, Err(ExternalSpanError::OutOfBounds { .. })));
/// ```
pub fn transform_spans(
    source: &[u8],
    language: Language,
    spans: &[(ByteSpan, CommentKind)],
    options: TransformOptions,
) -> Result<TransformResult, ExternalSpanError> {
    let prepared = PreparedScanner::new(options.scan)
        .map_err(|error| ExternalSpanError::InvalidPattern(error.to_string()))?;
    Ok(prepared
        .transform_spans_plan(source, language, spans, options.layout)?
        .finish(source))
}

fn external_report(
    source: &[u8],
    language: Language,
    spans: &[(ByteSpan, CommentKind)],
    prepared: &PreparedScanner,
) -> Result<ScanReport, ExternalSpanError> {
    let mut cursor = 0;
    let mut comments = Vec::with_capacity(spans.len());
    for (index, (span, kind)) in spans.iter().copied().enumerate() {
        if span.start > span.end || span.end > source.len() {
            return Err(ExternalSpanError::OutOfBounds {
                index,
                source_len: source.len(),
            });
        }
        if span.is_empty() {
            return Err(ExternalSpanError::Empty { index });
        }
        if index > 0 && span.start < cursor {
            return Err(ExternalSpanError::OrderOrOverlap { index });
        }
        cursor = span.end;
        comments.push(Comment::new(
            span,
            kind,
            disposition(
                kind,
                prepared.options(),
                &source[span.start..span.end],
                &prepared.patterns,
            ),
        ));
    }
    /* NOTE: The one verdict a comment's own bytes cannot reach, so it is
     * applied to the hand-off as a built-in scan applies it: a YAML block
     * scalar leaning on the comment that ends it keeps that comment, whoever
     * found it. Without this the report would promise a removal that
     * `lines_a_removal_must_swallow` cannot make safe. */
    keep_yaml_structural_trails(source, language, &mut comments);
    Ok(ScanReport {
        language,
        comments,
        diagnostics: Vec::new(),
        valid: true,
    })
}

pub(crate) fn transform_report(
    source: &[u8],
    report: crate::ScanReport,
    options: TransformOptions,
) -> TransformResult {
    plan_report(source, report, options.layout, options.scan.force_invalid).finish(source)
}

/// Plan the edits a report calls for, without scanning again.
///
/// [`transform_plan`] is this with the scan in front of it. They are separate
/// because a report is not always the one a scan produced untouched: a caller
/// may hold a rule the scanner cannot decide — one that needs a clock, a
/// repository, anything outside the bytes — and a plan built from a fresh scan
/// would quietly ignore it.
pub fn plan_report(
    source: &[u8],
    report: crate::ScanReport,
    layout: Layout,
    force_invalid: bool,
) -> TransformPlan {
    let edits = if report.valid || force_invalid {
        /* NOTE: A forced run is a run over a file the scanner could not finish,
         * so the comments it reported are not all worth the same. It asks for
         * the edits a broken file still supports, not for every edit a broken
         * report happens to name. */
        let considered: Cow<'_, [Comment]> = if report.established_everything() {
            Cow::Borrowed(report.comments.as_slice())
        } else {
            Cow::Owned(
                report
                    .comments
                    .iter()
                    .filter(|comment| report.established(comment.span))
                    .cloned()
                    .collect(),
            )
        };
        /* NOTE: The one hole whose own bytes carry meaning, so every layout has
         * to be told where not to leave one. `compact` takes the line already;
         * what it does not know on its own is how far past the line to go
         * under a `|+` body. */
        let swallow = lines_a_removal_must_swallow(source, report.language, &considered);
        match layout {
            Layout::Lines => line_edits(source, &considered, &swallow),
            Layout::Columns => column_edits(source, &considered, &swallow),
            Layout::Compact => compact_edits(source, &considered, &swallow),
        }
    } else {
        Vec::new()
    };
    TransformPlan { edits, report }
}

impl TransformPlan {
    /// Apply this plan and build its source map.
    ///
    /// # Panics
    ///
    /// Panics when `source` is not the source the plan was made for and an
    /// edit consequently lies outside it, just like [`apply_edits`].
    pub fn finish(self, source: &[u8]) -> TransformResult {
        let output = apply_edits(source, &self.edits);
        let source_map = SourceMap::from_edits(source.len(), &self.edits);
        TransformResult {
            output,
            edits: self.edits,
            report: self.report,
            source_map,
        }
    }

    /// Build only the transformed bytes, leaving the plan available.
    pub fn output(&self, source: &[u8]) -> Vec<u8> {
        apply_edits(source, &self.edits)
    }

    /// Build only the source map, leaving the plan available.
    pub fn source_map(&self, source_len: usize) -> SourceMap {
        SourceMap::from_edits(source_len, &self.edits)
    }
}

/// The edit a rewritten comment plans: its own span, and the bytes the style
/// rules make of it.
///
/// Not a layout question, which is why all three layouts build it the same
/// way. A layout decides what is left *where a comment used to be*, and a
/// rewritten comment has not been anywhere: it is still there, spelled
/// differently.
///
/// [`Layout::Columns`] is the one layout this costs something. Its promise is
/// that every column after an edit keeps its number, and a replacement of a
/// different width cannot keep it. The promise is kept for removals, which is
/// what the layout exists for; a caller that has asked for both is asking for
/// two things that contradict each other, and the CLI refuses the pair rather
/// than picking one silently.
///
/// The bytes are the verdict's own. Nothing is recomputed here and there is
/// nothing to recompute it from: the rules that decided are not in scope, and
/// that is the point — a planner holding the rules is a planner that can plan
/// with different ones than the scan used.
fn rewrite_edit(comment: &Comment) -> Option<Edit> {
    Some(Edit {
        span: comment.span,
        replacement: comment.disposition().replacement()?.to_vec(),
    })
}

/// Apply sorted, non-overlapping half-open edits.
///
/// The bytes outside the edited spans are copied through untouched, which is
/// what makes a transformation byte-preserving: a BOM, CRLF line endings, a
/// missing final newline, and bytes that are not UTF-8 at all all survive.
///
/// # Panics
///
/// Panics if an edit has `start > end`, starts before its predecessor ends, or
/// reaches past the end of `source`. The edits of a
/// [`TransformResult`] always satisfy that contract; edits assembled by hand
/// have to be sorted first.
///
/// # Examples
///
/// ```
/// use ocomment_core::{ByteSpan, Edit, Language, TransformOptions, apply_edits, transform};
///
/// let edits = [Edit {
///     span: ByteSpan::new(3, 8),
///     replacement: b"there".to_vec(),
/// }];
/// assert_eq!(apply_edits(b"hi world", &edits), b"hi there");
///
/// // Re-applying a transformation's own edits reproduces its output.
/// let source = b"let x = 1; // note\n";
/// let result = transform(source, Language::Rust, TransformOptions::default());
/// assert_eq!(apply_edits(source, &result.edits), result.output);
/// ```
pub fn apply_edits(source: &[u8], edits: &[Edit]) -> Vec<u8> {
    let mut cursor = 0;
    let output_len = edits.iter().fold(source.len(), |length, edit| {
        length
            .saturating_sub(edit.span.len())
            .saturating_add(edit.replacement.len())
    });
    let mut output = Vec::with_capacity(output_len);
    for edit in edits {
        assert!(
            edit.span.start <= edit.span.end,
            "edit has an inverted span"
        );
        assert!(edit.span.start >= cursor, "edits overlap or are not sorted");
        assert!(edit.span.end <= source.len(), "edit is outside the source");
        output.extend_from_slice(&source[cursor..edit.span.start]);
        output.extend_from_slice(&edit.replacement);
        cursor = edit.span.end;
    }
    output.extend_from_slice(&source[cursor..]);
    output
}

/// The edits [`Layout::Lines`] makes: one per removed comment, over exactly
/// the bytes that comment covers — save where `swallow` names a whole line,
/// because there the hole itself would say something (see
/// [`lines_a_removal_must_swallow`]). This is the layout that promises line
/// numbers and those lines are the one place it cannot keep that promise.
fn line_edits(source: &[u8], comments: &[Comment], swallow: &[Option<ByteSpan>]) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut floor = 0usize;
    for (index, comment) in comments.iter().enumerate() {
        match comment.disposition().action() {
            Action::Keep => continue,
            Action::Rewrite => {
                edits.extend(rewrite_edit(comment));
                continue;
            }
            Action::Remove => {}
        }
        let edit = match swallow.get(index).copied().flatten() {
            Some(line) => Edit {
                span: ByteSpan::new(line.start.max(floor), line.end),
                replacement: Vec::new(),
            },
            None => Edit {
                span: comment.span,
                replacement: if comment.kind == CommentKind::HtmlComment {
                    Vec::new()
                } else {
                    line_replacement(source, comment.span)
                },
            },
        };
        floor = edit.span.end;
        edits.push(edit);
    }
    edits
}

/// What [`Layout::Lines`] leaves in place of a removed comment: the line
/// terminators the comment spanned, so every following line keeps its number,
/// and a single space when the comment was all that kept two tokens apart. A
/// comment that spanned a terminator needs no space of its own, because a
/// newline is a lexical separator already.
fn line_replacement(source: &[u8], span: ByteSpan) -> Vec<u8> {
    let mut output = newline_sequence(&source[span.start..span.end]);
    if output.is_empty() && has_non_whitespace_neighbors(source, span) {
        output.push(b' ');
    }
    output
}

/// The edits [`Layout::Columns`] makes: one per removed comment, of spaces as
/// wide as the comment was — save where `swallow` names a line, because a line
/// of spaces under a YAML block scalar body is indented into it (see
/// [`lines_a_removal_must_swallow`]). This is the layout that promises columns
/// and those lines are the one place it cannot keep that promise.
///
/// The display column is threaded from one edit to the next so every source
/// byte is inspected at most once. It also reflects an explicitly removed HTML
/// comment: because that edit emits no bytes, the newlines it covered do not
/// move the column the edits after it are measured from.
fn column_edits(source: &[u8], comments: &[Comment], swallow: &[Option<ByteSpan>]) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut cursor = 0usize;
    let mut column = 0usize;
    for (index, comment) in comments.iter().enumerate() {
        match comment.disposition().action() {
            Action::Keep => continue,
            Action::Rewrite => {
                edits.extend(rewrite_edit(comment));
                continue;
            }
            Action::Remove => {}
        }
        /* NOTE: A swallowed line takes its terminator with it, so what follows
         * starts a line of its own in the output as it did in the source and
         * the column count begins again there. */
        if let Some(line) = swallow.get(index).copied().flatten() {
            let span = ByteSpan::new(line.start.max(cursor), line.end);
            cursor = span.end;
            column = 0;
            edits.push(Edit {
                span,
                replacement: Vec::new(),
            });
            continue;
        }
        column = advance_display_column(source, cursor, comment.span.start, column);
        let (replacement, next) = if comment.kind == CommentKind::HtmlComment {
            (Vec::new(), column)
        } else {
            column_replacement(source, comment.span, column)
        };
        cursor = comment.span.end;
        column = next;
        edits.push(Edit {
            span: comment.span,
            replacement,
        });
    }
    edits
}

/// The edits [`Layout::Compact`] makes: [`Layout::Lines`], plus the promise
/// that a line which held nothing but a removed comment goes away instead of
/// staying behind as a blank one.
///
/// Whether a comment was alone on its line is judged from the bytes of the
/// original source, so a line holding two comments and nothing else keeps its
/// terminator: neither of them was alone on it.
///
/// The start of the current line is tracked forward through the whole source,
/// comment bodies included, so a comment beginning on a line that an earlier
/// comment ended is still measured from that line's real beginning.
///
/// `swallow` names the lines whose hole would carry meaning, and it reaches
/// further than a line: under a `|+` body it takes the empty lines the comment
/// was sheltering too (see [`lines_a_removal_must_swallow`]). Taking the line
/// is what `compact` does anyway, so this only ever widens what it takes, and
/// it is what keeps all three layouts writing the same bytes there.
fn compact_edits(source: &[u8], comments: &[Comment], swallow: &[Option<ByteSpan>]) -> Vec<Edit> {
    let mut edits = Vec::new();
    /* NOTE: Which edits the blank-run pass below may widen. A swallowed line
     * is the one place all three layouts are required to write the same bytes,
     * so it is left exactly where the other two put it. */
    let mut collapsible = Vec::new();
    let mut scan = 0usize;
    let mut line_start = 0usize;
    let mut floor = 0usize;
    for (index, comment) in comments.iter().enumerate() {
        match comment.disposition().action() {
            Action::Keep => continue,
            Action::Rewrite => {
                edits.extend(rewrite_edit(comment));
                continue;
            }
            Action::Remove => {}
        }
        if let Some(line) = swallow.get(index).copied().flatten() {
            let span = ByteSpan::new(line.start.max(floor), line.end.max(floor));
            floor = span.end;
            scan = span.end;
            line_start = span.end;
            edits.push(Edit {
                span,
                replacement: Vec::new(),
            });
            collapsible.push(false);
            continue;
        }
        while scan < comment.span.start {
            match unicode_line_terminator_width(source, scan) {
                Some(width) if scan + width <= comment.span.start => {
                    scan += width;
                    line_start = scan;
                }
                _ => scan += 1,
            }
        }
        /* NOTE: The next comment of any disposition, kept ones included: the
         * blanks an edit swallows must never reach into one. */
        let ceiling = comments
            .get(index + 1)
            .map_or(source.len(), |next| next.span.start)
            .max(comment.span.end);
        let edit = compact_edit(source, comment, line_start, floor, ceiling);
        floor = edit.span.end;
        edits.push(edit);
        collapsible.push(true);
    }
    collapse_created_blank_runs(source, &mut edits, &collapsible);
    edits
}

/// Take back the blank lines a removal *created*.
///
/// Dropping the line a comment held is what `compact` is for, and it is not
/// the whole of what the comment occupied. A comment set off by a blank line
/// above and another below is three lines of file for one comment, and taking
/// only the middle one leaves the two blanks touching — a run one line longer
/// than the file ever had, in a place where the file had never put one. Every
/// formatter with an opinion says so: `swift-format` reports `[RemoveLine]`,
/// `gofmt` closes the gap, `rustfmt` collapses it. A tool that has to be
/// followed by a formatter to finish its own edit has not finished it.
///
/// The rule is the narrow one, because widening it would mean reflowing a file
/// rather than removing a comment from it: **a removal never leaves more
/// consecutive blank lines than the longest run it was already standing next
/// to.** With `before` blanks above and `after` below, the removal takes
/// `min(before, after)` of the ones below it, which leaves `max(before,
/// after)`. Blank lines above a removal are never touched, and the count taken
/// can never exceed the count that followed the comment, so two lines of code
/// that had a blank line between them still do.
///
/// Only a removal that took whole lines is eligible: an edit that begins in
/// the middle of a line is a comment with code beside it, and the line it sits
/// on is staying.
fn collapse_created_blank_runs(source: &[u8], edits: &mut [Edit], collapsible: &[bool]) {
    if edits.is_empty() {
        return;
    }
    let starts = line_starts(source);
    let line_of = |offset: usize| starts.partition_point(|start| *start <= offset) - 1;
    let at_line_start = |offset: usize| starts.binary_search(&offset).is_ok();

    let mut index = 0;
    while index < edits.len() {
        if !collapsible.get(index).copied().unwrap_or(false)
            || !edits[index].replacement.is_empty()
            || !at_line_start(edits[index].span.start)
        {
            index += 1;
            continue;
        }
        /* NOTE: Comments written on consecutive lines are separate comments and
         * separate edits, and the blank runs either side belong to the block
         * they make together rather than to any one of them. So the touching
         * edits are treated as one removal. */
        let mut last = index;
        while last + 1 < edits.len()
            && collapsible.get(last + 1).copied().unwrap_or(false)
            && edits[last + 1].replacement.is_empty()
            && edits[last].span.end == edits[last + 1].span.start
        {
            last += 1;
        }
        let run_end = edits[last].span.end;
        if at_line_start(run_end) {
            let mut before = 0usize;
            let mut line = line_of(edits[index].span.start);
            while line > 0 && line_is_blank(source, &starts, line - 1) {
                before += 1;
                line -= 1;
            }
            let mut after = 0usize;
            let mut line = line_of(run_end);
            while line_is_blank(source, &starts, line) {
                after += 1;
                line += 1;
            }
            let mut end = run_end;
            for _ in 0..before.min(after) {
                match starts.get(line_of(end) + 1) {
                    Some(next) => end = *next,
                    None => break,
                }
            }
            /* INVARIANT: The blanks a removal takes must not reach the next
             * edit. They cannot in fact -- the line that edit is on holds a
             * comment and so is not blank -- but the clamp is what keeps the
             * edits provably sorted and non-overlapping. */
            let ceiling = edits
                .get(last + 1)
                .map_or(source.len(), |next| next.span.start);
            edits[last].span.end = end.min(ceiling).max(run_end);
        }
        index = last + 1;
    }
}

/// Where every line of `source` begins, in order, starting at `0`.
///
/// A source that ends with a terminator has a final entry at its length: the
/// empty last line, which is a line start with nothing on it and which
/// [`line_is_blank`] therefore refuses to call a blank line.
fn line_starts(source: &[u8]) -> Vec<usize> {
    let mut starts = vec![0usize];
    let mut index = 0;
    while index < source.len() {
        match unicode_line_terminator_width(source, index) {
            Some(width) => {
                index += width;
                starts.push(index);
            }
            None => index += 1,
        }
    }
    starts
}

/// Whether line `line` holds nothing but blanks and its terminator.
///
/// The position past the last terminator is not a line at all: there is no
/// line there to take, and counting it would let a removal at the end of a
/// file swallow the terminator that ends it.
fn line_is_blank(source: &[u8], starts: &[usize], line: usize) -> bool {
    let Some(&start) = starts.get(line) else {
        return false;
    };
    let end = starts.get(line + 1).copied().unwrap_or(source.len());
    if start >= end {
        return false;
    }
    let mut index = start;
    while index < end {
        match unicode_line_terminator_width(source, index) {
            Some(width) => index += width,
            None if source[index].is_ascii_whitespace() => index += 1,
            None => return false,
        }
    }
    true
}

/// One [`Layout::Compact`] edit.
///
/// `line_start` is where the line holding `comment` begins, `floor` is the end
/// of the previous edit and `ceiling` the start of the next comment, so the
/// span that comes back is sorted and non-overlapping with its neighbours
/// however a scanner laid the comments out.
fn compact_edit(
    source: &[u8],
    comment: &Comment,
    line_start: usize,
    floor: usize,
    ceiling: usize,
) -> Edit {
    let span = comment.span;
    /* NOTE: An HTML comment closes up completely under every layout, the
     * newlines it spanned included, so it never counts as ending a line by
     * spanning one and never puts a terminator back. */
    let html = comment.kind == CommentKind::HtmlComment;
    let interior = first_line_terminator(source, span);
    let tail = line_tail(source, span.end);
    let head_code = source[line_start..span.start]
        .iter()
        .any(|byte| !byte.is_ascii_whitespace());
    let ends_the_line = tail.is_some() || (interior.is_some() && !html);
    let start = if ends_the_line {
        blank_start(source, span.start, floor.max(line_start))
    } else {
        span.start
    };
    let eats_the_terminator = if html {
        !head_code
    } else {
        interior.is_some() || !head_code
    };
    let end = match tail {
        Some((blanks, terminator)) => blanks + if eats_the_terminator { terminator } else { 0 },
        None => span.end,
    };
    let replacement = if html {
        Vec::new()
    } else if !ends_the_line {
        /* NOTE: An interior comment: the line goes on after it, so keeping the
         * two tokens either side apart is the whole story, exactly as under
         * `lines`. */
        line_replacement(source, span)
    } else if let Some(terminator) = interior.filter(|_| head_code) {
        /* NOTE: The code before the comment keeps its own line, and the
         * terminator that ended that line was inside the comment. */
        terminator.to_vec()
    } else {
        /* NOTE: Nothing that survives on this line follows the comment, so the
         * line terminator - the one kept after it or the one that ended the
         * code line - is separator enough. */
        Vec::new()
    };
    Edit {
        span: ByteSpan::new(start, end.min(ceiling)),
        replacement,
    }
}

/// The first line terminator inside a comment, as the bytes that wrote it, so
/// a CRLF file keeps its CRLF. A terminator that would reach past the end of
/// the comment is not one: the same rule [`newline_sequence`] applies.
fn first_line_terminator(source: &[u8], span: ByteSpan) -> Option<&[u8]> {
    let mut index = span.start;
    while index < span.end {
        match unicode_line_terminator_width(source, index) {
            Some(width) if index + width <= span.end => return Some(&source[index..index + width]),
            _ => index += 1,
        }
    }
    None
}

/// How the line a comment ended on runs out: where the blanks after the
/// comment stop, and how wide the line terminator there is — `0` at the end of
/// the source. `None` when something other than blanks follows on that line,
/// which is what makes the comment an interior one rather than the last thing
/// on its line.
fn line_tail(source: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from;
    loop {
        if let Some(width) = unicode_line_terminator_width(source, index) {
            return Some((index, width));
        }
        match source.get(index) {
            None => return Some((index, 0)),
            Some(byte) if byte.is_ascii_whitespace() => index += 1,
            Some(_) => return None,
        }
    }
}

/// Where the run of blanks that ends at `at` begins. It never reaches before
/// `floor` and never crosses a line terminator, so trimming what a removal
/// left at the end of a line can never touch the line before it.
fn blank_start(source: &[u8], at: usize, floor: usize) -> usize {
    let mut index = at;
    while index > floor
        && source[index - 1].is_ascii_whitespace()
        && unicode_line_terminator_width(source, index - 1).is_none()
    {
        index -= 1;
    }
    index
}

fn newline_sequence(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(width) = unicode_line_terminator_width(bytes, index) {
            output.extend_from_slice(&bytes[index..index + width]);
            index += width;
        } else {
            index += 1;
        }
    }
    output
}

fn has_non_whitespace_neighbors(source: &[u8], span: ByteSpan) -> bool {
    source
        .get(span.start.wrapping_sub(1))
        .is_some_and(|byte| !byte.is_ascii_whitespace())
        && source
            .get(span.end)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
}

fn column_replacement(source: &[u8], span: ByteSpan, mut column: usize) -> (Vec<u8>, usize) {
    let mut output = Vec::with_capacity(span.len());
    let mut index = span.start;
    while index < span.end {
        if let Some(width) = unicode_line_terminator_width(source, index)
            && index + width <= span.end
        {
            output.extend_from_slice(&source[index..index + width]);
            index += width;
            column = 0;
            continue;
        }
        match source[index] {
            b'\t' => {
                let width = 8 - (column % 8);
                output.extend(std::iter::repeat_n(b' ', width));
                column += width;
                index += 1;
            }
            byte if byte.is_ascii() => {
                output.push(b' ');
                column += 1;
                index += 1;
            }
            _ => {
                if let Some((character, length)) = utf8_character(source, index, span.end) {
                    let width = character.width().unwrap_or(0);
                    output.extend(std::iter::repeat_n(b' ', width));
                    column += width;
                    index += length;
                } else {
                    // NOTE: Invalid source bytes each occupy one conservative display column.
                    output.push(b' ');
                    column += 1;
                    index += 1;
                }
            }
        }
    }
    (output, column)
}

fn advance_display_column(source: &[u8], mut index: usize, end: usize, mut column: usize) -> usize {
    while index < end {
        if let Some(width) = unicode_line_terminator_width(source, index)
            && index + width <= end
        {
            index += width;
            column = 0;
            continue;
        }
        if source[index] == b'\t' {
            column += 8 - (column % 8);
            index += 1;
        } else if source[index].is_ascii() {
            column += 1;
            index += 1;
        } else if let Some((character, length)) = utf8_character(source, index, end) {
            column += character.width().unwrap_or(0);
            index += length;
        } else {
            column += 1;
            index += 1;
        }
    }
    column
}

fn utf8_character(source: &[u8], index: usize, end: usize) -> Option<(char, usize)> {
    let length = match *source.get(index)? {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return None,
    };
    let bytes = source.get(index..index.checked_add(length)?)?;
    if index + length > end {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    Some((text.chars().next()?, length))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Policy, ScanOptions};
    use proptest::prelude::*;

    #[test]
    fn preserves_crlf_and_separates_tokens() {
        let result = transform(b"a/* x\r\ny */b", Language::C, TransformOptions::default());
        assert_eq!(result.output, b"a\r\nb");
        let joined = transform(b"a/*x*/b", Language::C, TransformOptions::default());
        assert_eq!(joined.output, b"a b");
    }

    #[test]
    fn invalid_input_is_not_edited_without_force() {
        let result = transform(b"x /* no end", Language::C, TransformOptions::default());
        assert!(!result.report.valid);
        assert!(result.edits.is_empty());
        assert_eq!(result.output, b"x /* no end");
    }

    #[test]
    fn external_spans_use_the_normal_policy_and_validate_boundaries() {
        let source = b"a/* ordinary */b/* directive */";
        let result = transform_spans(
            source,
            Language::Unknown,
            &[
                (ByteSpan::new(1, 15), CommentKind::Block),
                (ByteSpan::new(16, source.len()), CommentKind::Directive),
            ],
            TransformOptions::default(),
        )
        .unwrap();
        assert_eq!(result.output, b"a b/* directive */");
        assert!(matches!(
            transform_spans(
                source,
                Language::Unknown,
                &[(ByteSpan::new(2, source.len() + 1), CommentKind::Block)],
                TransformOptions::default(),
            ),
            Err(ExternalSpanError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn source_map_prefers_the_following_segment_at_edit_boundaries() {
        let source = b"ab/* remove */cd";
        let result = transform(source, Language::C, TransformOptions::default());
        let edit = &result.edits[0];
        assert_eq!(result.source_map.original_to_output(0), Some(0));
        assert_eq!(
            result.source_map.original_to_output(edit.span.start),
            Some(edit.span.start)
        );
        assert_eq!(
            result.source_map.original_to_output(edit.span.end),
            Some(edit.span.start + edit.replacement.len())
        );
        assert_eq!(
            result.source_map.original_to_output(source.len()),
            Some(result.output.len())
        );
        assert_eq!(
            result.source_map.output_to_original(result.output.len()),
            Some(source.len())
        );
    }

    #[test]
    fn html_is_byte_identical_in_standard_mode() {
        let input = b"a<!-- visible\ncomment -->b";
        assert_eq!(
            transform(input, Language::Html, TransformOptions::default()).output,
            input
        );
        let options = TransformOptions {
            scan: ScanOptions {
                policy: Policy::All,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(transform(input, Language::Html, options).output, b"ab");
    }

    proptest! {
        #[test]
        fn transform_is_idempotent(left in "[a-z ]{0,30}", body in "[a-z ]{0,30}", right in "[a-z ]{0,30}") {
            let input = format!("{left}/*{body}*/{right}").into_bytes();
            let first = transform(&input, Language::C, TransformOptions::default()).output;
            let second = transform(&first, Language::C, TransformOptions::default()).output;
            prop_assert_eq!(first, second);
        }
    }
}
