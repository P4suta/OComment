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
    Action, Age, AllowRules, CommentKind, DispositionExplanation, DispositionPatterns, Language,
    Policy, ProtectedPattern, ProtectionTier, ScanOptions, explain_comment, explain_comment_with,
    explain_disposition, explain_disposition_with, scan,
};
use std::collections::{BTreeMap, BTreeSet};

/// Fixtures chosen so that between them the classifier emits every
/// [`CommentKind`]; `every_kind_is_covered` keeps that promise honest.
fn fixtures() -> Vec<(Language, &'static [u8])> {
    vec![
        (
            Language::Rust,
            /* NOTE: The last line is a comment beside code, which is the one
             * shape rule no other fixture reaches. */
            b"// plain\n/* block */\n/// doc line\n/** doc block */\n// Copyright 2024 Example\n// rustfmt::skip\nlet n = 1; // ordinary, and beside code\n"
                .as_slice(),
        ),
        (
            Language::JavaScript,
            b"// eslint-disable-next-line\n/* ordinary */\n".as_slice(),
        ),
        (Language::Html, b"<!-- observable -->\n".as_slice()),
        /* NOTE: A build constraint, which is the kind no `remove` policy
         * reaches: the tool tier above it is `// rustfmt::skip`, and what
         * separates the two is that removing this one changes which files the
         * compiler is given rather than what a linter says about them. */
        (
            Language::Go,
            b"//go:build linux\n// +build linux\n\npackage main\n// ordinary\n".as_slice(),
        ),
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

/// Every field of [`ScanOptions`], classified as either steered by the sweep
/// below or out of its reach — by destructuring rather than by a list, so a
/// field added later fails to compile here until somebody says which it is.
///
/// This is not decoration. `allow` was added without this, the sweep went on
/// covering the fields it already knew, and `--explain` spent a release
/// printing "removed: policy `conservative` removes ordinary comments" under
/// a line reading `kept line comment`. The sweep was passing the whole time,
/// because nothing made it look.
fn every_option_is_classified(options: ScanOptions) {
    let ScanOptions {
        // NOTE: Steered below, each by at least one variant.
        policy: _,
        force_protected: _,
        keep_kinds: _,
        remove_kinds: _,
        keep_regex: _,
        remove_regex: _,
        allow: _,
        protected: _,
        /* NOTE: Out of reach, and for the same reason in both cases: neither
         * changes any verdict. `dialect` chooses which bytes lex as a comment
         * and `force_invalid` chooses whether an edit is applied to a file
         * that would not lex; the disposition of a comment that was found is
         * the same either way. */
        dialect: _,
        force_invalid: _,
    } = options;
}

/// The same classification one level down, for the same reason.
///
/// `allow` is a table rather than a value, so covering "the `allow` field" is
/// not covering the rules in it.
fn every_allow_rule_is_classified(rules: AllowRules) {
    let AllowRules {
        // NOTE: Steered by `allow_variants`.
        tags: _,
        max_lines: _,
        trailing: _,
        /* NOTE: Out of reach here, and out of reach of this crate: the verdict
         * a deadline reaches needs the age of a line, which means reading a
         * repository. `a_deadline_is_not_this_crates_to_reach` is what states
         * that. The tag names still steer the tag rule, which is why the
         * variants below set one. */
        expiry: _,
    } = rules;
}

/// Each variant steers at least one branch of the table: the policies, the
/// preamble override, both kind lists and both regex lists, the overlap where
/// a keep and a remove pattern match the same bytes, and each of the three
/// shape rules on its own plus all three at once.
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
                ..base.clone()
            });
            for allow in allow_variants() {
                variants.push(ScanOptions {
                    allow,
                    ..base.clone()
                });
            }
            /* NOTE: A project's own markers, one per tier. The fixtures carry
             * `ordinary` and `Copyright`, so both arms are reached and the
             * stronger tier is reached under every policy including `all`. */
            variants.push(ScanOptions {
                protected: vec![
                    ProtectedPattern {
                        contains: "ordinary".into(),
                        reason: "read by our linter".into(),
                        tier: ProtectionTier::Tool,
                    },
                    ProtectedPattern {
                        contains: "Copyright".into(),
                        reason: "read by our build".into(),
                        tier: ProtectionTier::LoadBearing,
                    },
                ],
                ..base
            });
        }
    }
    for options in &variants {
        every_option_is_classified(options.clone());
        every_allow_rule_is_classified(options.allow.clone());
    }
    variants
}

/// One variant per shape rule, and one with all three, so that a fixture meets
/// each rule alone and meets the order they are applied in.
fn allow_variants() -> Vec<AllowRules> {
    vec![
        AllowRules {
            tags: vec!["ordinary".into(), "plain".into()],
            ..Default::default()
        },
        AllowRules {
            max_lines: Some(1),
            ..Default::default()
        },
        AllowRules {
            trailing: Some(false),
            ..Default::default()
        },
        AllowRules {
            tags: vec!["ordinary".into(), "copyright".into()],
            max_lines: Some(2),
            trailing: Some(false),
            ..Default::default()
        },
        /* NOTE: A tag with a deadline is an allowed tag until something with a
         * clock says otherwise, and nothing in this crate has one. */
        AllowRules {
            expiry: BTreeMap::from([("plain".to_owned(), Age::from_days(14))]),
            ..Default::default()
        },
    ]
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

/// The bytes-only entry point is the whole answer for every comment but the
/// ones the file around them decided, and this is what says which those are.
///
/// The match is exhaustive and the arms are the classification: a verdict a
/// comment's own bytes can reach has to equal what the bytes alone reached,
/// and a verdict that needs the file has to be one of the four named here and
/// has to be counted. A rule added later lands in neither list and the test
/// stops compiling, which is the point of writing it this way — this test
/// previously claimed there was exactly one such rule, and went on claiming it
/// while three more were added.
#[test]
fn the_two_entry_points_agree_away_from_the_one_rule() {
    let mut from_the_file = BTreeSet::new();
    for options in option_variants() {
        for (language, source) in fixtures() {
            let report = scan(source, language, options.clone());
            for comment in &report.comments {
                let raw = &source[comment.span.start..comment.span.end];
                let scanned = explain_comment(comment, raw, language, &options);
                let bytes_alone = explain_disposition(comment.kind, raw, language, &options);
                match scanned {
                    DispositionExplanation::KeptStructural { language: named } => {
                        from_the_file.insert("structural");
                        assert_eq!(named, language);
                        assert_eq!(language, Language::Yaml);
                        assert!(
                            bytes_alone.action().is_remove(),
                            "the bytes alone would have removed it: {bytes_alone}"
                        );
                    }
                    /* NOTE: The three shape rules. Each needs something outside
                     * the comment -- the tag list, the line the comment sits
                     * on, the run it belongs to -- so the bytes alone reaching
                     * a different verdict is the expected outcome rather than
                     * a disagreement. What is checked is that the scanner and
                     * the comment agree, which `..._over_the_whole_branch_table`
                     * states for every verdict and this one repeats for these. */
                    DispositionExplanation::KeptByTag { .. } => {
                        from_the_file.insert("tag");
                        assert!(!comment.disposition.is_remove());
                    }
                    DispositionExplanation::RemovedAsTrailing => {
                        from_the_file.insert("trailing");
                        assert!(comment.disposition.is_remove());
                    }
                    DispositionExplanation::RemovedByLength { lines, limit } => {
                        from_the_file.insert("length");
                        assert!(comment.disposition.is_remove());
                        assert!(lines > limit, "{lines} lines is not over {limit}");
                    }
                    /* NOTE: Unreachable by construction rather than by
                     * omission: the scan cannot measure the age of a line, so
                     * it never reaches this verdict, and an arm saying so is
                     * what keeps that true as the enum grows. */
                    DispositionExplanation::RemovedAsExpired { .. } => {
                        panic!("a scan reached a verdict that needs a repository to reach")
                    }
                    other @ (DispositionExplanation::KeptByKind(_)
                    | DispositionExplanation::KeptByRegex { .. }
                    | DispositionExplanation::ProtectedPreamble
                    | DispositionExplanation::KeptHtml
                    | DispositionExplanation::KeptLoadBearing { .. }
                    | DispositionExplanation::KeptDirective { .. }
                    | DispositionExplanation::KeptDocumentation { .. }
                    | DispositionExplanation::KeptLicense { .. }
                    | DispositionExplanation::RemovedByKind(_)
                    | DispositionExplanation::RemovedByRegex { .. }
                    | DispositionExplanation::RemovedByPolicy { .. }
                    | DispositionExplanation::RemovedByDefault { .. }) => assert_eq!(
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
    assert_eq!(
        from_the_file,
        BTreeSet::from(["length", "structural", "tag", "trailing"]),
        "the fixtures no longer reach every rule the file decides"
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
        DispositionExplanation::RemovedByDefault {
            policy: Policy::Standard,
            kind: CommentKind::Line,
        }
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
        DispositionExplanation::RemovedByDefault {
            policy: Policy::Standard,
            kind: CommentKind::Shebang,
        }
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
        DispositionExplanation::RemovedByPolicy {
            policy: Policy::All,
            kind: CommentKind::Shebang,
        }
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
        policy: Policy::Conservative,
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
            DispositionExplanation::RemovedByPolicy {
                policy: Policy::All,
                kind,
            },
            "{kind} under policy all"
        );
        assert!(explanation.to_string().contains("all"));
    }
}

#[test]
fn html_comments_are_kept_by_both_conservative_policies() {
    for policy in [Policy::Standard, Policy::Conservative] {
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
fn the_sql_comments_the_server_reads_are_out_of_reach_of_every_policy() {
    // NOTE: Both of these are read by the server as part of the statement: a
    // NOTE: version-gated comment is executed, and an optimizer hint decides
    // NOTE: the plan. That makes them load-bearing rather than directives
    // NOTE: addressed to a tool, so `all` does not reach them. It used to take
    // NOTE: both, which left a dumped database restoring into a different one
    // NOTE: with nothing failing.
    for (kind, raw) in [
        (CommentKind::OptimizerHint, "/*+ INDEX(t idx) */"),
        (CommentKind::VersionComment, "/*!40000 ALTER TABLE t */"),
    ] {
        for policy in Policy::ALL {
            let options = ScanOptions {
                policy,
                ..Default::default()
            };
            let explanation = explain(kind, raw, Language::Sql, &options);
            assert_eq!(
                explanation,
                DispositionExplanation::KeptLoadBearing { name: None },
                "{raw} under policy {policy}"
            );
            assert_eq!(explanation.action(), Action::Keep, "{raw} under {policy}");
            let sentence = explanation.to_string();
            assert!(sentence.contains("language or its build"), "{sentence}");
        }
    }
    // NOTE: The one way out, and the half of the claim that would otherwise
    // NOTE: never be observed failing: a gate that only ever keeps has not
    // NOTE: been shown to be a gate.
    for (kind, raw) in [
        (CommentKind::OptimizerHint, "/*+ INDEX(t idx) */"),
        (CommentKind::VersionComment, "/*!40000 ALTER TABLE t */"),
    ] {
        let forced = ScanOptions {
            policy: Policy::All,
            force_protected: true,
            ..Default::default()
        };
        let explanation = explain(kind, raw, Language::Sql, &forced);
        assert_eq!(
            explanation,
            DispositionExplanation::RemovedByPolicy {
                policy: Policy::All,
                kind,
            },
            "{raw} with --force-protected"
        );
    }
}

#[test]
fn a_license_is_kept_only_by_the_conservative_policy_and_names_its_marker() {
    let legal = ScanOptions {
        policy: Policy::Conservative,
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
        DispositionExplanation::RemovedByDefault {
            policy: Policy::Standard,
            kind: CommentKind::License,
        }
    );
    assert!(explanation.to_string().contains("standard"));
}

#[test]
fn ordinary_comments_fall_through_to_the_policy_default() {
    for policy in [Policy::Standard, Policy::Conservative] {
        let options = ScanOptions {
            policy,
            ..Default::default()
        };
        for (kind, raw) in [
            (CommentKind::Line, "// plain"),
            (CommentKind::Block, "/* block */"),
        ] {
            let explanation = explain(kind, raw, Language::Rust, &options);
            assert_eq!(
                explanation,
                DispositionExplanation::RemovedByDefault { policy, kind },
                "{kind} under policy {policy}"
            );
            assert_eq!(explanation.action(), Action::Remove);
        }
    }
}

/// A documentation comment is the API documentation, so the two policies part
/// company over it exactly as they do over a licence notice.
#[test]
fn documentation_is_kept_by_the_conservative_policy_and_taken_by_the_standard_one() {
    for (kind, raw) in [
        (CommentKind::DocLine, "/// doc line"),
        (CommentKind::DocBlock, "/** doc block */"),
    ] {
        let conservative = explain(
            kind,
            raw,
            Language::Rust,
            &ScanOptions {
                policy: Policy::Conservative,
                ..Default::default()
            },
        );
        assert_eq!(
            conservative,
            DispositionExplanation::KeptDocumentation { kind },
            "{kind} under the default policy"
        );
        assert_eq!(conservative.action(), Action::Keep);
        assert!(
            conservative.to_string().contains("documentation"),
            "{conservative}"
        );

        /* NOTE: The other half. A tier only ever observed keeping has not been
         * shown to be a tier, and `standard` is the policy someone reaches for
         * when they do mean to take the documentation. */
        let standard = explain(
            kind,
            raw,
            Language::Rust,
            &ScanOptions {
                policy: Policy::Standard,
                ..Default::default()
            },
        );
        assert_eq!(
            standard,
            DispositionExplanation::RemovedByDefault {
                policy: Policy::Standard,
                kind,
            },
            "{kind} under policy standard"
        );
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
        policy: Policy::Conservative,
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

/// The one verdict this crate owns the words for and never reaches.
///
/// `[policy.allow.expiry]` gives a tag a deadline, and how old a line is takes
/// a repository to answer. The scan therefore keeps such a comment exactly as
/// it keeps any other tagged one, and a caller with a clock takes it back. The
/// vocabulary lives here so both halves say the same thing.
#[test]
fn a_deadline_is_not_this_crates_to_reach() {
    let options = ScanOptions {
        policy: Policy::Conservative,
        allow: AllowRules {
            expiry: BTreeMap::from([("TODO".to_owned(), Age::from_days(14))]),
            ..Default::default()
        },
        ..Default::default()
    };
    let source = b"// TODO: a promise\nfn a() {}\n";
    let report = scan(source, Language::Rust, options.clone());
    let comment = &report.comments[0];
    assert!(!comment.disposition.is_remove(), "the scan took it back");
    let explanation = explain_comment(
        comment,
        &source[comment.span.start..comment.span.end],
        Language::Rust,
        &options,
    );
    assert_eq!(
        explanation,
        DispositionExplanation::KeptByTag {
            tag: "TODO".to_owned()
        }
    );
}

/// `"14d"`, `"2w"`, a bare number of days, and nothing that would be a guess.
#[test]
fn an_age_reads_the_units_a_commit_date_can_answer() {
    assert_eq!("14d".parse::<Age>(), Ok(Age::from_days(14)));
    assert_eq!("2w".parse::<Age>(), Ok(Age::from_days(14)));
    assert_eq!("0d".parse::<Age>(), Ok(Age::ZERO));
    assert_eq!("30".parse::<Age>(), Ok(Age::from_days(30)));
    assert_eq!(Age::from_days(14).to_string(), "14d");
    /* NOTE: An hour is not a meaningful deadline for a line of source and a
     * month is not a fixed number of days, so neither is guessed at. */
    assert!("12h".parse::<Age>().is_err());
    assert!("1m".parse::<Age>().is_err());
    assert!("soon".parse::<Age>().is_err());
}

/// Every field of every verdict, classified as either shown to a reader or
/// deliberately not — by destructuring without `..`, so a field added later
/// fails to compile here until somebody says which it is.
///
/// `every_option_is_classified` does this for the inputs and the match in
/// `explanation_rule` does it for the variants, and between them a new *reason*
/// cannot slip through. A new *field on an existing reason* still can: every
/// renderer in the CLI takes what it wants with `{ .. }`, so a reason that
/// gained something to say would go on printing yesterday's sentence and
/// nothing would fail. This is the one place that has to name the field.
///
/// The returned strings are what the rendering has to contain. A field that is
/// deliberately silent contributes none, which is a decision written down
/// rather than an omission nobody made.
fn shown_by(verdict: &DispositionExplanation) -> Vec<String> {
    match verdict {
        DispositionExplanation::KeptByKind(kind) => vec![kind.to_string()],
        DispositionExplanation::RemovedByKind(kind) => vec![kind.to_string()],
        DispositionExplanation::KeptByRegex { index, pattern } => {
            vec![index.to_string(), pattern.clone()]
        }
        DispositionExplanation::RemovedByRegex { index, pattern } => {
            vec![index.to_string(), pattern.clone()]
        }
        /* NOTE: Nothing to name: the reason is the whole of what happened. */
        DispositionExplanation::ProtectedPreamble
        | DispositionExplanation::KeptHtml
        | DispositionExplanation::RemovedAsTrailing => Vec::new(),
        /* NOTE: `name` and `marker` are `None` when the scanner kept the
         * comment without a marker to point at, and a sentence cannot quote
         * what is not there. When there is one, it is the whole point. */
        DispositionExplanation::KeptLoadBearing { name } => {
            name.map(str::to_owned).into_iter().collect()
        }
        DispositionExplanation::KeptLicense { marker } => {
            marker.map(str::to_owned).into_iter().collect()
        }
        /* NOTE: The sentence names the directive and not the kind. The kind is
         * not idle -- it is what the CLI's next-step clause turns into
         * `--remove-kind <kind>` -- but that clause is a different rendering,
         * and this one has nothing to say about whether a `rubocop:` arrived
         * on a line or in a block. */
        DispositionExplanation::KeptDirective { kind: _, name } => {
            name.map(str::to_owned).into_iter().collect()
        }
        DispositionExplanation::KeptDocumentation { kind } => vec![kind.to_string()],
        /* NOTE: The kind reaches the reader as the category it belongs to --
         * "ordinary comments", "doc comments", "license comments" -- so it is
         * not in the sentence as its own token and cannot be looked for here.
         * `a_policy_removal_does_not_say_the_same_thing_about_every_kind` is
         * what holds it: the sentence has to depend on the field. That is the
         * property that was missing when every kind, license included, was
         * reported as "removes ordinary comments". */
        DispositionExplanation::RemovedByPolicy { policy, kind: _ } => {
            vec![policy.to_string()]
        }
        DispositionExplanation::RemovedByDefault { policy, kind: _ } => {
            vec![policy.to_string()]
        }
        DispositionExplanation::KeptByTag { tag } => vec![tag.clone()],
        DispositionExplanation::RemovedAsExpired { tag, age, limit } => {
            vec![tag.clone(), age.to_string(), limit.to_string()]
        }
        DispositionExplanation::RemovedByLength { lines, limit } => {
            vec![lines.to_string(), limit.to_string()]
        }
        DispositionExplanation::KeptStructural { language } => vec![language.to_string()],
    }
}

/// One of every verdict, so the guard above is asked about all of them.
fn every_verdict() -> Vec<DispositionExplanation> {
    vec![
        DispositionExplanation::KeptByKind(CommentKind::Block),
        DispositionExplanation::RemovedByKind(CommentKind::Line),
        DispositionExplanation::KeptByRegex {
            index: 3,
            pattern: "^keep-me".to_owned(),
        },
        DispositionExplanation::RemovedByRegex {
            index: 4,
            pattern: "^drop-me".to_owned(),
        },
        DispositionExplanation::ProtectedPreamble,
        DispositionExplanation::KeptHtml,
        DispositionExplanation::RemovedAsTrailing,
        DispositionExplanation::KeptLoadBearing { name: Some("go:") },
        DispositionExplanation::KeptLoadBearing { name: None },
        DispositionExplanation::KeptLicense {
            marker: Some("SPDX-License-Identifier"),
        },
        DispositionExplanation::KeptLicense { marker: None },
        DispositionExplanation::KeptDirective {
            kind: CommentKind::Line,
            name: Some("rubocop:"),
        },
        DispositionExplanation::KeptDocumentation {
            kind: CommentKind::DocBlock,
        },
        DispositionExplanation::RemovedByPolicy {
            policy: Policy::All,
            kind: CommentKind::License,
        },
        DispositionExplanation::RemovedByDefault {
            policy: Policy::Conservative,
            kind: CommentKind::Line,
        },
        DispositionExplanation::KeptByTag {
            tag: "SAFETY".to_owned(),
        },
        DispositionExplanation::RemovedAsExpired {
            tag: "TODO".to_owned(),
            age: Age::from_days(40),
            limit: Age::from_days(30),
        },
        DispositionExplanation::RemovedByLength { lines: 9, limit: 8 },
        DispositionExplanation::KeptStructural {
            language: Language::Yaml,
        },
    ]
}

/// What a verdict names, it says.
#[test]
fn every_field_a_verdict_carries_reaches_the_reader() {
    for verdict in every_verdict() {
        let sentence = verdict.to_string();
        for expected in shown_by(&verdict) {
            assert!(
                sentence.contains(&expected),
                "`{verdict:?}` carries {expected:?} and its sentence does not say it: {sentence}"
            );
        }
    }
}

/// A policy removal names the kind it took, in the words a reader uses for it.
///
/// The exact-substring guard above cannot ask this, because the sentence says
/// "license comments" rather than `license`. What it can ask is that the
/// sentence is not constant in the field -- which is exactly what failed
/// before, when `RemovedByDefault` carried only the policy and every kind came
/// out as "removes ordinary comments" under a line that said `kept license
/// comment`.
#[test]
fn a_policy_removal_does_not_say_the_same_thing_about_every_kind() {
    for build in [
        (|kind| DispositionExplanation::RemovedByDefault {
            policy: Policy::Standard,
            kind,
        }) as fn(CommentKind) -> DispositionExplanation,
        |kind| DispositionExplanation::RemovedByPolicy {
            policy: Policy::All,
            kind,
        },
    ] {
        let sentences: BTreeSet<String> = CommentKind::ALL
            .into_iter()
            .map(|kind| build(kind).to_string())
            .collect();
        assert!(
            sentences.len() > 1,
            "a policy removal says the same thing about every kind: {sentences:?}"
        );
        assert_ne!(
            build(CommentKind::License).to_string(),
            build(CommentKind::Line).to_string(),
            "a licence removed by policy is reported the way an ordinary comment is"
        );
    }
}
