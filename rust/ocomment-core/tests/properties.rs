//! Randomised properties the engine holds for every input.
//!
//! The generators favour the bytes that open and close lexical states, so
//! the cases are unlikely rather than merely random.

use ocomment_core::{
    ByteSpan, DocumentChange, IncrementalDocument, Language, Layout, ScanOptions, TransformOptions,
    lexical_pool, scan, transform,
};
use proptest::{prelude::*, sample::select};

/// A pool length as a `prop_oneof!` weight, so that drawing uniformly from a
/// pool of `n` gives each of its members the weight one arm would have.
fn weight(length: usize) -> u32 {
    u32::try_from(length).expect("the pool is far smaller than a weight")
}

/// One byte of the shared pool, or a uniformly random one.
///
/// The pool is `ocomment_core::lexical_pool::BYTES`, and the checkpoint
/// properties in `src/incremental.rs` draw from the same one: a fragment worth
/// generating against the whole-file scanner is worth generating against the
/// incremental one. The extra `\n` arm doubles that byte's weight, because a
/// line boundary is where most of the interesting lexical states begin and end.
fn lexical_byte() -> impl Strategy<Value = u8> {
    prop_oneof![
        4 => any::<u8>(),
        1 => Just(b'\n'),
        weight(lexical_pool::BYTES.len()) => select(lexical_pool::BYTES),
    ]
}

/// A fragment: one byte of the pool, or one whole token from it.
///
/// The tokens are `ocomment_core::lexical_pool::TOKENS` — multi-byte openers a
/// single-byte alphabet can never synthesise, and the reason each of them is
/// there is written out beside the list. Each is drawn as often as one byte is,
/// which is what the eight-to-one weight in front of the byte arm keeps in
/// proportion.
fn lexical_fragment() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        8 => lexical_byte().prop_map(|byte| vec![byte]),
        weight(lexical_pool::TOKENS.len()) => select(lexical_pool::TOKENS).prop_map(<[u8]>::to_vec),
    ]
}

/// A source built from at most `fragments` raw bytes and literal tokens.
fn lexical_source(fragments: std::ops::Range<usize>) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(lexical_fragment(), fragments).prop_map(|fragments| fragments.concat())
}

fn newlines(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .copied()
        .filter(|byte| matches!(byte, b'\r' | b'\n'))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::default())]

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
        source in lexical_source(0..48),
        replacement in lexical_source(0..8),
        first in any::<usize>(),
        second in any::<usize>(),
    ) {
        let modulus = source.len() + 1;
        let left = first % modulus;
        let right = second % modulus;
        let span = ByteSpan::new(left.min(right), left.max(right));
        for language in Language::ALL {
            let original = scan(&source, language, ScanOptions::default());
            prop_assert!(
                original.comments.iter().all(|comment|
                    comment.span.start <= comment.span.end && comment.span.end <= source.len()),
                "comment span outside source for {}: {:?}",
                language,
                original.comments,
            );
            prop_assert!(
                original.diagnostics.iter().all(|diagnostic|
                    diagnostic.span.start <= diagnostic.span.end && diagnostic.span.end <= source.len()),
                "diagnostic span outside source for {}: {:?}",
                language,
                original.diagnostics,
            );
            let transformed = transform(&source, language, TransformOptions::default());
            prop_assert!(transformed.edits.iter().all(|edit|
                edit.span.start <= edit.span.end && edit.span.end <= source.len()));
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
            let full = scan(document.source(), language, ScanOptions::default());
            prop_assert!(full.comments.iter().all(|comment|
                comment.span.start <= comment.span.end && comment.span.end <= document.source().len()));
            prop_assert!(full.diagnostics.iter().all(|diagnostic|
                diagnostic.span.start <= diagnostic.span.end && diagnostic.span.end <= document.source().len()));
            prop_assert_eq!(
                document.report(),
                &full,
                "incremental mismatch for {} at {:?}",
                language,
                span,
            );
        }
        for dialect in [ocomment_core::Dialect::Scss, ocomment_core::Dialect::Sass] {
            let options = ScanOptions { dialect, ..ScanOptions::default() };
            let report = scan(&source, Language::Css, options.clone());
            prop_assert!(report.comments.iter().all(|comment|
                comment.span.start <= comment.span.end && comment.span.end <= source.len()));
            prop_assert!(report.diagnostics.iter().all(|diagnostic|
                diagnostic.span.start <= diagnostic.span.end && diagnostic.span.end <= source.len()));
            let transformed = transform(
                &source,
                Language::Css,
                TransformOptions { scan: options, ..TransformOptions::default() },
            );
            prop_assert!(transformed.edits.iter().all(|edit|
                edit.span.start <= edit.span.end && edit.span.end <= source.len()));
        }
    }

    #[test]
    fn columns_layout_never_removes_line_boundaries(body in "[A-Za-z0-9 \\t\\r\\n]{0,100}") {
        let source = format!("/*{body}*/").into_bytes();
        let result = transform(&source, Language::Css, TransformOptions { layout: Layout::Columns, ..Default::default() });
        prop_assert_eq!(newlines(&source), newlines(&result.output));
    }
}

/// The two counterexamples recorded in `properties.proptest-regressions` were
/// drawn from the single-byte alphabet this file's generator no longer uses on
/// its own, so proptest can no longer replay them from their seeds. They are
/// kept here verbatim instead: both are unterminated Rust character literals
/// whose six-byte lookahead straddles the rescan window.
#[test]
fn recorded_counterexamples_still_match_a_full_scan_for_every_builtin() {
    let cases: [(&[u8], ByteSpan, &[u8]); 2] = [
        (
            &[
                0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 39, 128, 10, 34, 39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            ByteSpan::new(13, 15),
            &[],
        ),
        (
            &[
                0, 0, 35, 0, 39, 128, 34, 39, 10, 39, 0, 35, 35, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0,
            ],
            ByteSpan::new(5, 8),
            &[128],
        ),
    ];
    for (source, span, replacement) in cases {
        for language in Language::ALL {
            let mut document =
                IncrementalDocument::new(source.to_vec(), language, ScanOptions::default(), 1);
            document
                .apply_changes(
                    &[DocumentChange {
                        span,
                        replacement: replacement.to_vec(),
                    }],
                    2,
                )
                .unwrap();
            assert_eq!(
                document.report(),
                &scan(document.source(), language, ScanOptions::default()),
                "incremental mismatch for {language} at {span:?}",
            );
        }
    }
}
