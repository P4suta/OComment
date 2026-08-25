use ocomment_core::{
    ByteSpan, DocumentChange, IncrementalDocument, Language, Layout, ScanOptions, TransformOptions,
    scan, transform,
};
use proptest::prelude::*;

fn newlines(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .copied()
        .filter(|byte| matches!(byte, b'\r' | b'\n'))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn lines_layout_keeps_the_exact_newline_sequence(body in "[A-Za-z0-9 \\t\\r\\n]{0,200}") {
        let source = format!("left/*{body}*/right").into_bytes();
        let result = transform(&source, Language::C, TransformOptions::default());
        prop_assert!(result.report.valid);
        prop_assert_eq!(newlines(&source), newlines(&result.output));
    }

    #[test]
    fn edits_are_sorted_non_overlapping_and_only_cover_comments(
        first in "[A-Za-z0-9 ]{0,40}", second in "[A-Za-z0-9 ]{0,40}", third in "[A-Za-z0-9 ]{0,40}")
    {
        let source = format!("x/*{first}*/y//{second}\nz/*{third}*/w").into_bytes();
        let result = transform(&source, Language::Rust, TransformOptions::default());
        prop_assert!(result.edits.windows(2).all(|pair| pair[0].span.end <= pair[1].span.start));
        for edit in &result.edits {
            prop_assert!(result.report.comments.iter().any(|comment| comment.span == edit.span));
        }
        let mut cursor = 0;
        let mut output_cursor = 0;
        for edit in &result.edits {
            let unchanged = edit.span.start - cursor;
            prop_assert_eq!(&source[cursor..edit.span.start], &result.output[output_cursor..output_cursor + unchanged]);
            cursor = edit.span.end;
            output_cursor += unchanged + edit.replacement.len();
        }
        prop_assert_eq!(&source[cursor..], &result.output[output_cursor..]);
    }

    #[test]
    fn string_contents_never_become_c_comments(content in "[A-Za-z0-9 /\\*#]{0,100}") {
        let escaped = content.replace('"', "\\\"");
        let source = format!("const char *s = \"{escaped}\";").into_bytes();
        let report = scan(&source, Language::C, ScanOptions::default());
        prop_assert!(report.comments.is_empty());
    }

    #[test]
    fn one_incremental_edit_always_matches_full_scan(
        prefix in "[a-z ;]{0,40}", old in "[a-z ]{0,30}", replacement in "[a-z ]{0,30}", suffix in "[a-z ;]{0,40}")
    {
        let source = format!("{prefix}/*{old}*/{suffix}").into_bytes();
        let start = prefix.len() + 2;
        let end = start + old.len();
        let mut document = IncrementalDocument::new(source, Language::Rust, ScanOptions::default(), 1);
        document.apply_changes(&[DocumentChange {
            span: ByteSpan::new(start, end), replacement: replacement.as_bytes().to_vec(),
        }], 2).unwrap();
        prop_assert_eq!(document.report(), &scan(document.source(), Language::Rust, ScanOptions::default()));
    }

    #[test]
    fn arbitrary_incremental_edits_match_full_scans_for_every_builtin(
        source in prop::collection::vec(prop_oneof![
            4 => any::<u8>(),
            2 => Just(b'\n'),
            1 => Just(b'\r'),
            1 => Just(b'/'),
            1 => Just(b'*'),
            1 => Just(b'\''),
            1 => Just(b'"'),
            1 => Just(b'#'),
            1 => Just(b'`'),
            1 => Just(b'{'),
            1 => Just(b'}'),
        ], 0..96),
        replacement in prop::collection::vec(any::<u8>(), 0..32),
        first in any::<usize>(),
        second in any::<usize>(),
    ) {
        let modulus = source.len() + 1;
        let left = first % modulus;
        let right = second % modulus;
        let span = ByteSpan::new(left.min(right), left.max(right));
        for language in Language::BUILT_INS {
            let mut document = IncrementalDocument::new(
                source.clone(),
                language,
                ScanOptions::default(),
                1,
            );
            document.apply_changes(&[DocumentChange {
                span,
                replacement: replacement.clone(),
            }], 2).unwrap();
            prop_assert_eq!(
                document.report(),
                &scan(document.source(), language, ScanOptions::default()),
                "incremental mismatch for {} at {:?}",
                language,
                span,
            );
        }
    }

    #[test]
    fn columns_layout_never_removes_line_boundaries(body in "[A-Za-z0-9 \\t\\r\\n]{0,100}") {
        let source = format!("/*{body}*/").into_bytes();
        let result = transform(&source, Language::Css, TransformOptions { layout: Layout::Columns, ..Default::default() });
        prop_assert_eq!(newlines(&source), newlines(&result.output));
    }
}
