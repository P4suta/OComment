//! A keep pattern protects one contiguous block without crossing its boundaries.

use ocomment_core::{
    Action, AllowRules, ByteSpan, DocumentChange, IncrementalDocument, Language, Policy,
    ScanOptions, explain_comment, scan,
};

#[test]
fn keep_regex_protects_every_line_when_the_first_middle_or_last_line_matches() {
    let source = b"// first\n// middle\n// last\n";
    for policy in Policy::ALL {
        for pattern in ["first", "middle", "last"] {
            let options = ScanOptions {
                policy,
                keep_regex: vec![pattern.into()],
                remove_regex: vec![".*".into()],
                ..Default::default()
            };
            let report = scan(source, Language::Rust, options.clone());
            assert!(report.valid);
            assert_eq!(report.comments.len(), 3);
            for comment in &report.comments {
                assert_eq!(comment.action(), Action::Keep, "{policy:?}: {pattern}");
                let why = explain_comment(
                    comment,
                    &source[comment.span.start..comment.span.end],
                    Language::Rust,
                    &options,
                );
                assert_eq!(why.action(), Action::Keep, "{policy:?}: {pattern}: {why}");
            }
        }
    }
}

#[test]
fn keep_regex_does_not_cross_blank_lines_code_or_a_different_comment_kind() {
    for source in [
        &b"// KEEP\n\n// other\n"[..],
        b"// KEEP\r\n\r\n// other\r\n",
        b"// KEEP\r\r// other\r",
        b"// KEEP\nlet value = 1;\n// other\n",
        b"// KEEP\n/* other */\n// other\n",
        b"// KEEP\n/// other\n// other\n",
    ] {
        let options = ScanOptions {
            policy: Policy::All,
            keep_regex: vec!["KEEP".into()],
            ..Default::default()
        };
        let report = scan(source, Language::Rust, options);
        assert!(report.valid);
        let actions: Vec<_> = report
            .comments
            .iter()
            .map(|comment| comment.action())
            .collect();
        let mut expected = vec![Action::Remove; report.comments.len()];
        expected[0] = Action::Keep;
        assert_eq!(actions, expected, "{}", String::from_utf8_lossy(source));
    }
}

#[test]
fn a_named_block_survives_length_and_trailing_shape_rules() {
    let source = b"let value = 1; // KEEP\n// continuation\n// final\n";
    let options = ScanOptions {
        policy: Policy::All,
        keep_regex: vec!["KEEP".into()],
        allow: AllowRules {
            max_lines: Some(1),
            trailing: Some(false),
            ..Default::default()
        },
        ..Default::default()
    };
    let report = scan(source, Language::Rust, options);
    assert!(report.valid);
    assert_eq!(report.comments.len(), 3);
    for comment in report.comments {
        assert_eq!(comment.action(), Action::Keep);
    }
}

#[test]
fn incremental_edits_add_and_remove_the_whole_block_keep() {
    let source = b"// first\n// token\n// last\nlet value = 1;\n";
    let options = ScanOptions {
        policy: Policy::All,
        keep_regex: vec!["KEEP".into()],
        ..Default::default()
    };
    let mut document =
        IncrementalDocument::new(source.to_vec(), Language::Rust, options.clone(), 1);
    let start = source
        .windows(b"token".len())
        .position(|part| part == b"token")
        .unwrap();
    document
        .apply_changes(
            &[DocumentChange {
                span: ByteSpan::new(start, start + b"token".len()),
                replacement: b"KEEP".to_vec(),
            }],
            2,
        )
        .unwrap();
    let full = scan(document.source(), Language::Rust, options.clone());
    assert_eq!(document.report().comments, full.comments);
    assert!(
        document
            .report()
            .comments
            .iter()
            .all(|comment| comment.action() == Action::Keep)
    );
    document
        .apply_changes(
            &[DocumentChange {
                span: ByteSpan::new(start, start + b"KEEP".len()),
                replacement: b"token".to_vec(),
            }],
            3,
        )
        .unwrap();
    let full = scan(document.source(), Language::Rust, options);
    assert_eq!(document.source(), source);
    assert_eq!(document.report().comments, full.comments);
    assert!(
        document
            .report()
            .comments
            .iter()
            .all(|comment| comment.action() == Action::Remove)
    );
}

#[test]
fn incremental_edits_split_and_rejoin_kept_blocks() {
    fn change(
        document: &mut IncrementalDocument,
        options: &ScanOptions,
        span: ByteSpan,
        replacement: &[u8],
        version: i64,
        expected: &[Action],
    ) {
        document
            .apply_changes(
                &[DocumentChange {
                    span,
                    replacement: replacement.to_vec(),
                }],
                version,
            )
            .unwrap();
        let full = scan(document.source(), Language::Rust, options.clone());
        assert_eq!(document.report().comments, full.comments);
        let actions: Vec<_> = full
            .comments
            .iter()
            .map(|comment| comment.action())
            .collect();
        assert_eq!(actions, expected);
    }

    let source = b"// KEEP\n// middle\n// last\n";
    let options = ScanOptions {
        policy: Policy::All,
        keep_regex: vec!["KEEP".into()],
        ..Default::default()
    };
    let mut document =
        IncrementalDocument::new(source.to_vec(), Language::Rust, options.clone(), 1);
    let boundary = b"// KEEP\n".len();
    let separate = [Action::Keep, Action::Remove, Action::Remove];
    let joined = [Action::Keep; 3];
    change(
        &mut document,
        &options,
        ByteSpan::new(boundary, boundary),
        b"\n",
        2,
        &separate,
    );
    change(
        &mut document,
        &options,
        ByteSpan::new(boundary, boundary + 1),
        b"",
        3,
        &joined,
    );
    let code = b"let value = 1;\n";
    change(
        &mut document,
        &options,
        ByteSpan::new(boundary, boundary),
        code,
        4,
        &separate,
    );
    change(
        &mut document,
        &options,
        ByteSpan::new(boundary, boundary + code.len()),
        b"",
        5,
        &joined,
    );
    change(
        &mut document,
        &options,
        ByteSpan::new(boundary, boundary + 2),
        b"///",
        6,
        &separate,
    );
    change(
        &mut document,
        &options,
        ByteSpan::new(boundary, boundary + 3),
        b"//",
        7,
        &joined,
    );
    assert_eq!(document.source(), source);
}

#[test]
fn a_block_keep_precedes_the_yaml_structural_fallback() {
    let source = b"k: |\n  a\n# ends the block\n  # KEEPME\nz: 1\n";
    let options = ScanOptions {
        policy: Policy::All,
        keep_regex: vec!["KEEPME".into()],
        ..Default::default()
    };
    let report = scan(source, Language::Yaml, options.clone());
    assert!(report.valid);
    let comment = &report.comments[0];
    let why = explain_comment(
        comment,
        &source[comment.span.start..comment.span.end],
        Language::Yaml,
        &options,
    );
    assert!(matches!(
        why,
        ocomment_core::DispositionExplanation::KeptByRegex { index: 0, .. }
    ));
    assert!(
        report
            .comments
            .iter()
            .all(|comment| comment.action() == Action::Keep)
    );
}

#[test]
fn style_rules_still_rewrite_every_kept_member_and_explain_the_rewrite() {
    let source = b"//KEEP\n//continuation\n";
    let options = ScanOptions {
        policy: Policy::All,
        keep_regex: vec!["KEEP".into()],
        style: ocomment_core::StyleRules {
            space_after_marker: Some(true),
            ..Default::default()
        },
        ..Default::default()
    };
    let report = scan(source, Language::Rust, options.clone());
    assert!(report.valid);
    for comment in &report.comments {
        assert_eq!(comment.action(), Action::Rewrite);
        let why = explain_comment(
            comment,
            &source[comment.span.start..comment.span.end],
            Language::Rust,
            &options,
        );
        assert_eq!(why.action(), Action::Rewrite);
    }
}
