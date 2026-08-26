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
    let mut edits = Vec::new();
    let mut column_cursor = 0usize;
    let mut display_column = 0usize;
    if report.valid || options.scan.force_invalid {
        for comment in report
            .comments
            .iter()
            .filter(|comment| comment.disposition.is_remove())
        {
            let replacement = if options.layout == Layout::Columns {
                display_column = advance_display_column(
                    source,
                    column_cursor,
                    comment.span.start,
                    display_column,
                );
                let (replacement, next_column) = if comment.kind == CommentKind::HtmlComment {
                    (Vec::new(), display_column)
                } else {
                    column_replacement(source, comment.span, display_column)
                };
                column_cursor = comment.span.end;
                display_column = next_column;
                replacement
            } else if comment.kind == CommentKind::HtmlComment {
                Vec::new()
            } else {
                replacement(source, comment.span, options.layout)
            };
            edits.push(Edit {
                span: comment.span,
                replacement,
            });
        }
    }
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

fn replacement(source: &[u8], span: ByteSpan, layout: Layout) -> Vec<u8> {
    let comment = &source[span.start..span.end];
    match layout {
        Layout::Columns => unreachable!("column replacement requires tracked display state"),
        Layout::Lines => {
            let mut output = newline_sequence(comment);
            if output.is_empty() && has_non_whitespace_neighbors(source, span) {
                output.push(b' ');
            } else if !output.is_empty() && needs_leading_separator(source, span) {
                // NOTE: A newline is already a lexical separator; no extra byte is needed.
            }
            output
        }
        Layout::Compact => {
            let output = newline_sequence(comment);
            if !output.is_empty() {
                output
            } else if has_non_whitespace_neighbors(source, span) {
                vec![b' ']
            } else {
                Vec::new()
            }
        }
    }
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

fn needs_leading_separator(source: &[u8], span: ByteSpan) -> bool {
    source
        .get(span.start.wrapping_sub(1))
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
