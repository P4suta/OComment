//! Biome suppressions remain tool directives through every scanner entry point.

use ocomment_core::{
    Action, ByteSpan, CommentKind, DeclarativeProfile, DocumentChange, IncrementalDocument,
    Language, LineDelimiter, Policy, ScanOptions, scan, scan_profile,
};

const MARKERS: [&str; 4] = [
    "biome-ignore",
    "biome-ignore-all",
    "biome-ignore-start",
    "biome-ignore-end",
];

#[test]
fn every_suppression_is_a_directive_in_each_builtin_biome_language() {
    for marker in MARKERS {
        for (language, source) in [
            (
                Language::JavaScript,
                format!("// {marker} lint: reason\nconst value = 1;\n"),
            ),
            (
                Language::TypeScript,
                format!(
                    "// {marker} lint/suspicious/noExplicitAny: reason\nconst value: any = 1;\n"
                ),
            ),
            (
                Language::Jsonc,
                format!("// {marker} lint: reason\n{{\"value\":1}}\n"),
            ),
            (
                Language::Css,
                format!("/* {marker} format: reason */\na {{ color: red; }}\n"),
            ),
        ] {
            for policy in [Policy::Conservative, Policy::Standard] {
                let report = scan(
                    source.as_bytes(),
                    language,
                    ScanOptions {
                        policy,
                        ..Default::default()
                    },
                );
                assert!(report.valid, "{language:?}: {marker}");
                assert_eq!(report.comments.len(), 1);
                assert_eq!(report.comments[0].kind, CommentKind::Directive);
                assert_eq!(report.comments[0].action(), Action::Keep);
            }
        }
    }
}

#[test]
fn a_suppression_can_follow_another_line_in_a_block_comment() {
    let source = b"/* rationale\n * biome-ignore-start lint: reason\n */\nlet value = 1;\n";
    let report = scan(source, Language::TypeScript, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(report.comments[0].kind, CommentKind::Directive);
    assert_eq!(report.comments[0].action(), Action::Keep);
}

#[test]
fn similar_words_and_mentions_stay_ordinary_comments() {
    for text in [
        "biome-ignoreish lint: prose",
        "biome-ignore-alligator lint: prose",
        "biome-ignore-started lint: prose",
        "biome-ignore-ending lint: prose",
        "biome-ignore-next lint: prose",
        "This mentions biome-ignore lint: prose",
        "biome ignores lint: prose",
    ] {
        let source = format!("// {text}\nconst value = 1;\n");
        let report = scan(
            source.as_bytes(),
            Language::TypeScript,
            ScanOptions::default(),
        );
        assert!(report.valid);
        assert_eq!(report.comments.len(), 1);
        assert_eq!(report.comments[0].kind, CommentKind::Line);
        let protected = report.comments[0].action() == Action::Keep;
        assert!(!protected, "ordinary prose must remain removable: {text}");
    }
}

#[test]
fn all_can_remove_a_biome_directive() {
    for marker in MARKERS {
        let source = format!("// {marker} lint: reason\n");
        let report = scan(
            source.as_bytes(),
            Language::TypeScript,
            ScanOptions {
                policy: Policy::All,
                ..Default::default()
            },
        );
        assert_eq!(report.comments.len(), 1);
        assert_eq!(report.comments[0].kind, CommentKind::Directive);
        assert_eq!(report.comments[0].action(), Action::Remove);
    }
}

#[test]
fn header_boundaries_and_block_decoration_preserve_their_meaning() {
    for separator in [" ", "\t", "\u{85}", "\u{a0}", "\u{2003}"] {
        let source = format!("// biome-ignore{separator}lint: reason\nconst value = 1;\n");
        let report = scan(
            source.as_bytes(),
            Language::TypeScript,
            ScanOptions::default(),
        );
        assert!(report.valid);
        assert_eq!(report.comments[0].kind, CommentKind::Directive);
    }
    for source in [
        "// biome-ignore\n",
        "// BIOME-IGNORE lint: reason\n",
        "/* rationale\n *\u{2003}biome-ignore\tlint: reason\n */\n",
    ] {
        let report = scan(
            source.as_bytes(),
            Language::TypeScript,
            ScanOptions::default(),
        );
        assert!(report.valid);
        assert_eq!(report.comments[0].kind, CommentKind::Directive);
    }
    let report = scan(
        b"// * biome-ignore lint: prose\n",
        Language::TypeScript,
        ScanOptions::default(),
    );
    assert_eq!(report.comments[0].kind, CommentKind::Line);
    assert_eq!(report.comments[0].action(), Action::Remove);
}

#[test]
fn a_graphql_profile_uses_the_same_tool_classification() {
    let profile = DeclarativeProfile {
        name: "graphql".into(),
        line_comments: vec![LineDelimiter {
            start: "#".into(),
            kind: CommentKind::Line,
            ..Default::default()
        }],
        ..Default::default()
    };
    for marker in MARKERS {
        let source =
            format!("# {marker} lint: reason\ntype Query {{ value: String }}\n# ordinary\n");
        let report = scan_profile(source.as_bytes(), &profile, ScanOptions::default()).unwrap();
        assert!(report.valid);
        assert_eq!(report.comments.len(), 2);
        assert_eq!(report.comments[0].kind, CommentKind::Directive);
        assert_eq!(report.comments[0].action(), Action::Keep);
        assert_eq!(report.comments[1].kind, CommentKind::Line);
        assert_eq!(report.comments[1].action(), Action::Remove);
    }
}

#[test]
fn incremental_edits_reclassify_a_suppression_and_a_near_miss() {
    let source = b"// biome-ignoreish lint: reason\nconst value = 1;\n";
    let options = ScanOptions::default();
    let mut document =
        IncrementalDocument::new(source.to_vec(), Language::TypeScript, options.clone(), 1);
    let start = b"// biome-ignore".len();
    document
        .apply_changes(
            &[DocumentChange {
                span: ByteSpan::new(start, start + 3),
                replacement: vec![],
            }],
            2,
        )
        .unwrap();
    assert_eq!(
        document.report(),
        &scan(document.source(), Language::TypeScript, options.clone())
    );
    assert_eq!(document.report().comments[0].kind, CommentKind::Directive);
    assert_eq!(document.report().comments[0].action(), Action::Keep);
    document
        .apply_changes(
            &[DocumentChange {
                span: ByteSpan::new(start, start),
                replacement: b"ish".to_vec(),
            }],
            3,
        )
        .unwrap();
    assert_eq!(
        document.report(),
        &scan(document.source(), Language::TypeScript, options)
    );
    assert_eq!(document.report().comments[0].kind, CommentKind::Line);
    assert_eq!(document.report().comments[0].action(), Action::Remove);
}
