//! The explanation API mirrors `disposition()` branch for branch.
//!
//! Two claims are under test. The sweep checks that an explanation always
//! reaches the same verdict as the scanner did for the very same comment, over
//! every kind the classifier can produce crossed with every option that steers
//! a branch. The targeted cases then pin which branch each explanation names,
//! because agreeing on keep-or-remove is worthless if the stated reason is the
//! wrong one.
//!
//! The sweep asks `explain_comment`, not `explain_disposition`: one rule is
//! decided by where a comment sits rather than by what it says, and the
//! bytes-only entry point cannot see it. Every other comment gets the same
//! answer from both, which `the_two_entry_points_agree_away_from_the_one_rule`
//! is what states.

use ocomment_core::{
    Action, CommentKind, DispositionExplanation, DispositionPatterns, Language, Policy,
    ScanOptions, explain_comment, explain_comment_with, explain_disposition,
    explain_disposition_with, scan,
};
use std::collections::BTreeSet;

/// Fixtures chosen so that between them the classifier emits every
/// [`CommentKind`]; `every_kind_is_covered` keeps that promise honest.
fn fixtures() -> Vec<(Language, &'static [u8])> {
    vec![
        (
            Language::Rust,
            b"// plain\n/* block */\n/// doc line\n/** doc block */\n// Copyright 2024 Example\n// rustfmt::skip\n"
                .as_slice(),
        ),
        (
            Language::JavaScript,
            b"// eslint-disable-next-line\n/* ordinary */\n".as_slice(),
        ),
        (Language::Html, b"<!-- observable -->\n".as_slice()),
        (Language::Shell, b"#!/bin/sh\n# ordinary\n".as_slice()),
        (
            Language::Python,
            b"# -*- coding: utf-8 -*-\n# ordinary\n".as_slice(),
        ),
        (
            Language::Sql,
            b"/*+ INDEX(t idx) */\n/*!40000 ALTER TABLE t */\n-- ordinary\n".as_slice(),
        ),
        /* NOTE: A block scalar leaning on the comment that ends it, which is
         * the one verdict a comment's own bytes cannot reach. */
        (
            Language::Yaml,
            b"k: |\n  a\n# ends the block\n  # yamllint disable\nz: 1\n".as_slice(),
        ),
    ]
}

/// Each variant steers at least one branch of the table: the policies, the
/// preamble override, both kind lists and both regex lists, plus the overlap
/// where a keep and a remove pattern match the same bytes.
fn option_variants() -> Vec<ScanOptions> {
    let mut variants = Vec::new();
    for policy in Policy::ALL {
        for force_protected in [false, true] {
            let base = ScanOptions {
                policy,
                force_protected,
                ..Default::default()
            };
            variants.push(base.clone());
            variants.push(ScanOptions {
                keep_kinds: vec![CommentKind::Line, CommentKind::HtmlComment],
                ..base.clone()
            });
            variants.push(ScanOptions {
                remove_kinds: vec![
                    CommentKind::License,
                    CommentKind::Directive,
                    CommentKind::Shebang,
                    CommentKind::Encoding,
                ],
                ..base.clone()
            });
            variants.push(ScanOptions {
                keep_regex: vec!["never".into(), "(?i)ordinary".into()],
                ..base.clone()
            });
            variants.push(ScanOptions {
                remove_regex: vec!["(?i)copyright".into(), "(?i)doc".into()],
                ..base.clone()
            });
            variants.push(ScanOptions {
                keep_regex: vec!["(?i)coding".into()],
                remove_regex: vec!["(?i)coding".into()],
                ..base
            });
        }
    }
    variants
}

fn explain(
    kind: CommentKind,
    raw: &str,
    language: Language,
    options: &ScanOptions,
) -> DispositionExplanation {
    explain_disposition(kind, raw.as_bytes(), language, options)
}

/// The pattern sets are the same for every comment scanned under one set of
/// options, so a report that explains a whole file compiles them once and calls
/// the precompiled form. That form has to be the very same answer, over the
/// whole branch table, or the cheap path would quietly explain something else.
#[test]
fn the_precompiled_explanation_equals_the_convenience_wrapper() {
    for options in option_variants() {
        let patterns = DispositionPatterns::compile(&options).expect("the fixtures compile");
        for (language, source) in fixtures() {
            let report = scan(source, language, options.clone());
            for comment in &report.comments {
                let raw = &source[comment.span.start..comment.span.end];
                assert_eq!(
                    explain_disposition_with(&patterns, comment.kind, raw, language, &options),
                    explain_disposition(comment.kind, raw, language, &options),
                    "{language} {} `{}` under {options:?}",
                    comment.kind,
                    String::from_utf8_lossy(raw),
                );
                assert_eq!(
                    explain_comment_with(&patterns, comment, raw, language, &options),
                    explain_comment(comment, raw, language, &options),
                    "{language} {} `{}` under {options:?}",
                    comment.kind,
                    String::from_utf8_lossy(raw),
                );
            }
        }
    }
}

/// A pattern list that will not compile is ignored by the scanner, and both
/// entry points ignore it the same way: the empty sets a caller compiles for
/// the precompiled form are the fallback the wrapper builds for itself.
#[test]
fn an_unparseable_pattern_list_falls_back_the_same_way() {
    let options = ScanOptions {
        keep_regex: vec!["(".into()],
        ..ScanOptions::default()
    };
    assert!(DispositionPatterns::compile(&options).is_err());
    let raw = b"// ordinary".as_slice();
    assert_eq!(
        explain_disposition_with(
            &DispositionPatterns::empty(),
            CommentKind::Line,
            raw,
            Language::Rust,
            &options,
        ),
        explain_disposition(CommentKind::Line, raw, Language::Rust, &options),
    );
}

#[test]
fn every_kind_is_covered_by_a_fixture() {
    let mut seen = BTreeSet::new();
    for (language, source) in fixtures() {
        let report = scan(source, language, ScanOptions::default());
        assert!(report.valid, "{language} fixture must lex cleanly");
        for comment in &report.comments {
            seen.insert(comment.kind.as_str());
        }
    }
    let expected: BTreeSet<_> = CommentKind::ALL.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(seen, expected, "fixtures must exercise every comment kind");
}

#[test]
fn explanations_agree_with_the_scanner_over_the_whole_branch_table() {
    for options in option_variants() {
        for (language, source) in fixtures() {
            let report = scan(source, language, options.clone());
            for comment in &report.comments {
                let raw = &source[comment.span.start..comment.span.end];
                let explanation = explain_comment(comment, raw, language, &options);
                assert_eq!(
                    explanation.action().is_remove(),
                    comment.disposition.is_remove(),
                    "{language} {} `{}` under {options:?}: {explanation} contradicts {}",
                    comment.kind,
                    String::from_utf8_lossy(raw),
                    comment.disposition,
                );
            }
        }
    }
}

/// The bytes-only entry point is the whole answer for every comment but the one
/// the file around it decided, and this is what says which comments those are.
#[test]
fn the_two_entry_points_agree_away_from_the_one_rule() {
    let mut structural = 0;
    for options in option_variants() {
        for (language, source) in fixtures() {
            let report = scan(source, language, options.clone());
            for comment in &report.comments {
                let raw = &source[comment.span.start..comment.span.end];
                let scanned = explain_comment(comment, raw, language, &options);
                let bytes_alone = explain_disposition(comment.kind, raw, language, &options);
                match scanned {
                    DispositionExplanation::KeptStructural { language: named } => {
                        structural += 1;
                        assert_eq!(named, language);
                        assert_eq!(language, Language::Yaml);
                        assert!(
                            bytes_alone.action().is_remove(),
                            "the bytes alone would have removed it: {bytes_alone}"
                        );
                    }
                    other => assert_eq!(
                        other,
                        bytes_alone,
                        "{language} {} `{}` under {options:?}",
                        comment.kind,
                        String::from_utf8_lossy(raw),
                    ),
                }
            }
        }
    }
    assert!(
        structural > 0,
        "the fixtures no longer reach the positional rule"
    );
}

/// The sentence the new verdict writes, and the fact that no option reaches it:
/// `all` removes the directive under the comment and the question with it, but
/// an override that keeps that directive leaves this comment load-bearing.
#[test]
fn a_structural_keep_names_the_block_scalar_under_it() {
    let source = b"k: |\n  a\n# ends the block\n  # KEEPME\nz: 1\n";
    let options = ScanOptions {
        policy: Policy::All,
        keep_regex: vec!["KEEPME".into()],
        ..Default::default()
    };
    let report = scan(source, Language::Yaml, options.clone());
    let comment = &report.comments[0];
    let explanation = explain_comment(
        comment,
        &source[comment.span.start..comment.span.end],
        Language::Yaml,
        &options,
    );
    assert_eq!(
        explanation,
        DispositionExplanation::KeptStructural {
            language: Language::Yaml
        }
    );
    assert_eq!(explanation.action(), Action::Keep);
    let sentence = explanation.to_string();
    assert!(sentence.starts_with("kept:"), "{sentence}");
    assert!(sentence.contains("block scalar"), "{sentence}");
    assert!(sentence.contains("yaml"), "{sentence}");
}

#[test]
fn an_invalid_regex_explains_the_same_way_the_scanner_scans() {
    let options = ScanOptions {
        keep_regex: vec!["(unclosed".into()],
        ..Default::default()
    };
    let source = b"// plain\n";
    let report = scan(source, Language::Rust, options.clone());
    assert!(!report.valid, "an invalid pattern is a scan error");
    let explanation = explain(CommentKind::Line, "// plain", Language::Rust, &options);
    assert_eq!(
        explanation,
        DispositionExplanation::RemovedByDefault(Policy::Safe)
    );
    assert_eq!(
        explanation.action().is_remove(),
        report.comments[0].disposition.is_remove(),
    );
}

#[test]
fn a_kept_kind_names_the_kind() {
    let options = ScanOptions {
        keep_kinds: vec![CommentKind::Block],
        ..Default::default()
    };
    let explanation = explain(
        CommentKind::Block,
        "/* keep me */",
        Language::Rust,
        &options,
    );
    assert_eq!(
        explanation,
        DispositionExplanation::KeptByKind(CommentKind::Block)
    );
    assert_eq!(explanation.action(), Action::Keep);
    let sentence = explanation.to_string();
    assert!(sentence.starts_with("kept:"), "{sentence}");
    assert!(sentence.contains("block"), "{sentence}");
}

#[test]
fn a_kept_regex_names_the_first_matching_pattern() {
    let options = ScanOptions {
        keep_regex: vec!["never".into(), "(?i)generated".into(), "gener".into()],
        ..Default::default()
    };
    let explanation = explain(
        CommentKind::Line,
        "// GENERATED by a tool",
        Language::Rust,
        &options,
    );
    assert_eq!(
        explanation,
        DispositionExplanation::KeptByRegex {
            index: 1,
            pattern: "(?i)generated".into(),
        }
    );
    let sentence = explanation.to_string();
    assert!(sentence.contains("(?i)generated"), "{sentence}");
    assert!(sentence.contains("keep_regex"), "{sentence}");
}

#[test]
fn a_removed_regex_names_the_first_matching_pattern() {
    let options = ScanOptions {
        remove_regex: vec!["nope".into(), "(?i)todo".into()],
        ..Default::default()
    };
    let explanation = explain(
        CommentKind::License,
        "// TODO: copyright 2024",
        Language::Rust,
        &options,
    );
    assert_eq!(
        explanation,
        DispositionExplanation::RemovedByRegex {
            index: 1,
            pattern: "(?i)todo".into(),
        }
    );
    assert_eq!(explanation.action(), Action::Remove);
    let sentence = explanation.to_string();
    assert!(sentence.starts_with("removed:"), "{sentence}");
    assert!(sentence.contains("(?i)todo"), "{sentence}");
    assert!(sentence.contains("remove_regex"), "{sentence}");
}

#[test]
fn a_removed_kind_names_the_kind() {
    let options = ScanOptions {
        remove_kinds: vec![CommentKind::DocLine],
        ..Default::default()
    };
    let explanation = explain(CommentKind::DocLine, "/// docs", Language::Rust, &options);
    assert_eq!(
        explanation,
        DispositionExplanation::RemovedByKind(CommentKind::DocLine)
    );
    assert!(explanation.to_string().contains("doc-line"));
}

#[test]
fn the_preamble_is_protected_until_it_is_forced() {
    let default = ScanOptions::default();
    assert_eq!(
        explain(CommentKind::Shebang, "#!/bin/sh", Language::Shell, &default),
        DispositionExplanation::ProtectedPreamble
    );
    assert_eq!(
        explain(
            CommentKind::Encoding,
            "# -*- coding: utf-8 -*-",
            Language::Python,
            &default
        ),
        DispositionExplanation::ProtectedPreamble
    );
    let forced = ScanOptions {
        force_protected: true,
        ..Default::default()
    };
    assert_eq!(
        explain(CommentKind::Shebang, "#!/bin/sh", Language::Shell, &forced),
        DispositionExplanation::RemovedByDefault(Policy::Safe)
    );
    let forced_all = ScanOptions {
        force_protected: true,
        policy: Policy::All,
        ..Default::default()
    };
    assert_eq!(
        explain(
            CommentKind::Shebang,
            "#!/bin/sh",
            Language::Shell,
            &forced_all
        ),
        DispositionExplanation::RemovedByPolicy(Policy::All)
    );
}

#[test]
fn a_keep_override_outranks_every_later_branch() {
    let options = ScanOptions {
        policy: Policy::All,
        force_protected: true,
        keep_kinds: vec![CommentKind::Shebang],
        remove_kinds: vec![CommentKind::Shebang],
        remove_regex: vec!["bin".into()],
        ..Default::default()
    };
    assert_eq!(
        explain(CommentKind::Shebang, "#!/bin/sh", Language::Shell, &options),
        DispositionExplanation::KeptByKind(CommentKind::Shebang)
    );
    let by_regex = ScanOptions {
        policy: Policy::All,
        keep_regex: vec!["(?i)license".into()],
        remove_kinds: vec![CommentKind::License],
        ..Default::default()
    };
    assert_eq!(
        explain(
            CommentKind::License,
            "// SPDX-License-Identifier: MIT",
            Language::Rust,
            &by_regex
        ),
        DispositionExplanation::KeptByRegex {
            index: 0,
            pattern: "(?i)license".into(),
        }
    );
}

#[test]
fn a_remove_override_outranks_the_policy_protections() {
    let options = ScanOptions {
        policy: Policy::Legal,
        remove_kinds: vec![CommentKind::License, CommentKind::HtmlComment],
        ..Default::default()
    };
    assert_eq!(
        explain(
            CommentKind::License,
            "// Copyright 2024 Example",
            Language::Rust,
            &options
        ),
        DispositionExplanation::RemovedByKind(CommentKind::License)
    );
    assert_eq!(
        explain(
            CommentKind::HtmlComment,
            "<!-- observable -->",
            Language::Html,
            &options
        ),
        DispositionExplanation::RemovedByKind(CommentKind::HtmlComment)
    );
}

#[test]
fn policy_all_removes_what_the_other_policies_protect() {
    let options = ScanOptions {
        policy: Policy::All,
        ..Default::default()
    };
    for (kind, raw, language) in [
        (
            CommentKind::HtmlComment,
            "<!-- observable -->",
            Language::Html,
        ),
        (
            CommentKind::Directive,
            "// eslint-disable-next-line",
            Language::JavaScript,
        ),
        (
            CommentKind::License,
            "// Copyright 2024 Example",
            Language::Rust,
        ),
    ] {
        let explanation = explain(kind, raw, language, &options);
        assert_eq!(
            explanation,
            DispositionExplanation::RemovedByPolicy(Policy::All),
            "{kind} under policy all"
        );
        assert!(explanation.to_string().contains("all"));
    }
}

#[test]
fn html_comments_are_kept_by_both_conservative_policies() {
    for policy in [Policy::Safe, Policy::Legal] {
        let options = ScanOptions {
            policy,
            ..Default::default()
        };
        let explanation = explain(
            CommentKind::HtmlComment,
            "<!-- observable -->",
            Language::Html,
            &options,
        );
        assert_eq!(explanation, DispositionExplanation::KeptHtml);
        assert_eq!(explanation.action(), Action::Keep);
        assert!(explanation.to_string().contains("HTML"));
    }
}

#[test]
fn a_kept_directive_names_the_matched_directive() {
    for (raw, language, name) in [
        (
            "// eslint-disable-next-line",
            Language::JavaScript,
            Some("eslint"),
        ),
        ("//go:generate stringer", Language::Go, Some("go:")),
        ("// rustfmt::skip", Language::Rust, Some("rustfmt::")),
        (
            "/// <reference path=\"./x.d.ts\" />",
            Language::TypeScript,
            Some("///"),
        ),
    ] {
        let explanation = explain(
            CommentKind::Directive,
            raw,
            language,
            &ScanOptions::default(),
        );
        assert_eq!(
            explanation,
            DispositionExplanation::KeptDirective {
                kind: CommentKind::Directive,
                name,
            },
            "{raw}"
        );
        let sentence = explanation.to_string();
        assert!(sentence.contains("directive"), "{sentence}");
        assert!(sentence.contains(name.expect("named")), "{sentence}");
    }
}

#[test]
fn an_unnamed_directive_kind_still_explains_itself() {
    for (kind, raw) in [
        (CommentKind::OptimizerHint, "/*+ INDEX(t idx) */"),
        (CommentKind::VersionComment, "/*!40000 ALTER TABLE t */"),
    ] {
        let explanation = explain(kind, raw, Language::Sql, &ScanOptions::default());
        assert_eq!(
            explanation,
            DispositionExplanation::KeptDirective { kind, name: None },
            "{raw}"
        );
        let sentence = explanation.to_string();
        assert!(sentence.contains("directive"), "{sentence}");
        assert!(sentence.contains(kind.as_str()), "{sentence}");
    }
}

#[test]
fn a_license_is_kept_only_by_the_legal_policy_and_names_its_marker() {
    let legal = ScanOptions {
        policy: Policy::Legal,
        ..Default::default()
    };
    for (raw, marker) in [
        ("// Copyright 2024 Example", Some("copyright")),
        (
            "// SPDX-License-Identifier: MIT",
            Some("spdx-license-identifier"),
        ),
        ("/* All Rights Reserved */", Some("all rights reserved")),
    ] {
        let explanation = explain(CommentKind::License, raw, Language::Rust, &legal);
        assert_eq!(
            explanation,
            DispositionExplanation::KeptLicense { marker },
            "{raw}"
        );
        let sentence = explanation.to_string();
        assert!(sentence.contains(marker.expect("named")), "{sentence}");
    }
    let safe = ScanOptions::default();
    let explanation = explain(
        CommentKind::License,
        "// Copyright 2024 Example",
        Language::Rust,
        &safe,
    );
    assert_eq!(
        explanation,
        DispositionExplanation::RemovedByDefault(Policy::Safe)
    );
    assert!(explanation.to_string().contains("safe"));
}

#[test]
fn ordinary_comments_fall_through_to_the_policy_default() {
    for policy in [Policy::Safe, Policy::Legal] {
        let options = ScanOptions {
            policy,
            ..Default::default()
        };
        for (kind, raw) in [
            (CommentKind::Line, "// plain"),
            (CommentKind::Block, "/* block */"),
            (CommentKind::DocLine, "/// doc line"),
            (CommentKind::DocBlock, "/** doc block */"),
        ] {
            let explanation = explain(kind, raw, Language::Rust, &options);
            assert_eq!(
                explanation,
                DispositionExplanation::RemovedByDefault(policy),
                "{kind} under policy {policy}"
            );
            assert_eq!(explanation.action(), Action::Remove);
        }
    }
}

#[test]
fn the_action_helper_is_the_inverse_of_a_removal() {
    assert!(Action::Remove.is_remove());
    assert!(!Action::Keep.is_remove());
    assert_eq!(Action::Keep.as_str(), "keep");
    assert_eq!(Action::Remove.as_str(), "remove");
    assert_eq!(Action::Keep.to_string(), "keep");
    assert_eq!(Action::Remove.to_string(), "remove");
}

#[test]
fn explaining_a_report_leaves_the_report_alone() {
    let options = ScanOptions {
        policy: Policy::Legal,
        keep_regex: vec!["(?i)ordinary".into()],
        ..Default::default()
    };
    for (language, source) in fixtures() {
        let before = scan(source, language, options.clone());
        for comment in &before.comments {
            let raw = &source[comment.span.start..comment.span.end];
            let _ = explain_disposition(comment.kind, raw, language, &options).to_string();
        }
        let after = scan(source, language, options.clone());
        assert_eq!(before, after, "{language} scan output must be untouched");
    }
}
