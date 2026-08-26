//! Stable-name contract for the public enums.
//!
//! `as_str` is the single source of truth for every user-visible spelling: it
//! must equal the serde name byte-for-byte, round-trip through `FromStr`, and
//! agree with `Display`. Every historical alias is pinned here so a refactor
//! cannot silently drop one.

use ocomment_core::{
    CommentKind, Dialect, Disposition, Language, Layout, Policy, ScanOptions, Severity, scan,
};
use std::{collections::BTreeSet, str::FromStr};

fn serde_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("enum serializes")
        .as_str()
        .expect("enum serializes as a string")
        .to_owned()
}

/// Every variant of `$type` agrees with serde, `FromStr`, and `Display`, and no
/// spelling is claimed by two variants.
macro_rules! check_stable_names {
    ($type:ident) => {{
        let mut seen = BTreeSet::new();
        for value in $type::ALL {
            assert_eq!(
                serde_name(&value),
                value.as_str(),
                "{}::{value:?} serde name differs from as_str",
                stringify!($type)
            );
            assert_eq!(
                $type::from_str(value.as_str()),
                Ok(value),
                "{}::{value:?} canonical name does not round-trip",
                stringify!($type)
            );
            assert_eq!(
                value.to_string(),
                value.as_str(),
                "{}::{value:?} Display differs from as_str",
                stringify!($type)
            );
            assert!(
                seen.insert(value.as_str()),
                "{}::{value:?} name `{}` is claimed twice",
                stringify!($type),
                value.as_str()
            );
            for alias in value.aliases() {
                assert_eq!(
                    $type::from_str(alias),
                    Ok(value),
                    "{}::{value:?} alias `{alias}` does not parse",
                    stringify!($type)
                );
                assert!(
                    seen.insert(alias),
                    "{}::{value:?} alias `{alias}` is claimed twice",
                    stringify!($type)
                );
            }
        }
        seen
    }};
}

#[test]
fn language_names_are_stable() {
    let seen = check_stable_names!(Language);
    assert_eq!(Language::ALL.len(), 20);
    assert!(
        !seen.contains("unknown"),
        "Unknown must stay out of the parseable set"
    );
    assert_eq!(serde_name(&Language::Unknown), "unknown");
    assert_eq!(Language::Unknown.as_str(), "unknown");
    assert_eq!(Language::Unknown.to_string(), "unknown");
    assert!(!Language::ALL.contains(&Language::Unknown));
}

#[test]
fn dialect_names_are_stable() {
    check_stable_names!(Dialect);
    assert_eq!(Dialect::ALL.len(), 16);
}

#[test]
fn comment_kind_names_are_stable() {
    check_stable_names!(CommentKind);
    assert_eq!(CommentKind::ALL.len(), 11);
}

#[test]
fn policy_names_are_stable() {
    check_stable_names!(Policy);
    assert_eq!(Policy::ALL.len(), 3);
}

#[test]
fn layout_names_are_stable() {
    check_stable_names!(Layout);
    assert_eq!(Layout::ALL.len(), 3);
}

#[test]
fn severity_names_are_stable() {
    check_stable_names!(Severity);
    assert_eq!(Severity::ALL.len(), 4);
}

#[test]
fn language_aliases_are_pinned() {
    let cases = [
        ("rust", Language::Rust),
        ("rs", Language::Rust),
        ("ocaml", Language::Ocaml),
        ("ml", Language::Ocaml),
        ("c", Language::C),
        ("cpp", Language::Cpp),
        ("c++", Language::Cpp),
        ("cxx", Language::Cpp),
        ("go", Language::Go),
        ("golang", Language::Go),
        ("java", Language::Java),
        ("javascript", Language::JavaScript),
        ("js", Language::JavaScript),
        ("jsx", Language::JavaScript),
        ("ecmascript", Language::JavaScript),
        ("typescript", Language::TypeScript),
        ("ts", Language::TypeScript),
        ("tsx", Language::TypeScript),
        ("python", Language::Python),
        ("py", Language::Python),
        ("shell", Language::Shell),
        ("sh", Language::Shell),
        ("bash", Language::Shell),
        ("zsh", Language::Shell),
        ("html", Language::Html),
        ("htm", Language::Html),
        ("css", Language::Css),
        ("jsonc", Language::Jsonc),
        ("json5", Language::Jsonc),
        ("sql", Language::Sql),
        ("kotlin", Language::Kotlin),
        ("kt", Language::Kotlin),
        ("kts", Language::Kotlin),
        ("toml", Language::Toml),
        ("lua", Language::Lua),
        ("yaml", Language::Yaml),
        ("yml", Language::Yaml),
        ("php", Language::Php),
    ];
    for (text, expected) in cases {
        assert_eq!(Language::from_str(text), Ok(expected), "`{text}`");
    }
}

#[test]
fn language_parsing_ignores_case_dashes_and_underscores() {
    for text in ["RUST", "Rust", "-r-u-s-t-", "r_u_s_t"] {
        assert_eq!(Language::from_str(text), Ok(Language::Rust), "`{text}`");
    }
    assert_eq!(
        Language::from_str("Java_Script"),
        Ok(Language::JavaScript),
        "underscores are stripped"
    );
    assert_eq!(Language::from_str("C++"), Ok(Language::Cpp));
}

#[test]
fn dialect_aliases_are_pinned() {
    let cases = [
        ("standard", Dialect::Standard),
        ("jsx", Dialect::Jsx),
        ("tsx", Dialect::Tsx),
        ("objective-c", Dialect::ObjectiveC),
        ("objc", Dialect::ObjectiveC),
        ("objective-cpp", Dialect::ObjectiveCpp),
        ("objective-c++", Dialect::ObjectiveCpp),
        ("objcpp", Dialect::ObjectiveCpp),
        ("gnu-c", Dialect::GnuC),
        ("gnuc", Dialect::GnuC),
        ("gnu-cpp", Dialect::GnuCpp),
        ("gnu-c++", Dialect::GnuCpp),
        ("gnucpp", Dialect::GnuCpp),
        ("cuda", Dialect::Cuda),
        ("posix-sh", Dialect::PosixSh),
        ("posix", Dialect::PosixSh),
        ("sh", Dialect::PosixSh),
        ("bash53", Dialect::Bash53),
        ("bash-5.3", Dialect::Bash53),
        ("bash", Dialect::Bash53),
        ("zsh", Dialect::Zsh),
        ("postgresql", Dialect::PostgreSql),
        ("postgres", Dialect::PostgreSql),
        ("pgsql", Dialect::PostgreSql),
        ("mysql", Dialect::MySql),
        ("sqlite", Dialect::Sqlite),
        ("t-sql", Dialect::TSql),
        ("tsql", Dialect::TSql),
        ("oracle", Dialect::Oracle),
    ];
    for (text, expected) in cases {
        assert_eq!(Dialect::from_str(text), Ok(expected), "`{text}`");
    }
}

#[test]
fn dialect_parsing_folds_case_and_underscores() {
    assert_eq!(Dialect::from_str("Objective_C"), Ok(Dialect::ObjectiveC));
    assert_eq!(Dialect::from_str("GNU-CPP"), Ok(Dialect::GnuCpp));
    assert_eq!(Dialect::from_str("bash_5.3"), Ok(Dialect::Bash53));
    assert_eq!(Dialect::from_str("T_SQL"), Ok(Dialect::TSql));
}

#[test]
fn comment_kind_aliases_are_pinned() {
    let cases = [
        ("line", CommentKind::Line),
        ("block", CommentKind::Block),
        ("doc-line", CommentKind::DocLine),
        ("doc", CommentKind::DocLine),
        ("doc-block", CommentKind::DocBlock),
        ("directive", CommentKind::Directive),
        ("pragma", CommentKind::Directive),
        ("license", CommentKind::License),
        ("legal", CommentKind::License),
        ("html", CommentKind::HtmlComment),
        ("html-comment", CommentKind::HtmlComment),
        ("shebang", CommentKind::Shebang),
        ("encoding", CommentKind::Encoding),
        ("optimizer-hint", CommentKind::OptimizerHint),
        ("version-comment", CommentKind::VersionComment),
    ];
    for (text, expected) in cases {
        assert_eq!(CommentKind::from_str(text), Ok(expected), "`{text}`");
    }
}

#[test]
fn comment_kind_parsing_folds_case_and_underscores() {
    assert_eq!(
        CommentKind::from_str("DOC_BLOCK"),
        Ok(CommentKind::DocBlock)
    );
    assert_eq!(
        CommentKind::from_str("Optimizer_Hint"),
        Ok(CommentKind::OptimizerHint)
    );
    assert_eq!(
        CommentKind::from_str("HTML_COMMENT"),
        Ok(CommentKind::HtmlComment)
    );
}

#[test]
fn policy_and_layout_aliases_are_pinned() {
    assert_eq!(Policy::from_str("safe"), Ok(Policy::Safe));
    assert_eq!(Policy::from_str("legal"), Ok(Policy::Legal));
    assert_eq!(Policy::from_str("all"), Ok(Policy::All));
    assert_eq!(Policy::from_str("SAFE"), Ok(Policy::Safe));
    assert_eq!(Layout::from_str("lines"), Ok(Layout::Lines));
    assert_eq!(Layout::from_str("columns"), Ok(Layout::Columns));
    assert_eq!(Layout::from_str("compact"), Ok(Layout::Compact));
    assert_eq!(Layout::from_str("Compact"), Ok(Layout::Compact));
    assert!(Policy::ALL.iter().all(|value| value.aliases().is_empty()));
    assert!(Layout::ALL.iter().all(|value| value.aliases().is_empty()));
}

#[test]
fn rejection_messages_are_unchanged() {
    assert_eq!(
        Language::from_str("unknown"),
        Err("unsupported language `unknown`".to_owned())
    );
    assert_eq!(
        Language::from_str("Klingon"),
        Err("unsupported language `Klingon`".to_owned())
    );
    assert_eq!(
        Dialect::from_str("mariadb"),
        Err("unknown dialect `mariadb`".to_owned())
    );
    assert_eq!(
        CommentKind::from_str("footnote"),
        Err("unknown comment kind `footnote`".to_owned())
    );
    assert_eq!(
        Policy::from_str("paranoid"),
        Err("unknown policy `paranoid`".to_owned())
    );
    assert_eq!(
        Layout::from_str("grid"),
        Err("unknown layout `grid`".to_owned())
    );
}

#[test]
fn disposition_display_is_human_readable() {
    assert_eq!(Disposition::Remove.to_string(), "remove");
    assert_eq!(
        Disposition::Keep {
            reason: "legal policy".to_owned()
        }
        .to_string(),
        "keep (legal policy)"
    );
}

#[test]
fn disposition_serde_shape_is_frozen() {
    assert_eq!(
        serde_json::to_value(Disposition::Remove).unwrap(),
        serde_json::json!({"action": "remove"})
    );
    assert_eq!(
        serde_json::to_value(Disposition::Keep {
            reason: "legal policy".to_owned()
        })
        .unwrap(),
        serde_json::json!({"action": "keep", "reason": "legal policy"})
    );
}

/// The differential protocol freezes these six strings; the OCaml reference
/// compares them byte-for-byte.
const KEEP_REASONS: [&str; 6] = [
    "kept by kind or regex override",
    "required source preamble",
    "HTML comments are DOM-observable",
    "tool or language directive",
    "legal policy",
    "structural in a YAML block scalar trail",
];

/// One fixture for `keep_reasons_are_observable_through_scan`: a source, how it
/// is scanned, how many comments it holds, and which of them carries the frozen
/// reason under test. The count is pinned per fixture so a scanner that started
/// finding a comment more or fewer fails here rather than sliding the index.
struct ReasonFixture {
    source: &'static [u8],
    language: Language,
    options: ScanOptions,
    comments: usize,
    index: usize,
    reason: &'static str,
}

#[test]
fn keep_reasons_are_observable_through_scan() {
    let cases = [
        ReasonFixture {
            source: b"// keep me\n",
            language: Language::Rust,
            options: ScanOptions {
                keep_kinds: vec![CommentKind::Line],
                ..Default::default()
            },
            comments: 1,
            index: 0,
            reason: "kept by kind or regex override",
        },
        ReasonFixture {
            source: b"#!/bin/sh\n",
            language: Language::Shell,
            options: ScanOptions::default(),
            comments: 1,
            index: 0,
            reason: "required source preamble",
        },
        ReasonFixture {
            source: b"<!-- note -->\n",
            language: Language::Html,
            options: ScanOptions::default(),
            comments: 1,
            index: 0,
            reason: "HTML comments are DOM-observable",
        },
        ReasonFixture {
            source: b"// rustfmt::skip\n",
            language: Language::Rust,
            options: ScanOptions::default(),
            comments: 1,
            index: 0,
            reason: "tool or language directive",
        },
        ReasonFixture {
            source: b"// Copyright 2026 Example\n",
            language: Language::Rust,
            options: ScanOptions {
                policy: Policy::Legal,
                ..Default::default()
            },
            comments: 1,
            index: 0,
            reason: "legal policy",
        },
        /* NOTE: The one reason that needs a second comment to exist at all: the
         * block scalar leans on the first comment only because the directive
         * below it survives and is indented into the body. */
        ReasonFixture {
            source: b"k: |\n  a\n# ends the block\n  # yamllint disable\nz: 1\n",
            language: Language::Yaml,
            options: ScanOptions::default(),
            comments: 2,
            index: 0,
            reason: "structural in a YAML block scalar trail",
        },
    ];
    let mut observed = BTreeSet::new();
    for case in cases {
        observed.insert(case.reason);
        let report = scan(case.source, case.language, case.options);
        assert_eq!(
            report.comments.len(),
            case.comments,
            "`{}` fixture found {:?}",
            case.reason,
            report.comments
        );
        assert_eq!(
            report.comments[case.index].disposition,
            Disposition::Keep {
                reason: case.reason.to_owned()
            },
            "`{}` fixture",
            case.reason
        );
        assert_eq!(
            report.comments[case.index].disposition.to_string(),
            format!("keep ({})", case.reason)
        );
    }
    assert_eq!(
        observed,
        BTreeSet::from(KEEP_REASONS),
        "the fixtures no longer exercise every frozen keep reason"
    );
}
