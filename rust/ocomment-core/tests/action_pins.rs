//! Pinned action annotations need their uses-line context.

use ocomment_core::{
    Action, ByteSpan, CommentKind, DispositionExplanation, DocumentChange, IncrementalDocument,
    Language, Policy, ScanOptions, explain_comment, scan,
};

const SHA: &str = "3d3c42e5aac5ba805825da76410c181273ba90b1";
const SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn pinned_references_are_directives_only_beside_their_action() {
    for reference in ["v7.0.1", "v2", "master", "release/v2", "prefix-v1.2.3"] {
        let source =
            format!("steps:\n  - uses: actions/checkout@{SHA} # {reference}\n# {reference}\n");
        let report = scan(source.as_bytes(), Language::Yaml, ScanOptions::default());
        assert!(report.valid, "{reference}: {:?}", report.diagnostics);
        assert_eq!(report.comments.len(), 2);
        assert_eq!(report.comments[0].kind, CommentKind::Directive);
        assert_eq!(report.comments[0].action(), Action::Keep);
        assert_eq!(report.comments[1].kind, CommentKind::Line);
        assert_eq!(report.comments[1].action(), Action::Remove);
        let comment = &report.comments[0];
        let explanation = explain_comment(
            comment,
            &source.as_bytes()[comment.span.start..comment.span.end],
            Language::Yaml,
            &ScanOptions::default(),
        );
        assert!(matches!(
            explanation,
            DispositionExplanation::KeptDirective { .. }
        ));
        assert_eq!(explanation.action(), comment.action());
    }
}

#[test]
fn quoted_values_keys_and_full_digest_widths_keep_the_annotation() {
    for line in [
        format!("  - uses: 'owner/repo/subpath@{SHA}' # v2"),
        format!("  uses: \"owner/repo@{SHA}\" # master"),
        format!("  'uses': owner/repo@{SHA} # release/v2"),
        format!("  \"uses\" : owner/repo@{SHA256} # v2"),
        format!("  - uses:\towner/repo@{}\t# v2", SHA.to_ascii_uppercase()),
    ] {
        let report = scan(line.as_bytes(), Language::Yaml, ScanOptions::default());
        assert!(report.valid, "{line}: {:?}", report.diagnostics);
        assert_eq!(report.comments.len(), 1, "{line}");
        assert_eq!(report.comments[0].kind, CommentKind::Directive, "{line}");
        assert_eq!(report.comments[0].action(), Action::Keep, "{line}");
    }
}

#[test]
fn version_metadata_can_keep_its_trailing_explanation() {
    for annotation in [
        "v2.1.0 because the API is stable",
        "prefix/v2.1.0 because the API is stable",
        "pin v2.1.0",
        "tag=v2.1.0",
        "tag = v2.1.0 because the API is stable",
    ] {
        let source = format!("  - uses: owner/repo@{SHA} # {annotation}\n");
        let report = scan(source.as_bytes(), Language::Yaml, ScanOptions::default());
        assert!(report.valid, "{annotation}");
        assert_eq!(
            report.comments[0].kind,
            CommentKind::Directive,
            "{annotation}"
        );
        assert_eq!(report.comments[0].action(), Action::Keep, "{annotation}");
    }
}

#[test]
fn similar_yaml_prose_and_unpinned_values_remain_removable() {
    for line in [
        "# v7.0.1".to_owned(),
        format!("  - run: echo owner/repo@{SHA} # v2"),
        "  - uses: owner/repo@v2 # v2".to_owned(),
        "  - uses: owner/repo@abc1234 # v2".to_owned(),
        format!("  - uses: owner/repo@{}z # v2", &SHA[..SHA.len() - 1]),
        format!("  - uses: ./local@{SHA} # v2"),
        format!("  - uses: docker://image@{SHA} # v2"),
        format!("  - uses: owner/repo@{SHA} # an ordinary explanation"),
        format!("  - uses: owner/repo@{SHA} # main branch"),
        format!("  - uses: owner/repo@{SHA} #\x0bv2"),
        format!("\x0buses: owner/repo@{SHA} # v2"),
        format!("  - uses: owner/repo@{SHA} # invalid..ref"),
        format!("  - uses: owner/repo@{SHA} # branch.lock"),
        format!("  - uses: owner/repo@{SHA} #"),
    ] {
        let report = scan(line.as_bytes(), Language::Yaml, ScanOptions::default());
        assert!(report.valid, "{line}: {:?}", report.diagnostics);
        assert_eq!(report.comments.len(), 1, "{line}");
        assert_eq!(report.comments[0].kind, CommentKind::Line, "{line}");
        assert_eq!(report.comments[0].action(), Action::Remove, "{line}");
    }
}

#[test]
fn bare_reference_annotations_agree_with_git() {
    for reference in [
        "v1",
        "main",
        "release/v2",
        "prefix-v1.2.3",
        "branch.LOCK",
        "",
        "@",
        ".hidden",
        "branch.lock",
        "a/.hidden",
        "a.lock/b",
        "a//b",
        "/branch",
        "branch/",
        "branch.",
        "a..b",
        "a@{b",
        "main branch",
        "main\tbranch",
        "a~b",
        "a^b",
        "a:b",
        "a?b",
        "a*b",
        "a[b",
        "a\\b",
    ] {
        let status = std::process::Command::new("git")
            .args(["check-ref-format", "--allow-onelevel", reference])
            .status()
            .expect("Git must check the independent reference grammar");
        assert!(
            matches!(status.code(), Some(0 | 1)),
            "{reference}: {status}"
        );
        let source = format!("  uses: owner/repo@{SHA} # {reference}\n");
        let report = scan(source.as_bytes(), Language::Yaml, ScanOptions::default());
        assert_eq!(
            report.comments[0].kind == CommentKind::Directive,
            status.success(),
            "{reference}"
        );
    }
}

#[test]
fn unfinished_uses_values_keep_the_yaml_diagnostic() {
    for quote in ['\'', '"'] {
        let source = format!("  uses: {quote}owner/repo@{SHA} # v2\n");
        let report = scan(source.as_bytes(), Language::Yaml, ScanOptions::default());
        assert!(!report.valid);
        assert!(report.comments.is_empty());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "unterminated-string")
        );
    }
}

#[test]
fn all_policy_still_controls_tool_directives() {
    let source = format!("  - uses: owner/repo@{SHA} # v2\n");
    for policy in [Policy::Conservative, Policy::Standard, Policy::All] {
        let options = ScanOptions {
            policy,
            ..Default::default()
        };
        let report = scan(source.as_bytes(), Language::Yaml, options);
        assert_eq!(report.comments[0].kind, CommentKind::Directive);
        assert_eq!(
            report.comments[0].action(),
            if policy == Policy::All {
                Action::Remove
            } else {
                Action::Keep
            }
        );
    }
}

#[test]
fn incremental_pin_edits_reclassify_the_annotation() {
    let source = format!("  - uses: owner/repo@{SHA} # v2\n# ordinary note\n");
    let options = ScanOptions::default();
    let mut document = IncrementalDocument::new(
        source.as_bytes().to_vec(),
        Language::Yaml,
        options.clone(),
        1,
    );
    assert_eq!(document.report().comments[0].kind, CommentKind::Directive);
    let position = source.find(SHA).unwrap();
    for (version, byte, expected) in [
        (2, b'z', CommentKind::Line),
        (3, b'3', CommentKind::Directive),
    ] {
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(position, position + 1),
                    replacement: vec![byte],
                }],
                version,
            )
            .unwrap();
        assert_eq!(document.report().comments[0].kind, expected);
        assert_eq!(
            document.report(),
            &scan(document.source(), Language::Yaml, options.clone())
        );
    }
}
