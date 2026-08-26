use crate::{
    ByteSpan, Comment, CommentKind, Edit, ExternalSpanError, Language, Layout, ScanReport,
    SourceMap, TransformOptions, TransformResult, scan,
    scanner::{DispositionPatterns, disposition, unicode_line_terminator_width},
};
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
    let report = scan(source, language, options.scan.clone());
    transform_report(source, report, options)
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
    let patterns = DispositionPatterns::compile(&options.scan)
        .map_err(|error| ExternalSpanError::InvalidPattern(error.to_string()))?;
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
        comments.push(Comment {
            span,
            kind,
            disposition: disposition(
                kind,
                &options.scan,
                &source[span.start..span.end],
                &patterns,
            ),
        });
    }
    Ok(transform_report(
        source,
        ScanReport {
            language,
            comments,
            diagnostics: Vec::new(),
            valid: true,
        },
        options,
    ))
}

pub(crate) fn transform_report(
    source: &[u8],
    report: crate::ScanReport,
    options: TransformOptions,
) -> TransformResult {
    let edits = if report.valid || options.scan.force_invalid {
        match options.layout {
            Layout::Lines => line_edits(source, &report.comments),
            Layout::Columns => column_edits(source, &report.comments),
            Layout::Compact => compact_edits(source, &report.comments),
        }
    } else {
        Vec::new()
    };
    let output = apply_edits(source, &edits);
    let source_map = SourceMap::from_edits(source.len(), &edits);
    TransformResult {
        output,
        edits,
        report,
        source_map,
    }
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

/// The removable comments of a report, in source order.
fn removable(comments: &[Comment]) -> impl Iterator<Item = &Comment> {
    comments
        .iter()
        .filter(|comment| comment.disposition.is_remove())
}

/// The edits [`Layout::Lines`] makes: one per removed comment, over exactly
/// the bytes that comment covers.
fn line_edits(source: &[u8], comments: &[Comment]) -> Vec<Edit> {
    removable(comments)
        .map(|comment| Edit {
            span: comment.span,
            replacement: if comment.kind == CommentKind::HtmlComment {
                Vec::new()
            } else {
                line_replacement(source, comment.span)
            },
        })
        .collect()
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

/// The edits [`Layout::Columns`] makes.
///
/// The display column is threaded from one edit to the next so every source
/// byte is inspected at most once. It also reflects an explicitly removed HTML
/// comment: because that edit emits no bytes, the newlines it covered do not
/// move the column the edits after it are measured from.
fn column_edits(source: &[u8], comments: &[Comment]) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut cursor = 0usize;
    let mut column = 0usize;
    for comment in removable(comments) {
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
fn compact_edits(source: &[u8], comments: &[Comment]) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut scan = 0usize;
    let mut line_start = 0usize;
    let mut floor = 0usize;
    for (index, comment) in comments.iter().enumerate() {
        if !comment.disposition.is_remove() {
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
    }
    edits
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
    fn html_is_byte_identical_in_safe_mode() {
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
