//! TSX type parameters and JSX keep separate lexical goals.

use ocomment_core::{
    Action, ByteSpan, Dialect, DocumentChange, IncrementalDocument, Language, ScanOptions, scan,
};

fn options(dialect: Dialect) -> ScanOptions {
    ScanOptions {
        dialect,
        ..Default::default()
    }
}

#[test]
fn generic_arrow_forms_leave_adjacent_jsx_and_comments_visible() {
    for head in ["<T extends U>", "<T,>", "<T = U>", "<const T extends U>"] {
        let source = format!(
            "// report this\ntype U = string;\nconst pick = {head}(value: T): T => value;\nconst view = <div>{{pick(\"a\") /* inside */}}</div>; // after\n"
        );
        let report = scan(
            source.as_bytes(),
            Language::TypeScript,
            options(Dialect::Tsx),
        );
        assert!(report.valid, "{head}: {:?}", report.diagnostics);
        let comments: Vec<_> = report
            .comments
            .iter()
            .map(|comment| {
                assert_eq!(comment.action(), Action::Remove);
                &source.as_bytes()[comment.span.start..comment.span.end]
            })
            .collect();
        assert_eq!(
            comments,
            [b"// report this".as_slice(), b"/* inside */", b"// after"]
        );

        let plain =
            format!("type U = string;\nconst pick = {head}(value: T): T => value; // after\n");
        let report = scan(
            plain.as_bytes(),
            Language::TypeScript,
            options(Dialect::Standard),
        );
        assert!(report.valid);
        assert_eq!(report.comments.len(), 1);
    }
}

#[test]
fn type_parameter_trivia_and_nested_types_do_not_hide_comments() {
    for ending in ["\n", "\r\n"] {
        for head in [
            "<T /* type */ extends /* bound */ U>",
            "<T\u{2003}extends\u{a0}U>",
            "<T extends Record<string, unknown> = {}>",
            "<T, U>",
        ] {
            let source = format!(
                "type U = string;{ending}const pick = {head}(value: T): T => value; // after{ending}const view = <div>{{1 /* inside */}}</div>;{ending}"
            );
            let report = scan(
                source.as_bytes(),
                Language::TypeScript,
                options(Dialect::Tsx),
            );
            assert!(report.valid, "{head}: {:?}", report.diagnostics);
            assert!(report.comments.iter().any(|comment| {
                &source.as_bytes()[comment.span.start..comment.span.end] == b"// after"
            }));
            assert!(report.comments.iter().any(|comment| {
                &source.as_bytes()[comment.span.start..comment.span.end] == b"/* inside */"
            }));
        }
    }
}

#[test]
fn jsx_attributes_named_extends_keep_their_lexical_goal() {
    for element in [
        "<View extends />",
        "<View extends>text {1 /* inside */}</View>",
        "<View extends=\"value\">text {1 /* inside */}</View>",
        "<View extends={value}>text {1 /* inside */}</View>",
    ] {
        let source = format!("const view = {element}; // after\n");
        let report = scan(
            source.as_bytes(),
            Language::TypeScript,
            options(Dialect::Tsx),
        );
        assert!(report.valid, "{element}: {:?}", report.diagnostics);
        assert_eq!(
            &source.as_bytes()[report.comments.last().unwrap().span.start
                ..report.comments.last().unwrap().span.end],
            b"// after"
        );
        assert_eq!(
            report.comments.len(),
            if element.contains("/* inside */") {
                2
            } else {
                1
            }
        );
    }
}

#[test]
fn broken_jsx_keeps_its_existing_diagnostics() {
    for source in [
        b"const view = <View>;".as_slice(),
        b"const view = <View extends>;",
        b"const view = <View extends=\"value\";",
        b"const view = <class,>;",
    ] {
        let report = scan(source, Language::TypeScript, options(Dialect::Tsx));
        assert!(!report.valid);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "unterminated-jsx-element"
                || diagnostic.code == "unterminated-jsx-tag"
        }));
    }
}

#[test]
fn javascript_jsx_does_not_accept_typescript_type_parameters() {
    let source = b"const pick = <T extends string>(value: T) => value;";
    let report = scan(source, Language::JavaScript, options(Dialect::Jsx));
    assert!(!report.valid);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "unterminated-jsx-element")
    );
}

#[test]
fn incremental_generic_edits_match_a_fresh_scan() {
    let source = b"const pick = <T,>(value: T) => value; // after\n";
    let options = options(Dialect::Tsx);
    let mut document =
        IncrementalDocument::new(source.to_vec(), Language::TypeScript, options.clone(), 1);
    assert!(document.report().valid);
    let comma = source.windows(4).position(|part| part == b"<T,>").unwrap() + 2;
    document
        .apply_changes(
            &[DocumentChange {
                span: ByteSpan::new(comma, comma + 1),
                replacement: vec![],
            }],
            2,
        )
        .unwrap();
    assert_eq!(
        document.report(),
        &scan(document.source(), Language::TypeScript, options.clone())
    );
    assert!(!document.report().valid);
    document
        .apply_changes(
            &[DocumentChange {
                span: ByteSpan::new(comma, comma),
                replacement: b",".to_vec(),
            }],
            3,
        )
        .unwrap();
    assert_eq!(
        document.report(),
        &scan(document.source(), Language::TypeScript, options)
    );
    assert!(document.report().valid);
    assert_eq!(document.report().comments.len(), 1);
}

#[test]
fn escaped_type_parameter_names_keep_their_lexical_goal() {
    for (head, name) in [
        ("<T\\u0065 extends string>", "Te"),
        ("<T\\u{65},>", "Te"),
        ("<T\\u{000000065},>", "Te"),
        ("<T\\u0065 = string>", "Te"),
        ("<\\u0054e extends string>", "Te"),
        ("<Té,>", "Té"),
    ] {
        let source = format!(
            "const pick = {head}(value: {name}): {name} => value; // after\nconst view = <div>{{pick(\"a\") /* inside */}}</div>;\n"
        );
        let report = scan(
            source.as_bytes(),
            Language::TypeScript,
            options(Dialect::Tsx),
        );
        assert!(report.valid, "{head}: {:?}", report.diagnostics);
        assert_eq!(report.comments.len(), 2);
    }
}
