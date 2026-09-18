//! Stable-name contract for the public enums.
//!
//! `as_str` is the single source of truth for every user-visible spelling: it
//! must equal the serde name byte-for-byte, round-trip through `FromStr`, and
//! agree with `Display`. Every historical alias is pinned here so a refactor
//! cannot silently drop one.

use ocomment_core::{
    CommentKind, Dialect, Disposition, Language, Layout, Policy, ScanOptions, Severity, scan,
};
use std::{
    collections::{BTreeSet, HashSet},
    str::FromStr,
};

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
    assert_eq!(Language::ALL.len(), 30);
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
    assert_eq!(Dialect::ALL.len(), 18);
}

#[test]
fn comment_kind_names_are_stable() {
    check_stable_names!(CommentKind);
    assert_eq!(CommentKind::ALL.len(), 12);
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

/// Every spelling [`Language::from_str`] accepts, written out rather than
/// generated, so that a rename shows up here as a changed line.
///
/// The table is also checked *against* [`Language::aliases`] below: a language
/// added without a row, or an alias added to that function and nowhere else,
/// fails here instead of shipping unpinned.
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
        ("ruby", Language::Ruby),
        ("rb", Language::Ruby),
        ("zig", Language::Zig),
        ("r", Language::R),
        ("rscript", Language::R),
        ("dart", Language::Dart),
        ("swift", Language::Swift),
        ("csharp", Language::CSharp),
        ("cs", Language::CSharp),
        ("c#", Language::CSharp),
        ("scala", Language::Scala),
        ("vue", Language::Vue),
        ("svelte", Language::Svelte),
        ("markdown", Language::Markdown),
        ("perl", Language::Perl),
    ];
    for (text, expected) in cases {
        assert_eq!(Language::from_str(text), Ok(expected), "`{text}`");
    }
    // NOTE: The other direction. `cases` is what pins the spellings, so every
    // NOTE: canonical name and every alias the crate publishes has to be one of
    // NOTE: its rows -- otherwise a new language, or a new alias for an old
    // NOTE: one, would be accepted by `from_str` with nothing holding it there.
    let pinned: HashSet<(&str, Language)> = cases.into_iter().collect();
    let mut missing = Vec::new();
    for language in Language::ALL {
        for spelling in std::iter::once(language.as_str()).chain(language.aliases().iter().copied())
        {
            if !pinned.contains(&(spelling, language)) {
                missing.push(format!("(\"{spelling}\", Language::{language:?})"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "these spellings are accepted by `Language::from_str` but pinned by no row \
         of this table: {missing:?}"
    );
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
        ("scss", Dialect::Scss),
        ("sass", Dialect::Sass),
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
        ("load-bearing", CommentKind::LoadBearing),
        ("load_bearing", CommentKind::LoadBearing),
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
    assert_eq!(Policy::from_str("safe"), Ok(Policy::Standard));
    assert_eq!(Policy::from_str("legal"), Ok(Policy::Conservative));
    assert_eq!(Policy::from_str("all"), Ok(Policy::All));
    assert_eq!(Policy::from_str("SAFE"), Ok(Policy::Standard));
    assert_eq!(Layout::from_str("lines"), Ok(Layout::Lines));
    assert_eq!(Layout::from_str("columns"), Ok(Layout::Columns));
    assert_eq!(Layout::from_str("compact"), Ok(Layout::Compact));
    assert_eq!(Layout::from_str("Compact"), Ok(Layout::Compact));
    /* NOTE: The policies carry their former spellings so that a configuration
     * or a command line written against the old names still resolves, and to
     * the same behaviour those names always had. Pinning them here is what
     * stops the compatibility from being dropped by accident. */
    assert_eq!(Policy::Conservative.aliases(), ["legal"]);
    assert_eq!(Policy::Standard.aliases(), ["safe"]);
    assert!(Policy::All.aliases().is_empty());
    assert_eq!(Policy::Conservative.former_name(), Some("legal"));
    assert_eq!(Policy::Standard.former_name(), Some("safe"));
    assert_eq!(Policy::All.former_name(), None);
    /* NOTE: The order of `ALL` is how much each policy takes, weakest first,
     * and help output reads it in that order. A reordering would make the
     * names stop describing a scale. */
    assert_eq!(
        Policy::ALL.map(Policy::as_str),
        ["conservative", "standard", "all"]
    );
    assert_eq!(Policy::default(), Policy::Conservative);
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
            reason: "conservative policy".to_owned()
        }
        .to_string(),
        "keep (conservative policy)"
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
            reason: "conservative policy".to_owned()
        })
        .unwrap(),
        serde_json::json!({"action": "keep", "reason": "conservative policy"})
    );
}

/// The differential protocol freezes these seven strings; the OCaml reference
/// compares them byte-for-byte.
const KEEP_REASONS: [&str; 7] = [
    "kept by keep_kind",
    "kept by keep_regex",
    "required source preamble",
    "HTML comments are DOM-observable",
    "tool or language directive",
    "conservative policy",
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
            reason: "kept by keep_kind",
        },
        /* NOTE: The companion of the fixture above. The two rules used to
         * share one reason, so one fixture covered both and neither was
         * actually observed on its own. */
        ReasonFixture {
            source: b"// keep me\n",
            language: Language::Rust,
            options: ScanOptions {
                keep_regex: vec!["keep me".into()],
                ..Default::default()
            },
            comments: 1,
            index: 0,
            reason: "kept by keep_regex",
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
                policy: Policy::Conservative,
                ..Default::default()
            },
            comments: 1,
            index: 0,
            reason: "conservative policy",
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

/// The policy table and the scanner agree, kind by kind and policy by policy.
///
/// `Policy::keeps` is the table the crate documentation prints and the
/// scanner decides by. It was prose in one place and a chain of `if`s in
/// another, and the CLI grew a third copy to answer "would a weaker policy
/// have kept this?" — which was wrong in the only case that occurs. One table
/// now, and this is what holds it to what a scan actually does.
#[test]
fn the_policy_table_is_what_a_scan_does() {
    for policy in Policy::ALL {
        for kind in CommentKind::ALL {
            /* NOTE: A protected kind is held back before the policy is asked,
             * so what `keeps` answers for it is what the policy would do with
             * the protection lifted -- which is what `force_protected` asks
             * for, and is the arrangement this compares against. */
            let options = ScanOptions {
                policy,
                force_protected: true,
                ..Default::default()
            };
            let (source, language) = match kind {
                CommentKind::Line => ("let x = 1; // plain\n", Language::Rust),
                CommentKind::Block => ("let x = 1; /* block */\n", Language::Rust),
                CommentKind::DocLine => ("/// doc\nfn f() {}\n", Language::Rust),
                CommentKind::DocBlock => ("/** doc */\nfn f() {}\n", Language::Rust),
                CommentKind::Directive => ("// rustfmt::skip\n", Language::Rust),
                CommentKind::License => ("// SPDX-License-Identifier: MIT\n", Language::Rust),
                CommentKind::HtmlComment => ("<!-- note -->\n", Language::Html),
                CommentKind::Shebang => ("#!/bin/sh\n", Language::Shell),
                CommentKind::Encoding => ("# -*- coding: utf-8 -*-\n", Language::Python),
                CommentKind::OptimizerHint => {
                    ("select /*+ index(t) */ 1 from dual;\n", Language::Sql)
                }
                CommentKind::VersionComment => ("/*!40101 SET NAMES utf8 */\n", Language::Sql),
                CommentKind::LoadBearing => ("//go:build linux\n", Language::Go),
            };
            let report = scan(source.as_bytes(), language, options);
            let Some(comment) = report.comments.iter().find(|comment| comment.kind == kind) else {
                panic!("no `{kind}` in the fixture for it: {source:?}");
            };
            assert_eq!(
                !comment.disposition.is_remove(),
                policy.keeps(kind),
                "policy {policy} and kind {kind}: the table and the scan disagree"
            );
        }
    }
}

/// Every kind states a protection, and the two tiers name themselves.
#[test]
fn every_kind_states_its_protection() {
    use ocomment_core::Protection;
    assert_eq!(Protection::None.reason(), None);
    assert_eq!(
        Protection::Preamble.reason(),
        Some("required source preamble")
    );
    assert_eq!(
        Protection::LoadBearing.reason(),
        Some("required by the language or its build")
    );
    /* NOTE: Spelled out rather than derived, because deriving it from the same
     * match it is checking would check nothing. A kind that changes tier has to
     * change here too, and that is meant to be an act. */
    for (kind, expected) in [
        (CommentKind::Line, Protection::None),
        (CommentKind::Block, Protection::None),
        (CommentKind::DocLine, Protection::None),
        (CommentKind::DocBlock, Protection::None),
        (CommentKind::Directive, Protection::None),
        (CommentKind::License, Protection::None),
        (CommentKind::HtmlComment, Protection::None),
        (CommentKind::Shebang, Protection::Preamble),
        (CommentKind::Encoding, Protection::Preamble),
        (CommentKind::OptimizerHint, Protection::LoadBearing),
        (CommentKind::VersionComment, Protection::LoadBearing),
        (CommentKind::LoadBearing, Protection::LoadBearing),
    ] {
        assert_eq!(kind.protection(), expected, "{kind}");
    }
}

/// Of the policies that would make a run clean, the one that still takes the
/// most is the one worth suggesting.
#[test]
fn the_policy_that_keeps_a_set_while_taking_the_most_is_found() {
    assert_eq!(
        Policy::strongest_keeping(&[CommentKind::DocLine, CommentKind::License]),
        Some(Policy::Conservative),
        "only `conservative` keeps documentation, so it is the only answer"
    );
    /* NOTE: Both `conservative` and `standard` keep a directive, and the answer
     * is `standard`: the caller is removing comments, so of the two the one
     * worth naming is the one that still takes the documentation and the
     * licence header. Naming the gentlest would answer a question nobody
     * asked. */
    assert_eq!(
        Policy::strongest_keeping(&[CommentKind::Directive]),
        Some(Policy::Standard)
    );
    assert_eq!(
        Policy::strongest_keeping(&[CommentKind::Line]),
        None,
        "no policy keeps an ordinary comment, and saying one does would be advice that fails"
    );
}
