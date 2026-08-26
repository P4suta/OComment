//! What [`Layout::Compact`] leaves behind, line by line.
//!
//! `compact` is `lines` plus one promise: a line that held nothing but a
//! removed comment goes away instead of staying behind as a blank one. Every
//! case here is a source where that promise is visible, and each is checked
//! against `lines` as well, because the two layouts have to differ exactly
//! where the promise says they do and nowhere else.

use ocomment_core::{
    ByteSpan, CommentKind, Disposition, Language, Layout, Policy, ScanOptions, TransformOptions,
    apply_edits, transform, transform_spans,
};
use proptest::prelude::*;

/// One transformation, with every structural promise checked on the way out:
/// edits sorted, non-overlapping, inside the source, reproducing the output in
/// one pass, and a source map that still maps both ends.
fn transformed(source: &[u8], language: Language, policy: Policy, layout: Layout) -> Vec<u8> {
    let result = transform(
        source,
        language,
        TransformOptions {
            scan: ScanOptions {
                policy,
                ..ScanOptions::default()
            },
            layout,
        },
    );
    let mut cursor = 0;
    for edit in &result.edits {
        assert!(
            edit.span.start <= edit.span.end,
            "inverted edit {:?}",
            edit.span
        );
        assert!(
            edit.span.start >= cursor,
            "edit {:?} overlaps its predecessor, which ended at {cursor}",
            edit.span
        );
        assert!(
            edit.span.end <= source.len(),
            "edit {:?} reaches past the {}-byte source",
            edit.span,
            source.len()
        );
        cursor = edit.span.end;
    }
    assert_eq!(
        apply_edits(source, &result.edits),
        result.output,
        "the edits do not reproduce the output"
    );
    assert_eq!(
        result
            .report
            .comments
            .iter()
            .filter(|comment| comment.disposition.is_remove())
            .count(),
        result.edits.len(),
        "one edit per removed comment"
    );
    assert_eq!(
        result.source_map.original_to_output(source.len()),
        Some(result.output.len()),
        "the end of the source does not map to the end of the output"
    );
    assert_eq!(
        result.source_map.output_to_original(result.output.len()),
        Some(source.len()),
        "the end of the output does not map back to the end of the source"
    );
    assert_eq!(
        result.source_map.original_to_output(0),
        Some(0),
        "the start of the source does not map to the start of the output"
    );
    result.output
}

/// `compact` over Rust under the default `safe` policy.
fn compact(source: &str) -> String {
    String::from_utf8(transformed(
        source.as_bytes(),
        Language::Rust,
        Policy::Safe,
        Layout::Compact,
    ))
    .expect("compact never splits a character")
}

/// `lines` over the same source, for the side-by-side assertions.
fn lines(source: &str) -> String {
    String::from_utf8(transformed(
        source.as_bytes(),
        Language::Rust,
        Policy::Safe,
        Layout::Lines,
    ))
    .expect("lines never splits a character")
}

#[test]
fn a_comment_alone_on_its_line_takes_the_line_with_it() {
    let source = "fn main() {}\n// one\n// two\nlet x = 1;\n";
    assert_eq!(compact(source), "fn main() {}\nlet x = 1;\n");
    assert_eq!(lines(source), "fn main() {}\n\n\nlet x = 1;\n");
}

#[test]
fn indentation_of_a_removed_line_goes_with_it() {
    let source = "fn main() {\n    // note\n    let x = 1;\n}\n";
    assert_eq!(compact(source), "fn main() {\n    let x = 1;\n}\n");
    assert_eq!(lines(source), "fn main() {\n    \n    let x = 1;\n}\n");
}

#[test]
fn crlf_lines_keep_their_endings() {
    let source = "let x = 1;\r\n// note\r\nlet y = 2;\r\n";
    assert_eq!(compact(source), "let x = 1;\r\nlet y = 2;\r\n");
    let shared = "let x = 1; /* one\r\ntwo */ let y = 2;\r\n";
    assert_eq!(compact(shared), "let x = 1;\r\n let y = 2;\r\n");
    assert_eq!(lines(shared), "let x = 1; \r\n let y = 2;\r\n");
}

#[test]
fn the_first_line_of_a_file_goes_like_any_other() {
    let source = "// header\nfn main() {}\n";
    assert_eq!(compact(source), "fn main() {}\n");
    assert_eq!(lines(source), "\nfn main() {}\n");
    assert_eq!(compact("// only\n"), "");
    assert_eq!(compact("// only"), "");
}

#[test]
fn a_surviving_line_keeps_the_ending_it_had_or_its_absence() {
    assert_eq!(compact("let x = 1; // note"), "let x = 1;");
    assert_eq!(lines("let x = 1; // note"), "let x = 1; ");
    // NOTE: The last line held nothing else, so it goes; the line before it
    // NOTE: keeps the terminator it always had.
    assert_eq!(compact("let x = 1;\n// note"), "let x = 1;\n");
    assert_eq!(compact("let x = 1;\n// one\n// two"), "let x = 1;\n");
    // NOTE: Here the terminator that ended the surviving line was inside the
    // NOTE: comment, so it comes back even though the file ended without one.
    assert_eq!(compact("let x = 1; /* one\ntwo */"), "let x = 1;\n");
    assert_eq!(lines("let x = 1; /* one\ntwo */"), "let x = 1; \n");
}

#[test]
fn whitespace_left_before_an_end_of_line_comment_is_trimmed() {
    assert_eq!(compact("let x = 1; \t // note\n"), "let x = 1;\n");
    assert_eq!(lines("let x = 1; \t // note\n"), "let x = 1; \t \n");
    assert_eq!(compact("let x = 1; /* note */  \n"), "let x = 1;\n");
    assert_eq!(compact("let x = 1; /* note */  "), "let x = 1;");
}

#[test]
fn a_block_comment_that_shares_a_line_with_code_keeps_that_line() {
    let before = "let a = 1; /* one\ntwo\nthree */\nlet b = 2;\n";
    assert_eq!(compact(before), "let a = 1;\nlet b = 2;\n");
    assert_eq!(lines(before), "let a = 1; \n\n\nlet b = 2;\n");
    let after = "let a = 1;\n/* one\ntwo\nthree */ let b = 2;\n";
    assert_eq!(compact(after), "let a = 1;\n let b = 2;\n");
    let both = "let a = 1; /* one\ntwo\nthree */ let b = 2;\n";
    assert_eq!(compact(both), "let a = 1;\n let b = 2;\n");
    assert_eq!(lines(both), "let a = 1; \n\n let b = 2;\n");
}

#[test]
fn a_block_comment_alone_on_its_lines_takes_all_of_them() {
    let source = "let a = 1;\n/* one\ntwo\nthree */\nlet b = 2;\n";
    assert_eq!(compact(source), "let a = 1;\nlet b = 2;\n");
    assert_eq!(lines(source), "let a = 1;\n\n\n\nlet b = 2;\n");
}

#[test]
fn a_comment_between_two_tokens_is_left_exactly_as_lines_leaves_it() {
    for source in [
        "let x = a/* widen */+ b;\n",
        "let x = a /* widen */ + b;\n",
        "let x = a/* widen */ + b;\n",
        "let x = a/* one */b/* two */c;\n",
    ] {
        assert_eq!(
            compact(source),
            lines(source),
            "compact and lines disagree about {source:?}"
        );
    }
}

#[test]
fn lines_and_columns_are_not_touched_by_the_compact_rules() {
    let source = "fn main() {\n    // note\n    let x = a /* widen */ + b; // trailing\n}\n";
    assert_eq!(
        String::from_utf8(transformed(
            source.as_bytes(),
            Language::Rust,
            Policy::Safe,
            Layout::Lines,
        ))
        .unwrap(),
        "fn main() {\n    \n    let x = a  + b; \n}\n"
    );
    assert_eq!(
        String::from_utf8(transformed(
            source.as_bytes(),
            Language::Rust,
            Policy::Safe,
            Layout::Columns,
        ))
        .unwrap(),
        "fn main() {\n           \n    let x = a             + b;            \n}\n"
    );
    assert_eq!(compact(source), "fn main() {\n    let x = a  + b;\n}\n");
}

#[test]
fn an_html_comment_still_closes_up_completely() {
    let whole_line = "<p>a</p>\n<!-- note -->\n<p>b</p>\n";
    assert_eq!(
        String::from_utf8(transformed(
            whole_line.as_bytes(),
            Language::Html,
            Policy::All,
            Layout::Compact,
        ))
        .unwrap(),
        "<p>a</p>\n<p>b</p>\n"
    );
    let inline = "a<!-- one\ntwo -->b";
    assert_eq!(
        String::from_utf8(transformed(
            inline.as_bytes(),
            Language::Html,
            Policy::All,
            Layout::Compact,
        ))
        .unwrap(),
        "ab"
    );
    let trailing = "<p>a</p> <!-- one\ntwo -->\n<p>b</p>\n";
    assert_eq!(
        String::from_utf8(transformed(
            trailing.as_bytes(),
            Language::Html,
            Policy::All,
            Layout::Compact,
        ))
        .unwrap(),
        "<p>a</p>\n<p>b</p>\n"
    );
}

#[test]
fn a_unicode_line_terminator_ends_a_line_like_any_other() {
    // NOTE: ECMA-262 12.3: U+2028 LINE SEPARATOR is a LineTerminator, so it
    // NOTE: ends the comment, and the line it ended goes with the comment.
    let source = "let a = 1;\u{2028}// note\u{2028}let b = 2;\n";
    assert_eq!(
        String::from_utf8(transformed(
            source.as_bytes(),
            Language::JavaScript,
            Policy::Safe,
            Layout::Compact,
        ))
        .unwrap(),
        "let a = 1;\u{2028}let b = 2;\n"
    );
    // NOTE: `lines` keeps the emptied line and the space that kept the two
    // NOTE: terminators from meeting.
    assert_eq!(
        String::from_utf8(transformed(
            source.as_bytes(),
            Language::JavaScript,
            Policy::Safe,
            Layout::Lines,
        ))
        .unwrap(),
        "let a = 1;\u{2028} \u{2028}let b = 2;\n"
    );
}

#[test]
fn a_kept_comment_holds_its_line_open() {
    let source = "// rustfmt::skip\n// note\nfn main() {}\n";
    assert_eq!(compact(source), "// rustfmt::skip\nfn main() {}\n");
    let shared = "let x = 1; // rustfmt::skip\n/* note */\nlet y = 2;\n";
    assert_eq!(compact(shared), "let x = 1; // rustfmt::skip\nlet y = 2;\n");
}

#[test]
fn external_spans_with_blanks_between_them_stay_non_overlapping() {
    // NOTE: A plugin or a declarative profile may report any spans the
    // NOTE: validator accepts, including two with nothing but blanks between
    // NOTE: them, and the edits still have to be sorted and non-overlapping.
    let source = b"x\na  \nb";
    let result = transform_spans(
        source,
        Language::Unknown,
        &[
            (ByteSpan::new(2, 3), CommentKind::Line),
            (ByteSpan::new(4, 5), CommentKind::Line),
        ],
        TransformOptions {
            layout: Layout::Compact,
            ..TransformOptions::default()
        },
    )
    .expect("the spans are sorted, non-empty and inside the source");
    let mut cursor = 0;
    for edit in &result.edits {
        assert!(
            edit.span.start >= cursor,
            "edit {:?} overlaps its predecessor, which ended at {cursor}",
            edit.span
        );
        cursor = edit.span.end;
    }
    assert_eq!(apply_edits(source, &result.edits), result.output);
    assert_eq!(result.output, b"x\n\nb");
}

/// The half-open span of `needle`, which must occur exactly once in `source`.
fn only_span(source: &[u8], needle: &[u8]) -> ByteSpan {
    let mut found = source
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| *window == needle)
        .map(|(start, _)| start);
    let start = found
        .next()
        .unwrap_or_else(|| panic!("`{}` is not in the source", String::from_utf8_lossy(needle)));
    assert_eq!(
        found.next(),
        None,
        "`{}` occurs more than once",
        String::from_utf8_lossy(needle)
    );
    ByteSpan::new(start, start + needle.len())
}

/// The hand-off gets the positional keep a built-in scan gets.
///
/// A YAML block scalar reads the lines below it, so the comment that ends one
/// is not commentary: take its line and the kept directive under it is handed
/// back to the body. The bytes of that comment say nothing about this, so an
/// external scanner cannot classify it — `transform_spans` has to apply the
/// rule itself. It has to under every layout, because the least any of them can
/// leave in place of that line is a blank one, and a blank line is content of
/// the body above it whatever its indentation.
#[test]
fn external_spans_keep_the_comment_a_yaml_block_scalar_leans_on() {
    let source =
        b"a: 1 # trailing note\nk: |\n  body\n# ends the block\n  # yamllint disable\nz: 1\n";
    let trailing = only_span(source, b"# trailing note");
    let ends_block = only_span(source, b"# ends the block");
    let directive = only_span(source, b"# yamllint disable");
    // NOTE: All three layouts agree about the structural comment and differ
    // NOTE: only over the trailing note: `columns` pads its width back, `lines`
    // NOTE: leaves the space in front of it, `compact` trims that space away.
    let expected: [(Layout, &[u8]); 3] = [
        (
            Layout::Lines,
            b"a: 1 \nk: |\n  body\n# ends the block\n  # yamllint disable\nz: 1\n",
        ),
        (
            Layout::Columns,
            b"a: 1                \nk: |\n  body\n# ends the block\n  # yamllint disable\nz: 1\n",
        ),
        (
            Layout::Compact,
            b"a: 1\nk: |\n  body\n# ends the block\n  # yamllint disable\nz: 1\n",
        ),
    ];
    for (layout, output) in expected {
        let result = transform_spans(
            source,
            Language::Yaml,
            &[
                (trailing, CommentKind::Line),
                (ends_block, CommentKind::Line),
                (directive, CommentKind::Directive),
            ],
            TransformOptions {
                layout,
                ..TransformOptions::default()
            },
        )
        .expect("the spans are sorted, non-empty and inside the source");
        assert_eq!(
            result.report.comments[1].disposition,
            Disposition::Keep {
                reason: "structural in a YAML block scalar trail".into()
            },
            "{layout:?} let the hand-off remove the comment the block scalar ends at"
        );
        assert!(
            result.edits.iter().all(|edit| edit.span != ends_block),
            "{layout:?} emitted an edit for it anyway: {:?}",
            result.edits
        );
        // NOTE: The trailing note leans on nothing, so it goes: the pass is the
        // NOTE: one keep the shape asks for, not a blanket amnesty for YAML.
        assert_eq!(
            result.edits.len(),
            1,
            "{layout:?} edits: {:?}",
            result.edits
        );
        assert_eq!(result.edits[0].span.end, trailing.end, "{layout:?}");
        assert_eq!(result.output, output, "{layout:?}");
        assert_eq!(
            apply_edits(source, &result.edits),
            result.output,
            "{layout:?}"
        );
    }
}

proptest! {
    /// With every removed comment between two tokens on one line, `compact`
    /// has no line to drop and no trailing whitespace to trim, so it must
    /// leave exactly the bytes `lines` leaves.
    #[test]
    fn compact_equals_lines_when_no_comment_ends_its_line(
        left in "[a-z]{1,8}", body in "[a-z ]{0,20}", right in "[a-z]{1,8}", tail in "[a-z]{1,8}")
    {
        let source = format!("{left}/*{body}*/{right}\n{tail}\n");
        prop_assert_eq!(compact(&source), lines(&source));
    }

    /// A comment alone on its line is the one case the two layouts differ
    /// over, and they differ by exactly that line.
    #[test]
    fn compact_drops_the_line_that_lines_leaves_blank(
        indent in " {0,6}", body in "[a-z ]{0,20}", head in "[a-z]{1,8}", tail in "[a-z]{1,8}")
    {
        let source = format!("{head}\n{indent}// note {body}\n{tail}\n");
        let compacted = compact(&source);
        prop_assert_eq!(&compacted, &format!("{head}\n{tail}\n"));
        prop_assert_eq!(lines(&source), format!("{head}\n{indent}\n{tail}\n"));
        prop_assert!(compacted.len() < lines(&source).len());
    }
}
