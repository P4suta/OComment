//! Per-language and per-dialect lexing, one test per lexical hazard.
//!
//! Each case names the construct it protects — a raw string, a nested
//! comment, a heredoc, a regex literal — and asserts on the comments found
//! and on the bytes a transformation leaves behind, so a scanner that starts
//! reading a delimiter inside a string fails here rather than in a user's
//! repository.

use ocomment_core::{
    ByteSpan, CommentKind, Dialect, Disposition, Language, Layout, Policy, ScanOptions,
    TransformOptions, detect_language, scan, transform,
};
use std::path::Path;

fn options(dialect: Dialect) -> ScanOptions {
    ScanOptions {
        dialect,
        ..Default::default()
    }
}

fn removable(report: &ocomment_core::ScanReport) -> usize {
    report
        .comments
        .iter()
        .filter(|comment| comment.disposition.is_remove())
        .count()
}

#[test]
fn rust_nested_comments_raw_strings_and_directives() {
    let source = br##"r#"// not a comment"# /* outer /* inner */ end */
// rustfmt::skip
/// documentation
"##;
    let report = scan(source, Language::Rust, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 3);
    assert_eq!(report.comments[0].kind, CommentKind::Block);
    assert_eq!(report.comments[1].kind, CommentKind::Directive);
    assert_eq!(report.comments[2].kind, CommentKind::DocLine);
    assert_eq!(removable(&report), 2);
}

#[test]
fn rust_raw_c_strings_hide_comment_delimiters() {
    let source = b"cr#\"inner \" // opaque\"#; // remove\n";
    let report = scan(source, Language::Rust, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        source.len() - b"// remove\n".len()
    );
}

#[test]
fn rust_string_literals_may_span_lines() {
    let source = b"const HELP: &str = \"first\n// opaque\nlast\"; // remove\n";
    let report = scan(source, Language::Rust, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        source.len() - b"// remove\n".len()
    );
}

#[test]
fn rust_string_literals_must_still_terminate() {
    let source = b"const HELP: &str = \"first\n// opaque\n";
    let report = scan(source, Language::Rust, ScanOptions::default());
    assert!(!report.valid);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "unterminated-string")
    );
    assert!(report.comments.is_empty());
}

#[test]
fn ocaml_nested_comments_and_quoted_strings() {
    let source = br#"{tag| (* string *) |tag} (* outer "*)" (* inner *) *)"#;
    let report = scan(source, Language::Ocaml, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(report.comments[0].span.start, 25);
}

#[test]
fn ocaml_quoted_strings_are_opaque_inside_comments_and_must_terminate() {
    let source = br"(* outer {tag| *) opaque |tag} end *)";
    let report = scan(source, Language::Ocaml, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(report.comments[0].span, ByteSpan::new(0, source.len()));

    let invalid = scan(
        br"{tag| unterminated (* opaque *)",
        Language::Ocaml,
        ScanOptions::default(),
    );
    assert!(!invalid.valid);
    assert!(invalid.comments.is_empty());
}

#[test]
fn ocaml_quoted_string_identifiers_have_no_artificial_length_limit() {
    let identifier = "a".repeat(80);
    let source = format!("{{{identifier}|(* opaque *)|{identifier}}} (* remove *)");
    let report = scan(source.as_bytes(), Language::Ocaml, ScanOptions::default());

    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        &source.as_bytes()[report.comments[0].span.start..report.comments[0].span.end],
        b"(* remove *)"
    );
}

#[test]
fn c_line_splicing_is_applied_before_comment_lexing() {
    let source = b"int x; /\\\n/ comment\\\ncontinued\nint y;";
    let report = scan(source, Language::C, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        &source[report.comments[0].span.start..report.comments[0].span.end],
        b"/\\\n/ comment\\\ncontinued"
    );
    let trailing = b"// trailing\\\n";
    let result = transform(trailing, Language::C, TransformOptions::default());
    assert_eq!(result.output, b"\n");
}

#[test]
fn cpp_raw_strings_hide_delimiters_and_invalid_raw_strings_stop_fix() {
    let valid = br###"R"tag(/* not */ // no)tag" // yes"###;
    assert_eq!(
        scan(valid, Language::Cpp, ScanOptions::default())
            .comments
            .len(),
        1
    );
    let invalid = br#"R"tag(unterminated /* still string */"#;
    let result = transform(invalid, Language::Cpp, TransformOptions::default());
    assert!(!result.report.valid);
    assert!(result.edits.is_empty());
}

#[test]
fn go_build_and_compiler_directives_are_protected() {
    let source = b"//go:build linux\n// +build linux\n//line generated.go:1\n// ordinary\n";
    let report = scan(source, Language::Go, ScanOptions::default());
    assert_eq!(
        report
            .comments
            .iter()
            .filter(|comment| comment.kind == CommentKind::Directive)
            .count(),
        3
    );
    assert_eq!(removable(&report), 1);
}

#[test]
fn java_unicode_escapes_obey_backslash_eligibility() {
    let escaped_comment = br"int x; \u002f\u002f comment\u000aint y;";
    let report = scan(escaped_comment, Language::Java, ScanOptions::default());
    assert_eq!(report.comments.len(), 1);
    let ineligible = br#"String s = "\\u002f\\u002f not";"#;
    let report = scan(ineligible, Language::Java, ScanOptions::default());
    assert!(report.comments.is_empty());

    let surrogates = scan(
        br#"String s = "\uD83D\uDE00 // opaque"; // remove"#,
        Language::Java,
        ScanOptions::default(),
    );
    assert!(surrogates.valid);
    assert_eq!(surrogates.comments.len(), 1);

    let invalid = transform(
        br"int x = 1; \u00G0 // known",
        Language::Java,
        TransformOptions::default(),
    );
    assert!(!invalid.report.valid);
    assert_eq!(invalid.report.comments.len(), 1);
    assert!(invalid.edits.is_empty());
}

/// Java's documentation comments are `/** ... */` (JLS 3.7) and, since JDK 23,
/// `///` (JEP 467). `//!` is Rust's inner-doc marker and `/*!` is Doxygen's;
/// Java has a convention for neither, so a comment opening with either one is
/// an ordinary line or block comment and is treated as one. Reading them as
/// documentation would hide them from `--policy safe` in a language that never
/// meant them as documentation.
#[test]
fn java_reads_only_its_own_two_documentation_markers() {
    let source = b"/// javadoc\n//! plain\n/** javadoc */\n/*! plain */\nclass A {}\n";
    let report = scan(source, Language::Java, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(
        report
            .comments
            .iter()
            .map(|comment| comment.kind)
            .collect::<Vec<_>>(),
        vec![
            CommentKind::DocLine,
            CommentKind::Line,
            CommentKind::DocBlock,
            CommentKind::Block,
        ]
    );
    // NOTE: C and C++ do have the Doxygen convention, so the same bytes there
    // NOTE: are documentation, which is what makes this a Java rule rather
    // NOTE: than a change to how the markers are spelled.
    let doxygen = scan(source, Language::Cpp, ScanOptions::default());
    assert_eq!(
        doxygen
            .comments
            .iter()
            .map(|comment| comment.kind)
            .collect::<Vec<_>>(),
        vec![
            CommentKind::DocLine,
            CommentKind::DocLine,
            CommentKind::DocBlock,
            CommentKind::DocBlock,
        ]
    );
}

#[test]
fn java_text_block_ignores_an_escaped_closing_delimiter() {
    let source = b"String s = \"\"\"\n\\\"\"\" // opaque\nend\n\"\"\"; // remove\n";
    let report = scan(source, Language::Java, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        source.len() - b"// remove\n".len()
    );
}

/// A Python string literal begins at its prefix, not at its quote: `r"`,
/// `rb"` and `f"` are one token with the quote that follows them (Python
/// reference 2.4.1). So an unterminated one is reported from the prefix, which
/// is what the triple-quoted and f-string paths already did while the ordinary
/// single-quoted one started the span at the quote and left the `r` outside
/// the literal it belongs to.
#[test]
fn an_unterminated_python_string_is_reported_from_its_prefix() {
    let spans = |source: &[u8]| {
        scan(source, Language::Python, ScanOptions::default())
            .diagnostics
            .into_iter()
            .map(|item| (item.code, item.span))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        spans(br#"r""#),
        vec![("unterminated-string".to_owned(), ByteSpan::new(0, 2))]
    );
    assert_eq!(
        spans(br#"rb""#),
        vec![("unterminated-string".to_owned(), ByteSpan::new(0, 3))]
    );
    assert_eq!(
        spans(b"x = r\"abc\n"),
        vec![("unterminated-string".to_owned(), ByteSpan::new(4, 9))]
    );
    // NOTE: The two paths that already anchored at the prefix, here so the
    // NOTE: three cannot drift apart again.
    assert_eq!(
        spans(br#"r""""#),
        vec![("unterminated-string".to_owned(), ByteSpan::new(0, 4))]
    );
    assert_eq!(
        spans(br#"rf"{"#),
        vec![
            (
                "unterminated-fstring-expression".to_owned(),
                ByteSpan::new(4, 4)
            ),
            ("unterminated-string".to_owned(), ByteSpan::new(0, 4)),
        ]
    );
    // NOTE: An unprefixed literal is unchanged: the token and the quote are
    // NOTE: the same byte.
    assert_eq!(
        spans(br#"""#),
        vec![("unterminated-string".to_owned(), ByteSpan::new(0, 1))]
    );
}

#[test]
fn javascript_regex_templates_hashbang_and_jsx_have_distinct_goals() {
    let javascript = br#"#!/usr/bin/env node
const r = /\/\/* not a comment/;
const t = `literal // no ${1 /* yes */}`;
// yes
"#;
    let report = scan(javascript, Language::JavaScript, ScanOptions::default());
    assert_eq!(report.comments.len(), 3);
    assert_eq!(report.comments[0].kind, CommentKind::Shebang);
    assert_eq!(removable(&report), 2);

    let jsx = br#"const view = <div url="http://example.test">text // not {1 /* yes */}<span /></div>; // yes"#;
    let report = scan(jsx, Language::JavaScript, options(Dialect::Jsx));
    assert!(report.valid);
    assert_eq!(report.comments.len(), 2);

    let after_control_parenthesis =
        br"if (ready) /https?:\/\/example\.test/.test(value); // remove";
    let report = scan(
        after_control_parenthesis,
        Language::JavaScript,
        ScanOptions::default(),
    );
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);

    let object_division_and_block_regex =
        b"const ratio = {} / 2; // remove\nif (ready) {} /[/*]/.test(value); // remove\n";
    let report = scan(
        object_division_and_block_regex,
        Language::JavaScript,
        ScanOptions::default(),
    );
    assert!(report.valid);
    assert_eq!(report.comments.len(), 2);

    let html_like = b"const x = 1; <!-- remove\n  --> remove\nconst text = '<!-- opaque';\n";
    let report = scan(html_like, Language::JavaScript, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 2);

    let unicode_lines = "// remove\u{2028}const value = 1; /* first\u{2029}second */\n";
    let result = transform(
        unicode_lines.as_bytes(),
        Language::JavaScript,
        TransformOptions::default(),
    );
    assert!(result.report.valid);
    assert_eq!(result.report.comments.len(), 2);
    assert_eq!(
        result.output,
        "\u{2028}const value = 1; \u{2029}\n".as_bytes()
    );

    let invalid_unicode_string = scan(
        "const text = 'first\u{2028}second'; // known\n".as_bytes(),
        Language::JavaScript,
        ScanOptions::default(),
    );
    assert!(!invalid_unicode_string.valid);
    assert!(
        invalid_unicode_string
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "unterminated-string")
    );
}

#[test]
fn typescript_triple_slash_is_protected() {
    let source = br#"/// <reference path="types.d.ts" />
const value: string = "// text"; // ordinary
"#;
    let report = scan(source, Language::TypeScript, ScanOptions::default());
    assert_eq!(report.comments.len(), 2);
    assert_eq!(report.comments[0].kind, CommentKind::Directive);
    assert_eq!(removable(&report), 1);

    let directives = scan(
        b"const x = /*#__PURE__*/ factory();\n// @ts-expect-error\ncall();\n// ordinary\n",
        Language::TypeScript,
        ScanOptions::default(),
    );
    assert_eq!(
        directives
            .comments
            .iter()
            .filter(|comment| comment.kind == CommentKind::Directive)
            .count(),
        2
    );
    assert_eq!(removable(&directives), 1);
}

#[test]
fn python_strings_fstring_expressions_and_preambles() {
    let source = br#"#!/usr/bin/env python3
# coding: utf-8
text = r'''# not a comment'''
value = f'''literal # no {(
  1 # expression comment
)}'''
# ordinary
"#;
    let report = scan(source, Language::Python, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 4);
    assert_eq!(report.comments[0].kind, CommentKind::Shebang);
    assert_eq!(report.comments[1].kind, CommentKind::Encoding);
    assert_eq!(removable(&report), 2);

    let third_line = b"x = 1\ny = 2\n# coding: latin-1\n";
    let report = scan(third_line, Language::Python, ScanOptions::default());
    assert_eq!(report.comments[0].kind, CommentKind::Line);

    let inline = scan(
        b"value = 1  # coding: utf-8\n",
        Language::Python,
        ScanOptions::default(),
    );
    assert_eq!(inline.comments[0].kind, CommentKind::Line);

    let template = scan(
        b"value = t'''literal # opaque {(\n  1 # expression comment\n)}'''\n# remove\n",
        Language::Python,
        ScanOptions::default(),
    );
    assert!(template.valid);
    assert_eq!(template.comments.len(), 2);
}

#[test]
fn shell_heredocs_and_here_strings_are_not_comments() {
    let source = b"cat <<'EOF'\n# heredoc data\nEOF\ncat <<< '# here string'\n# ordinary\n";
    let report = scan(source, Language::Shell, options(Dialect::Bash53));
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
    let invalid = scan(
        b"echo 'unterminated",
        Language::Shell,
        ScanOptions::default(),
    );
    assert!(!invalid.valid);

    let quoted_words = b"cat <<E\"OF\"\n# opaque\nEOF\ncat <<\\DONE\n# opaque\nDONE\nvalue=$'it\\'s # opaque'\n# remove\n";
    let report = scan(quoted_words, Language::Shell, options(Dialect::Bash53));
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
}

/// A Dockerfile is scanned as shell today, and two of its lines are addressed
/// to a tool rather than to a reader: the `# syntax=` parser directive BuildKit
/// reads before it reads anything else, and `# hadolint ignore=`, which turns
/// one rule of the Dockerfile linter off for the instruction below it. Removing
/// either changes what a build does, so neither is explanatory text.
///
/// `hadolint` and `shellcheck` are whole words the two tools answer to, so
/// what ends either of them is a word boundary rather than one particular
/// byte: `# hadolint\tignore=` is the directive written with a tab, and prose
/// that merely opens with those letters — `# hadolintish note`,
/// `# shellcheckish note` — is a comment *about* the tool rather than an
/// instruction to it, and stays removable.
#[test]
fn dockerfile_parser_and_linter_directives_are_protected() {
    let source = b"# syntax=docker/dockerfile:1\n# explanatory\n# hadolint ignore=DL3018\n# hadolint\tignore=DL3019\n# hadolintish note\nRUN apk add --no-cache musl-dev\n# shellcheck disable=SC2086\n# shellcheck\tdisable=SC2087\n# shellcheckish note\n";
    let report = scan(source, Language::Shell, ScanOptions::default());
    assert!(report.valid);
    let kinds: Vec<_> = report.comments.iter().map(|comment| comment.kind).collect();
    assert_eq!(
        kinds,
        vec![
            CommentKind::Directive,
            CommentKind::Line,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Line,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Line,
        ]
    );
    assert_eq!(removable(&report), 3);
}

#[test]
fn shell_command_substitutions_are_scanned_inside_quotes() {
    let source = b"value=\"$(printf ok # nested\n)\"\nold=`printf ok # legacy\n`\ntext=\"# opaque\"\n# remove\n";
    let report = scan(source, Language::Shell, options(Dialect::Bash53));
    assert!(report.valid);
    assert_eq!(report.comments.len(), 3);

    let invalid = scan(
        b"value=$(echo ok # comment\n",
        Language::Shell,
        ScanOptions::default(),
    );
    assert!(!invalid.valid);
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "unterminated-command-substitution")
    );

    let word_boundaries = b"value=word\\\n#suffix\njoined=$(printf x)#suffix\nprintf ok \\\n# remove\n$(printf x);# remove\n";
    let report = scan(word_boundaries, Language::Shell, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 2);

    let case_command = b"value=$(case x in\n  a) # remove\n    printf '%s' '# opaque' ;;\n  *) printf ok ;;\nesac\n)#suffix\n# remove\n";
    let report = scan(case_command, Language::Shell, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 2);
}

#[test]
fn html_comments_are_explicit_only_and_embedded_languages_recurse() {
    let source =
        b"<!-- observable\ncomment --><style>a{/* css */}</style><script>let x=1;// js\n</script>";
    let safe = transform(source, Language::Html, TransformOptions::default());
    assert!(safe.output.starts_with(b"<!-- observable\ncomment -->"));
    assert_eq!(safe.report.comments.len(), 3);
    let all = transform(
        source,
        Language::Html,
        TransformOptions {
            scan: ScanOptions {
                policy: Policy::All,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(!all.output.windows(4).any(|window| window == b"<!--"));

    let columns_after_html = transform(
        b"ab<!-- drop\nline --><script>let x=1;/*\t*/y</script>",
        Language::Html,
        TransformOptions {
            scan: ScanOptions {
                policy: Policy::All,
                ..Default::default()
            },
            layout: Layout::Columns,
        },
    );
    assert_eq!(
        columns_after_html.output,
        b"ab<script>let x=1;        y</script>"
    );

    let boundary = scan(
        b"text 1 < 2<script>const s = \"</scripture>\"; // remove\n</script>",
        Language::Html,
        ScanOptions::default(),
    );
    assert!(boundary.valid);
    assert_eq!(boundary.comments.len(), 1);

    let invalid_source = b"<script>const x = 1; // known\n";
    let invalid = transform(invalid_source, Language::Html, TransformOptions::default());
    assert!(!invalid.report.valid);
    assert_eq!(invalid.report.comments.len(), 1);
    assert!(invalid.edits.is_empty());
    let forced = transform(
        invalid_source,
        Language::Html,
        TransformOptions {
            scan: ScanOptions {
                force_invalid: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert_eq!(forced.edits.len(), 1);
}

#[test]
fn css_and_jsonc_strings_are_opaque() {
    let css = br#"a { content: "/* no */"; /* yes */ }"#;
    assert_eq!(
        scan(css, Language::Css, ScanOptions::default())
            .comments
            .len(),
        1
    );
    let jsonc = br#"{"url":"https://example.test",/* yes */"x":"// no"}// line"#;
    assert_eq!(
        scan(jsonc, Language::Jsonc, ScanOptions::default())
            .comments
            .len(),
        2
    );
}

#[test]
fn sql_dialects_handle_special_quotes_and_protected_hints() {
    let postgres = br#"select $tag$-- no /* no */$tag$; /* outer /* nested */ end */ -- yes"#;
    let report = scan(postgres, Language::Sql, options(Dialect::PostgreSql));
    assert!(report.valid);
    assert_eq!(report.comments.len(), 2);

    let oracle = br#"select q'[-- no /* no */]' from dual; /*+ index(t) */ -- yes"#;
    let report = scan(oracle, Language::Sql, options(Dialect::Oracle));
    assert_eq!(report.comments.len(), 2);
    assert_eq!(report.comments[0].kind, CommentKind::OptimizerHint);
    assert!(matches!(
        report.comments[0].disposition,
        Disposition::Keep { .. }
    ));

    let mysql = b"/*!40101 SET NAMES utf8 */ # ordinary\n";
    let report = scan(mysql, Language::Sql, options(Dialect::MySql));
    assert_eq!(report.comments[0].kind, CommentKind::VersionComment);

    let mysql_boundary = scan(
        b"select 1--2; -- remove\n",
        Language::Sql,
        options(Dialect::MySql),
    );
    assert_eq!(mysql_boundary.comments.len(), 1);

    let tsql_nested = scan(
        b"/* outer /* inner */ end */ select 1;",
        Language::Sql,
        options(Dialect::TSql),
    );
    assert!(tsql_nested.valid);
    assert_eq!(tsql_nested.comments.len(), 1);

    let standard = scan(
        b"select '\\'; -- remove\n",
        Language::Sql,
        ScanOptions::default(),
    );
    assert!(standard.valid);
    assert_eq!(standard.comments.len(), 1);

    let postgres_escape = scan(
        b"select E'it\\'s -- opaque'; -- remove\n",
        Language::Sql,
        options(Dialect::PostgreSql),
    );
    assert!(postgres_escape.valid);
    assert_eq!(postgres_escape.comments.len(), 1);

    let invalid_dollar_tag = scan(
        b"select $1$; -- remove\n",
        Language::Sql,
        options(Dialect::PostgreSql),
    );
    assert!(invalid_dollar_tag.valid);
    assert_eq!(invalid_dollar_tag.comments.len(), 1);

    let tag = "a".repeat(65);
    let long_dollar = format!("select ${tag}$-- opaque${tag}$; -- remove\n");
    let long_dollar = scan(
        long_dollar.as_bytes(),
        Language::Sql,
        options(Dialect::PostgreSql),
    );
    assert!(long_dollar.valid);
    assert_eq!(long_dollar.comments.len(), 1);

    let mysql_double = scan(
        b"select \"it\\\"s -- opaque\"; -- remove\n",
        Language::Sql,
        options(Dialect::MySql),
    );
    assert!(mysql_double.valid);
    assert_eq!(mysql_double.comments.len(), 1);

    let non_sql_hint = scan(
        b"int x; /*+ ordinary C comment */",
        Language::C,
        ScanOptions::default(),
    );
    assert_eq!(non_sql_hint.comments[0].kind, CommentKind::Block);
    assert_eq!(removable(&non_sql_hint), 1);
}

#[test]
fn kotlin_nested_comments_and_triple_strings() {
    let source = br#"val text = """/* no */ // no"""
/* outer /* nested */ end */
// yes
"#;
    let report = scan(source, Language::Kotlin, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 2);

    let templates = b"val regular = \"opaque // ${1 /* expression */}\"\nval raw = \"\"\"opaque ${run { // expression\n1 }} /* opaque */\"\"\"\n// remove\n";
    let report = scan(templates, Language::Kotlin, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 3);
}

/* NOTE: TOML has one comment form and no block form, so every hazard below is
 * a question about which `#` is inside a string. The sections cited are of the
 * TOML v1.0.0 specification. */

#[test]
fn toml_comments_are_line_comments_wherever_they_open() {
    let source = b"# leading\nkey = \"value\" # trailing\n[table] # after a header\n";
    let report = scan(source, Language::Toml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 3);
    assert!(
        report
            .comments
            .iter()
            .all(|comment| comment.kind == CommentKind::Line),
        "TOML has only the line form: {:?}",
        report.comments
    );
    assert_eq!(removable(&report), 3);
}

#[test]
fn toml_string_and_quoted_key_forms_hide_comment_openers() {
    let source = br##"basic = "a # b"
literal = 'c # d'
"quoted # key" = 1
'literal # key' = 2
inline = { x = "# no", y = 'no #' } # yes
"##;
    let report = scan(source, Language::Toml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        source.len() - b"# yes\n".len()
    );
}

#[test]
fn toml_multi_line_strings_hide_comment_openers() {
    let source = br##"basic = """
# opaque
"""
literal = '''
# opaque
'''
after = 1 # yes
"##;
    let report = scan(source, Language::Toml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        source.len() - b"# yes\n".len()
    );
}

#[test]
fn a_toml_multi_line_string_ends_at_the_last_three_of_up_to_five_quotes() {
    let source =
        b"a = \"\"\"x\"\"\"\" # four\nb = \"\"\"y\"\"\"\"\" # five\nc = \"\"\"z\"\"\" # three\n";
    let report = scan(source, Language::Toml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 3);
    let literal = b"a = '''x'''' # four\nb = '''y''''' # five\nc = '''z''' # three\n";
    let report = scan(literal, Language::Toml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 3);
}

#[test]
fn toml_escapes_are_read_in_basic_strings_and_not_in_literal_ones() {
    let basic = b"a = \"\"\"line \\\n  # opaque \\\"\"\" closed \"\"\"\nb = 1 # yes\n";
    let report = scan(basic, Language::Toml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        basic.len() - b"# yes\n".len()
    );

    let literal = b"a = '''keep \\''' # yes\nb = 1\n";
    let report = scan(literal, Language::Toml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(report.comments[0].span.start, b"a = '''keep \\''' ".len());
}

#[test]
fn every_unterminated_toml_string_stops_a_fix_until_it_is_forced() {
    for source in [
        b"a = \"unclosed\nb = 1\n".as_slice(),
        b"a = 'unclosed\nb = 1\n".as_slice(),
        b"a = \"\"\"unclosed\nb = 1\n".as_slice(),
        b"a = '''unclosed\nb = 1\n".as_slice(),
    ] {
        let result = transform(source, Language::Toml, TransformOptions::default());
        assert!(
            !result.report.valid,
            "{:?} was accepted",
            String::from_utf8_lossy(source)
        );
        assert!(
            result
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "unterminated-string"),
            "{:?} reported {:?}",
            String::from_utf8_lossy(source),
            result.report.diagnostics
        );
        assert!(
            result.edits.is_empty(),
            "{:?} was edited anyway",
            String::from_utf8_lossy(source)
        );
    }
    let forced = transform(
        b"# note\na = \"\"\"unclosed\n",
        Language::Toml,
        TransformOptions {
            scan: ScanOptions {
                force_invalid: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(!forced.report.valid);
    assert_eq!(forced.output, b"\na = \"\"\"unclosed\n");
}

#[test]
fn toml_schema_and_formatter_directives_are_protected() {
    let source = b"#:schema https://example.test/pyproject.json\n# taplo: array_auto_expand = false\n# ordinary\n";
    let report = scan(source, Language::Toml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(
        report
            .comments
            .iter()
            .filter(|comment| comment.kind == CommentKind::Directive)
            .count(),
        2
    );
    assert_eq!(removable(&report), 1);
}

#[test]
fn a_toml_hash_bang_is_a_preamble_only_on_the_first_line() {
    let source = b"#!/usr/bin/env taplo\n#! second line\n#! third line\n";
    let report = scan(source, Language::Toml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 3);
    assert_eq!(report.comments[0].kind, CommentKind::Shebang);
    assert_eq!(report.comments[1].kind, CommentKind::Line);
    assert_eq!(report.comments[2].kind, CommentKind::Line);
    assert_eq!(removable(&report), 2);
}

#[test]
fn toml_multi_line_constructs_survive_crlf_line_endings() {
    let source =
        b"a = \"\"\"\r\n# opaque\r\n\"\"\"\r\nb = '''\r\n# opaque\r\n'''\r\nc = 1 # yes\r\n";
    let report = scan(source, Language::Toml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(report.comments[0].span.end, source.len() - b"\r\n".len());
    let unterminated = scan(
        b"a = \"\"\"unclosed\r\n",
        Language::Toml,
        ScanOptions::default(),
    );
    assert!(!unterminated.valid);
}

#[test]
fn toml_is_detected_from_its_extension_and_from_the_lock_files_written_in_it() {
    let found = detect_language(Some(Path::new("pyproject.toml")), b"")
        .expect("`.toml` is detected as nothing");
    assert_eq!(
        (found.language, found.dialect, found.reason),
        (Language::Toml, Dialect::Standard, "extension")
    );
    for name in [
        "Cargo.lock",
        "Pipfile",
        "poetry.lock",
        "uv.lock",
        "pdm.lock",
    ] {
        let found =
            detect_language(Some(Path::new(name)), b"").unwrap_or_else(|| panic!("`{name}`"));
        assert_eq!(
            (found.language, found.reason),
            (Language::Toml, "reserved-filename"),
            "`{name}`"
        );
    }
    /* NOTE: `Pipfile.lock` is the JSON half of the pair Pipenv writes, so the
     * name that carries no extension of its own is the only one of the two
     * this scanner answers to. */
    assert!(detect_language(Some(Path::new("Pipfile.lock")), b"").is_none());
}

#[test]
fn toml_layouts_leave_a_line_columns_or_nothing() {
    let source = b"# alone\nkey = 1 # trailing\n";
    let lines = transform(source, Language::Toml, TransformOptions::default());
    assert_eq!(lines.output, b"\nkey = 1 \n");
    let columns = transform(
        source,
        Language::Toml,
        TransformOptions {
            layout: Layout::Columns,
            ..Default::default()
        },
    );
    let padded = format!("{}\nkey = 1 {}\n", " ".repeat(7), " ".repeat(10));
    assert_eq!(columns.output, padded.as_bytes());
    let compact = transform(
        source,
        Language::Toml,
        TransformOptions {
            layout: Layout::Compact,
            ..Default::default()
        },
    );
    assert_eq!(compact.output, b"key = 1\n");
}

/* NOTE: Lua's hazards are all about the long bracket: `[`, any number of `=`,
 * `[` opens a comment when `--` precedes it and a string when nothing does,
 * and it closes only at its own level. The sections cited are of the Lua 5.4
 * reference manual, 3.1 Lexical Conventions. Lua has no string interpolation,
 * so there is no comment inside one of those to protect. */

#[test]
fn lua_comment_forms_carry_their_own_kinds() {
    let source = b"-- line\n--- documentation\n---- divider\n--[[ long ]]\n--[==[ level two ]==]\nlocal x = 1 -- trailing\n";
    let report = scan(source, Language::Lua, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let kinds: Vec<CommentKind> = report.comments.iter().map(|comment| comment.kind).collect();
    assert_eq!(
        kinds,
        [
            CommentKind::Line,
            CommentKind::DocLine,
            CommentKind::Line,
            CommentKind::Block,
            CommentKind::Block,
            CommentKind::Line,
        ],
        "{:?}",
        report.comments
    );
    assert_eq!(removable(&report), 6);
}

#[test]
fn lua_string_forms_hide_comment_openers() {
    let source = br#"a = "-- not a comment"
b = '--[[ not a comment ]]'
c = [[ -- opaque ]]
d = [==[ -- opaque ]] ]==]
e = 1 -- yes
"#;
    let report = scan(source, Language::Lua, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        source.len() - b"-- yes\n".len()
    );
}

#[test]
fn a_lua_long_bracket_closes_only_at_its_own_level() {
    let source = b"--[==[ ]] ]=] ]===] still inside ]==]\nx = 1 -- yes\n";
    let report = scan(source, Language::Lua, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 2);
    assert_eq!(report.comments[0].kind, CommentKind::Block);
    assert_eq!(
        report.comments[0].span.end,
        b"--[==[ ]] ]=] ]===] still inside ]==]".len()
    );
    let string = b"s = [=[ ]] inside ]=] -- yes\n";
    let report = scan(string, Language::Lua, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        string.len() - b"-- yes\n".len()
    );
}

#[test]
fn a_lua_long_bracket_needs_the_second_bracket_to_open_at_all() {
    /* NOTE: `[b` and `[1` are indexing rather than long strings, so the comment
     * on the same line is still found; `--[=` never reaches its second `[`,
     * which leaves it an ordinary comment to the end of the line. */
    let source = b"a[b[1]] = 2 -- yes\n--[= still a line comment\nc = 3\n";
    let report = scan(source, Language::Lua, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 2);
    assert_eq!(report.comments[0].span.start, b"a[b[1]] = 2 ".len());
    assert_eq!(
        report.comments[1].span.end,
        source.len() - b"\nc = 3\n".len()
    );
    assert!(
        report
            .comments
            .iter()
            .all(|comment| comment.kind == CommentKind::Line)
    );
}

#[test]
fn a_lua_short_string_carries_whitespace_skips_and_line_continuations() {
    let source = b"a = \"x \\z\n   y\"\nb = \"c \\\nd\"\ne = 1 -- yes\n";
    let report = scan(source, Language::Lua, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        source.len() - b"-- yes\n".len()
    );
}

#[test]
fn every_unterminated_lua_construct_stops_a_fix_until_it_is_forced() {
    for (source, code) in [
        (b"a = \"unclosed\nb = 1\n".as_slice(), "unterminated-string"),
        (b"a = 'unclosed\nb = 1\n".as_slice(), "unterminated-string"),
        (
            b"a = \"unclosed at the end".as_slice(),
            "unterminated-string",
        ),
        (b"a = [[unclosed\nb = 1\n".as_slice(), "unterminated-string"),
        (
            b"a = [==[unclosed ]] ]=]\n".as_slice(),
            "unterminated-string",
        ),
        (b"--[[ unclosed\nb = 1\n".as_slice(), "unterminated-comment"),
        (
            b"--[==[ unclosed ]] ]=]\n".as_slice(),
            "unterminated-comment",
        ),
    ] {
        let result = transform(source, Language::Lua, TransformOptions::default());
        assert!(
            !result.report.valid,
            "{:?} was accepted",
            String::from_utf8_lossy(source)
        );
        assert!(
            result
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code),
            "{:?} reported {:?}",
            String::from_utf8_lossy(source),
            result.report.diagnostics
        );
        assert!(
            result.edits.is_empty(),
            "{:?} was edited anyway",
            String::from_utf8_lossy(source)
        );
    }
    let forced = transform(
        b"-- note\na = [[unclosed\n",
        Language::Lua,
        TransformOptions {
            scan: ScanOptions {
                force_invalid: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(!forced.report.valid);
    assert_eq!(forced.output, b"\na = [[unclosed\n");
}

#[test]
fn lua_annotation_and_linter_directives_are_protected() {
    let source = b"---@diagnostic disable-next-line: undefined-global\n\
---@param count number\n\
-- luacheck: ignore 212\n\
-- selene: allow(unused_variable)\n\
-- stylua: ignore\n\
-- luacov: disable\n\
-- ordinary\n";
    let report = scan(source, Language::Lua, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let kinds: Vec<CommentKind> = report.comments.iter().map(|comment| comment.kind).collect();
    assert_eq!(
        kinds,
        [
            CommentKind::Directive,
            CommentKind::DocLine,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Line,
        ],
        "{:?}",
        report.comments
    );
    /* NOTE: `---@param` documents a type where `---@diagnostic` instructs the
     * language server, and only the second is a directive, so the annotation
     * and the ordinary comment are the two a `safe` run removes. */
    assert_eq!(removable(&report), 2);
}

#[test]
fn a_lua_hash_line_is_a_preamble_only_at_the_first_byte() {
    let shebang = b"#!/usr/bin/env lua\nx = 1 -- yes\n";
    let report = scan(shebang, Language::Lua, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 2);
    assert_eq!(report.comments[0].kind, CommentKind::Shebang);
    assert_eq!(removable(&report), 1);

    /* NOTE: The loader skips the whole of a first line that opens with `#`,
     * whether or not a `!` follows, so a bare one is a comment Lua never sees
     * — and one a `safe` run may therefore remove. */
    let bare = b"# the loader skips this\nx = 1 -- yes\n";
    let report = scan(bare, Language::Lua, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 2);
    assert_eq!(report.comments[0].kind, CommentKind::Line);
    assert_eq!(removable(&report), 2);

    /* NOTE: On any later line `#` is the length operator, so neither the second
     * nor the third line of a file holds a comment the way the first does. */
    for source in [
        b"x = 1\n#!/usr/bin/env lua\n".as_slice(),
        b"x = 1\ny = 2\n# not a comment\n".as_slice(),
    ] {
        let report = scan(source, Language::Lua, ScanOptions::default());
        assert!(
            report.comments.is_empty(),
            "{:?} found {:?}",
            String::from_utf8_lossy(source),
            report.comments
        );
    }
}

#[test]
fn lua_multi_line_constructs_survive_crlf_line_endings() {
    let source = b"--[[ long\r\ncomment ]]\r\ns = [==[\r\n-- opaque\r\n]==]\r\nc = \"x \\\r\ny\"\r\nd = \"p \\z\r\n  q\"\r\ne = 1 -- yes\r\n";
    let report = scan(source, Language::Lua, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 2);
    assert_eq!(report.comments[0].kind, CommentKind::Block);
    assert_eq!(report.comments[1].span.end, source.len() - b"\r\n".len());
    let unterminated = scan(
        b"a = \"x\r\nb = 1\r\n",
        Language::Lua,
        ScanOptions::default(),
    );
    assert!(!unterminated.valid);
}

#[test]
fn lua_is_detected_from_its_extensions_and_from_a_shebang() {
    for name in ["init.lua", "luarocks-3.11-1.rockspec"] {
        let found = detect_language(Some(Path::new(name)), b"")
            .unwrap_or_else(|| panic!("`{name}` is detected as nothing"));
        assert_eq!(
            (found.language, found.dialect, found.reason),
            (Language::Lua, Dialect::Standard, "extension"),
            "`{name}`"
        );
    }
    for line in [
        "#!/usr/bin/env lua\n",
        "#!/usr/bin/lua5.4\n",
        "#!/usr/bin/luajit\n",
    ] {
        let found = detect_language(None, line.as_bytes())
            .unwrap_or_else(|| panic!("`{line}` is detected as nothing"));
        assert_eq!(
            (found.language, found.reason),
            (Language::Lua, "shebang"),
            "`{line}`"
        );
    }
    /* NOTE: Lua reserves no whole file name: `rockspec` is a suffix a package
     * writes in front of, and a file called nothing else is not one. */
    assert!(detect_language(Some(Path::new("rockspec")), b"").is_none());
}

#[test]
fn lua_layouts_leave_a_line_columns_or_nothing() {
    let source = b"-- alone\nx = 1 -- trailing\n";
    let lines = transform(source, Language::Lua, TransformOptions::default());
    assert_eq!(lines.output, b"\nx = 1 \n");
    let columns = transform(
        source,
        Language::Lua,
        TransformOptions {
            layout: Layout::Columns,
            ..Default::default()
        },
    );
    let padded = format!("{}\nx = 1 {}\n", " ".repeat(8), " ".repeat(11));
    assert_eq!(columns.output, padded.as_bytes());
    let compact = transform(
        source,
        Language::Lua,
        TransformOptions {
            layout: Layout::Compact,
            ..Default::default()
        },
    );
    assert_eq!(compact.output, b"x = 1\n");
}

#[test]
fn legal_policy_and_force_protected_are_ordered() {
    let source = b"#!/usr/bin/env node\n// SPDX-License-Identifier: MIT\n// ordinary\n";
    let legal = scan(
        source,
        Language::JavaScript,
        ScanOptions {
            policy: Policy::Legal,
            ..Default::default()
        },
    );
    assert_eq!(removable(&legal), 1);
    let all = scan(
        source,
        Language::JavaScript,
        ScanOptions {
            policy: Policy::All,
            ..Default::default()
        },
    );
    assert_eq!(removable(&all), 2);
    let forced = scan(
        source,
        Language::JavaScript,
        ScanOptions {
            policy: Policy::All,
            force_protected: true,
            ..Default::default()
        },
    );
    assert_eq!(removable(&forced), 3);
}

#[test]
fn byte_preservation_and_layouts() {
    let source = b"\xef\xbb\xbfa\xff/*\xe4\xb8\xad*/b\r\n";
    let lines = transform(source, Language::C, TransformOptions::default());
    assert_eq!(lines.output, b"\xef\xbb\xbfa\xff b\r\n");
    let columns = transform(
        source,
        Language::C,
        TransformOptions {
            layout: Layout::Columns,
            ..Default::default()
        },
    );
    assert!(columns.output.starts_with(b"\xef\xbb\xbfa\xff"));
    assert!(columns.output.ends_with(b"b\r\n"));

    let mixed_utf8 = transform(
        b"x/*\xe4\xb8\xad\xff*/y",
        Language::C,
        TransformOptions {
            layout: Layout::Columns,
            ..Default::default()
        },
    );
    assert_eq!(mixed_utf8.output, b"x       y");
}
