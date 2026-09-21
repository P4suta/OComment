//! Randomised properties the engine holds for every input.
//!
//! The generators favour the bytes that open and close lexical states, so the cases are unlikely rather than merely random.

use ocomment_core::{
    ByteSpan, DocumentChange, IncrementalDocument, Language, Layout, ScanOptions, TransformOptions,
    lexical_pool, scan, transform,
};
use proptest::{prelude::*, sample::select};

/// A pool length as a `prop_oneof!` weight, so that drawing uniformly from a pool of `n` gives each of its members the weight one arm would have.
fn weight(length: usize) -> u32 {
    u32::try_from(length).expect("the pool is far smaller than a weight")
}

/// One byte of the shared pool, or a uniformly random one.
///
/// The pool is `ocomment_core::lexical_pool::BYTES`, and the checkpoint properties in `src/incremental.rs` draw from the same one: a fragment worth generating against the whole-file scanner is worth generating against the incremental one.
/// The extra `\n` arm doubles that byte's weight, because a line boundary is where most of the interesting lexical states begin and end.
fn lexical_byte() -> impl Strategy<Value = u8> {
    prop_oneof![
        4 => any::<u8>(),
        1 => Just(b'\n'),
        weight(lexical_pool::BYTES.len()) => select(lexical_pool::BYTES),
    ]
}

/// A fragment: one byte of the pool, or one whole token from it.
///
/// The tokens are `ocomment_core::lexical_pool::TOKENS` — multi-byte openers a single-byte alphabet can never synthesise, and the reason each of them is there is written out beside the list.
/// Each is drawn as often as one byte is,
/// which is what the eight-to-one weight in front of the byte arm keeps in proportion.
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
        /* NOTE: The witness.
         * A property about what a removal preserves is vacuously true of a run that removed nothing, so a scanner that stopped finding this comment would pass this test forever. */
        prop_assert_eq!(result.report.comments.len(), 1);
        prop_assert_eq!(newlines(&source), newlines(&result.output));
    }

    #[test]
    fn edits_are_sorted_non_overlapping_and_only_cover_comments(
        first in "[A-Za-z0-9 ]{0,40}", second in "[A-Za-z0-9 ]{0,40}", third in "[A-Za-z0-9 ]{0,40}")
    {
        let source = format!("x/*{first}*/y//{second}\nz/*{third}*/w").into_bytes();
        let result = transform(&source, Language::Rust, TransformOptions::default());
        // NOTE: The witness; see `lines_layout_keeps_the_exact_newline_sequence`.
        prop_assert_eq!(result.edits.len(), 3);
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
        /* NOTE: The negative control, and this property needs one more than most: "nothing was found" is what a scanner that found nothing anywhere would also say.
         * The same bytes outside the string have to be found whenever they spell a comment, so the silence above is the string doing its job rather than the scanner having stopped. */
        if content.contains("//") || content.contains("/*") {
            let bare = format!("int x = 1; {content}\n").into_bytes();
            let outside = scan(&bare, Language::C, ScanOptions::default());
            prop_assert!(
                !outside.comments.is_empty() || !outside.valid,
                "the same bytes outside a string were not a comment either: {content}"
            );
        }
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

/// The two counterexamples recorded in `properties.proptest-regressions` were drawn from the single-byte alphabet this file's generator no longer uses on its own, so proptest can no longer replay them from their seeds.
/// They are kept here verbatim instead: both are unterminated Rust character literals whose six-byte lookahead straddles the rescan window.
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

/// The style rules under a policy that removes nothing, which is the only way to watch them on their own.
fn style_only(rules: ocomment_core::StyleRules) -> TransformOptions {
    TransformOptions {
        scan: ScanOptions {
            policy: ocomment_core::Policy::None,
            style: rules,
            ..ScanOptions::default()
        },
        layout: Layout::Lines,
    }
}

/// Every style rule at once, which is the hardest case: the rules compose, and a property that held for each alone could still fail for the pair.
fn every_style_rule() -> ocomment_core::StyleRules {
    ocomment_core::StyleRules {
        wrap: ocomment_core::Wrap::Sentence,
        space_after_marker: Some(true),
        trailing_whitespace: Some(false),
    }
}

/// `bytes` with every ASCII space, tab and line break taken out.
///
/// What a rewrite is allowed to move, and therefore what a comparison of the two sides has to ignore to be a comparison of the words.
fn without_spacing(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect()
}

proptest! {
        /// Rewriting twice is rewriting once.
    ///
    /// The property a formatter is worth nothing without, and the one the prose gate this replaces did not have: its checker accepted line breaks its fixer would go on to remove, so running the fixer produced a file the checker liked and the fixer would change again.
    #[test]
    fn restyling_a_restyled_source_changes_nothing(source in lexical_source(0..48)) {
        for language in [Language::Rust, Language::Python, Language::Html, Language::Ocaml] {
            let options = style_only(every_style_rule());
            let once = transform(&source, language, options.clone());
            if !once.report.valid {
                continue;
            }
            let twice = transform(&once.output, language, options);
            prop_assert_eq!(
                &twice.output, &once.output,
                "{} rewrote its own output: {:?}", language, String::from_utf8_lossy(&once.output)
            );
        }
    }

        /// What `fix` writes, `check` has nothing left to say about.
    ///
    /// Idempotence says the bytes settle; this says the *report* settles.
    /// The two are not the same claim, and it is the second one a gate depends on:
    /// a run whose output still holds findings is a run that fails the commit it was asked to clean.
    #[test]
    fn a_restyled_source_holds_no_findings(source in lexical_source(0..48)) {
        for language in [Language::Rust, Language::Python, Language::Html, Language::Ocaml] {
            let options = style_only(every_style_rule());
            let result = transform(&source, language, options.clone());
            if !result.report.valid {
                continue;
            }
            let after = scan(&result.output, language, options.scan.clone());
            if !after.valid {
                continue;
            }
            for comment in &after.comments {
                prop_assert!(
                    !comment.action().changes_bytes(),
                    "{language} left a finding in its own output: {:?} in {:?}",
                    comment,
                    String::from_utf8_lossy(&result.output)
                );
            }
        }
    }

        /// A rewrite that touches one comment leaves one comment of the same kind.
    ///
    /// The failure this rules out is the one the prose gate shipped: it rebuilt `/* One. Two. */` as two lines each opening `/*` and closing neither, so a formatter asked to tidy a file wrote a file that did not compile.
    /// Nothing about that is specific to block comments — it is what happens whenever a rewrite forgets a delimiter.
    ///
    /// The wrap rule is deliberately out of this one.
    /// Moving a line break between two comments is *meant* to change how many comments there are,
    /// and `a_reflowed_source_is_still_the_same_comments` is what holds that.
    #[test]
    fn a_comment_rewrite_leaves_one_comment_of_the_same_kind(source in lexical_source(0..48)) {
        for language in [Language::Rust, Language::Python, Language::Html, Language::Ocaml] {
            let options = style_only(ocomment_core::StyleRules {
                wrap: ocomment_core::Wrap::Preserve,
                ..every_style_rule()
            });
            let result = transform(&source, language, options.clone());
            if !result.report.valid {
                continue;
            }
            let after = scan(&result.output, language, options.scan.clone());
            prop_assert!(
                after.valid,
                "{language} wrote a source that no longer lexes: {:?}",
                String::from_utf8_lossy(&result.output)
            );
            prop_assert_eq!(
                after.comments.len(), result.report.comments.len(),
                "{} changed how many comments there are: {:?}",
                language, String::from_utf8_lossy(&result.output)
            );
            for (before, now) in result.report.comments.iter().zip(&after.comments) {
                prop_assert_eq!(
                    before.kind, now.kind,
                    "{} changed a comment's kind: {:?}",
                    language, String::from_utf8_lossy(&result.output)
                );
            }
        }
    }

        /// A reflow may change how many comments there are.
    /// It may not change what any of them is.
    ///
    /// A kind is read from a comment's bytes and, for two of them, from where the comment sits.
    /// A reflow moves what sits where, so it can turn a remark into something a toolchain reads without touching a byte of it: joining two lines above a Python encoding declaration carries that declaration up into the first two lines, which is where it starts meaning something.
    /// This property is what found that.
    #[test]
    fn a_reflowed_source_is_still_the_same_comments(source in lexical_source(0..48)) {
        for language in [Language::Rust, Language::Python, Language::Html, Language::Ocaml] {
            let options = style_only(every_style_rule());
            let result = transform(&source, language, options.clone());
            if !result.report.valid {
                continue;
            }
            let after = scan(&result.output, language, options.scan.clone());
            prop_assert!(
                after.valid,
                "{} wrote a source that no longer lexes: {:?}",
                language, String::from_utf8_lossy(&result.output)
            );
            let before: Vec<_> =
                result.report.comments.iter().map(|comment| comment.kind).collect();
            for comment in &after.comments {
                prop_assert!(
                    before.contains(&comment.kind),
                    "{} invented a {} comment: {:?}",
                    language, comment.kind, String::from_utf8_lossy(&result.output)
                );
            }
        }
    }

        /// A rewrite moves white space and nothing else.
    ///
    /// The last wall between a formatter and the accusation that it ate somebody's sentence.
    /// Every rule this axis holds today is about spacing,
    /// and a rule that is not would have to be exempted here deliberately rather than by this test quietly not covering it.
    #[test]
    fn a_rewrite_moves_only_white_space(source in lexical_source(0..48)) {
        for language in [Language::Rust, Language::Python, Language::Html, Language::Ocaml] {
            let options = style_only(every_style_rule());
            let result = transform(&source, language, options);
            if !result.report.valid {
                continue;
            }
            for comment in &result.report.comments {
                let Some(replacement) = comment.disposition().replacement() else {
                    continue;
                };
                let raw = &source[comment.span.start..comment.span.end];
                prop_assert_eq!(
                    without_spacing(raw), without_spacing(replacement),
                    "{} changed the words of {:?}", language, String::from_utf8_lossy(raw)
                );
            }
        }
    }
}
