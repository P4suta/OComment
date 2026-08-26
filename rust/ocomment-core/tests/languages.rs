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

/// The offset at which `needle` occurs in `source`, so a case can name the
/// comment it means by the text of it rather than by a counted offset.
fn offset_of(source: &[u8], needle: &[u8]) -> usize {
    source
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap_or_else(|| panic!("{:?} is not in the source", String::from_utf8_lossy(needle)))
}

/// YAML 1.2.2, 6.6 (Comments): a comment must be separated from other tokens
/// by white space, so a `#` behind a non-space byte is content — the fragment
/// of a URL, a hash in the middle of a plain scalar — and only one at the
/// start of a line or behind a space or a tab opens a comment.
#[test]
fn a_yaml_comment_opens_only_where_white_space_separates_it() {
    let source = b"# whole line\nurl: http://example.test/page#fragment\nplain: a#b # trailing\nkey: value\t# behind a tab\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let starts: Vec<usize> = report
        .comments
        .iter()
        .map(|comment| comment.span.start)
        .collect();
    assert_eq!(
        starts,
        [
            0,
            offset_of(source, b"# trailing"),
            offset_of(source, b"# behind a tab"),
        ],
        "{:?}",
        report.comments
    );
    assert!(
        report
            .comments
            .iter()
            .all(|comment| comment.kind == CommentKind::Line)
    );
    assert_eq!(removable(&report), 3);
    assert_eq!(report.comments[0].span.end, b"# whole line".len());
}

/// YAML 1.2.2, 7.3.1 and 7.3.2: a double-quoted scalar takes `\` escapes and a
/// single-quoted one takes `''` for a quote of its own, both may run over a
/// line break, and a `#` inside either is content. A quote only opens one
/// where a scalar may begin, so the apostrophe of a plain `it's` is a byte of
/// that scalar rather than the start of a literal that swallows the file.
#[test]
fn yaml_quoted_scalars_hide_comment_openers() {
    let source = b"a: \"x # not a comment\"\nb: 'it''s # not one either'\nc: \"first # line\n  second # line\"\nd: it's fine # yes\ne: [x,\"y # no\"] # also yes\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let starts: Vec<usize> = report
        .comments
        .iter()
        .map(|comment| comment.span.start)
        .collect();
    assert_eq!(
        starts,
        [
            offset_of(source, b"# yes"),
            offset_of(source, b"# also yes")
        ],
        "{:?}",
        report.comments
    );
    assert_eq!(removable(&report), 2);
}

/// YAML 1.2.2, 8.1 (Block Scalar Styles): `|` and `>` take an optional
/// indentation indicator and an optional chomping indicator, a comment may
/// follow the header on its own line, and the body is every following line
/// more indented than the parent node. Every `#` in that body is content.
#[test]
fn a_yaml_block_scalar_body_is_opaque_to_every_hash_in_it() {
    let source = b"literal: |\n  # not a comment\n  text\nfolded: >-\n  # not one either\nkeep: |+ # header comment\n  body # inside\nlist:\n  - |\n    # inside the entry\n  - plain # after the list\nindented: |2\n   # still inside\ndone: 1 # at the end\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let starts: Vec<usize> = report
        .comments
        .iter()
        .map(|comment| comment.span.start)
        .collect();
    assert_eq!(
        starts,
        [
            offset_of(source, b"# header comment"),
            offset_of(source, b"# after the list"),
            offset_of(source, b"# at the end"),
        ],
        "{:?}",
        report.comments
    );
    assert!(
        report
            .comments
            .iter()
            .all(|comment| comment.kind == CommentKind::Line)
    );
    assert_eq!(removable(&report), 3);
}

/// The body ends at the first line that is not more indented than the parent
/// and is not empty (YAML 1.2.2, 8.1.1.2: empty lines belong to the scalar),
/// or at a document marker in column zero (9.1.2 and 9.1.3), which is the only
/// thing that ends the body of a scalar that is the whole document.
#[test]
fn a_yaml_block_scalar_body_ends_at_an_outdent_or_a_document_marker() {
    let source = b"root: |\n  body # hidden\n\n  behind a blank line # hidden too\nnext: 1 # yes\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1, "{:?}", report.comments);
    assert_eq!(report.comments[0].span.start, offset_of(source, b"# yes"));

    for marker in [b"---".as_slice(), b"..."] {
        let mut document = b"|\n  a # hidden\n".to_vec();
        document.extend_from_slice(marker);
        document.extend_from_slice(b"\n# a real comment\n");
        let report = scan(&document, Language::Yaml, ScanOptions::default());
        assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
        assert_eq!(
            report.comments.len(),
            1,
            "{:?} left {:?}",
            String::from_utf8_lossy(marker),
            report.comments
        );
        assert_eq!(
            report.comments[0].span.start,
            offset_of(&document, b"# a real comment")
        );
    }
}

/// YAML 1.2.2, 8.1.1.1: the content of a block scalar is indented relative to
/// the indentation level of the node the scalar hangs off, never relative to
/// the column its `|` or `>` happens to sit in. The two differ whenever the
/// header is not the first token after its owner: node properties (6.9) may
/// come between them on the same line or on the one above, and the header may
/// sit on a line of its own, indented as deeply as it likes. A body line more
/// indented than the *owner* is content, so every `#` in it is content too.
#[test]
fn a_yaml_block_scalar_body_hangs_off_its_owner_not_the_header_column() {
    for source in [
        b"- |\n  # a\n  b\n".as_slice(),
        b"key: !!str |\n  # a\n",
        b"key: &x |\n  # a\n",
        b"key: &x !!str |\n  # a\n",
        b"? |\n  # a\n: v\n",
        b"- - |\n    # a\n",
        b"k: |2\n   # body\n",
        b"|\n # body\n",
        b"key:\n    |\n  # a\n",
        b"key: !!str\n  |\n  # a\n",
        b"!!str |\n # a\n",
    ] {
        let result = transform(source, Language::Yaml, TransformOptions::default());
        assert!(
            result.report.valid,
            "{:?} reported {:?}",
            String::from_utf8_lossy(source),
            result.report.diagnostics
        );
        assert!(
            result.report.comments.is_empty(),
            "{:?} found {:?}",
            String::from_utf8_lossy(source),
            result.report.comments
        );
        assert_eq!(
            result.output,
            source,
            "{:?} was rewritten to {:?}",
            String::from_utf8_lossy(source),
            String::from_utf8_lossy(&result.output)
        );
    }
}

/// The body ends at the first non-empty line that is *not* indented past the
/// owner. A line between the owner's indentation and the deeper indentation
/// the first body line set belongs to neither reading cleanly; taking it for
/// body is the reading that leaves bytes alone, so only the line that reaches
/// back to the owner's own depth ends the scalar.
#[test]
fn a_yaml_block_scalar_body_ends_at_the_owner_indentation() {
    let source = b"k:\n  - |\n    # a\n   # still body\n  # end\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1, "{:?}", report.comments);
    assert_eq!(report.comments[0].span.start, offset_of(source, b"# end"));
}

/// YAML 1.2.2, 8.1.1 and 8.1.1.2: a whole-line comment under a block scalar
/// body is `l-trail-comments` and is not part of the value, but every hole a
/// removal could leave on that line *is*. A line of spaces as wide as the
/// comment — what `columns` writes — is indented into the body it was
/// terminating; an empty line — what `lines` writes — is content under `|+`
/// and `>+`, which keep every empty line trailing a body. So the removal takes
/// the whole line, terminator included, under every layout and under every
/// chomping indicator.
#[test]
fn a_comment_trailing_a_yaml_block_scalar_takes_its_line_with_it() {
    let source = b"k: |+\n  body\n\n# after\nnext: 1 # yes\n";
    let expected: [(Layout, &[u8]); 3] = [
        (Layout::Lines, b"k: |+\n  body\n\nnext: 1 \n"),
        (Layout::Columns, b"k: |+\n  body\n\nnext: 1      \n"),
        (Layout::Compact, b"k: |+\n  body\n\nnext: 1\n"),
    ];
    for (layout, output) in expected {
        let result = transform(
            source,
            Language::Yaml,
            TransformOptions {
                layout,
                ..Default::default()
            },
        );
        assert_eq!(
            result.report.comments.len(),
            2,
            "{layout} found {:?}",
            result.report.comments
        );
        assert_eq!(
            result.output,
            output,
            "{layout} wrote {:?}",
            String::from_utf8_lossy(&result.output)
        );
    }
    /* NOTE: Clip and strip chomping drop the empty lines trailing a body, so
     * `lines` could leave a blank one there and change nothing. `columns`
     * could not: the padded line is as deep as the body and rejoins it. One
     * rule covers both, and all three layouts write the same bytes. */
    for header in [b"|".as_slice(), b"|-", b">", b">-", b"|+", b">+"] {
        for layout in Layout::ALL {
            let mut document = b"k: ".to_vec();
            document.extend_from_slice(header);
            document.extend_from_slice(b"\n  body\n\n# after\nnext: 1\n");
            let result = transform(
                &document,
                Language::Yaml,
                TransformOptions {
                    layout,
                    ..Default::default()
                },
            );
            let mut want = b"k: ".to_vec();
            want.extend_from_slice(header);
            want.extend_from_slice(b"\n  body\n\nnext: 1\n");
            assert_eq!(
                result.output,
                want,
                "{:?} under {layout} wrote {:?}",
                String::from_utf8_lossy(header),
                String::from_utf8_lossy(&result.output)
            );
        }
    }
}

/// The half of the rule that is about the lines the comment was *sheltering*.
///
/// Under `|+` and `>+` an empty line trailing a body is content
/// (YAML 1.2.2, 8.1.1.2) — but only until `l-trail-comments` begins, after
/// which every empty line is `l-comment` and belongs to nobody. Removing a
/// trail comment hands those lines back to the `+`, so the removal takes them
/// with it; the empty lines *above* the first comment were already content and
/// are left exactly where they were.
#[test]
fn a_keep_chomped_removal_takes_the_empty_lines_the_comment_was_sheltering() {
    let expected: [(&[u8], &[u8]); 6] = [
        /* NOTE: The blank run below the comment is separation while the
         * comment is there and content once it is gone, so it goes with the
         * comment. */
        (b"k: |+\n  a\n# c\n\nz: 1\n", b"k: |+\n  a\nz: 1\n"),
        (b"k: |+\n  a\n# c\n\n\nz: 1\n", b"k: |+\n  a\nz: 1\n"),
        /* NOTE: The blank run above it is already content: it stays where it
         * is. */
        (b"k: |+\n  a\n\n# c\n\nz: 1\n", b"k: |+\n  a\n\nz: 1\n"),
        (b"k: |+\n  a\n\n# c\nz: 1\n", b"k: |+\n  a\n\nz: 1\n"),
        /* NOTE: A comment that survives shelters what is under it, and the one
         * above it still takes the run it was sheltering. */
        (
            b"k: |+\n  a\n# c\n\n# yamllint disable\n\nz: 1\n",
            b"k: |+\n  a\n# yamllint disable\n\nz: 1\n",
        ),
        /* NOTE: Clip chomping keeps its blank lines: nothing was being
         * sheltered. */
        (b"k: |\n  a\n# c\n\nz: 1\n", b"k: |\n  a\n\nz: 1\n"),
    ];
    for (source, want) in expected {
        for layout in Layout::ALL {
            let result = transform(
                source,
                Language::Yaml,
                TransformOptions {
                    layout,
                    ..Default::default()
                },
            );
            assert_eq!(
                result.output,
                want,
                "{:?} under {layout} wrote {:?}",
                String::from_utf8_lossy(source),
                String::from_utf8_lossy(&result.output)
            );
        }
    }
}

/// The loose reading of a block scalar header — any `|` or `>` that ends its
/// line — is right for [`Layout`]-independent restart decisions and wrong
/// here: `k: a |+` is a plain scalar whose last two bytes look like a header,
/// and hanging a keep-chomped trail off it would take the line of a comment
/// that shelters nothing. Only a header the scanner itself recognised opens a
/// trail, so this comment is removed the ordinary way and keeps its line.
#[test]
fn a_pipe_inside_a_plain_yaml_scalar_opens_no_keep_chomped_trail() {
    let source = b"k: a |+\n# c\n\nz: 1\n";
    let result = transform(source, Language::Yaml, TransformOptions::default());
    assert_eq!(
        result.report.comments.len(),
        1,
        "{:?}",
        result.report.comments
    );
    assert_eq!(
        result.output,
        b"k: a |+\n\n\nz: 1\n",
        "wrote {:?}",
        String::from_utf8_lossy(&result.output)
    );
}

/// The keep reason the scanner writes for a trail comment a block scalar
/// cannot give up, repeated here so a rename has to be deliberate.
const STRUCTURAL: &str = "structural in a YAML block scalar trail";

/// Every layout writes the same bytes for `source`, and that is `want`.
fn yaml_layouts_write(source: &[u8], want: &[u8], options: ScanOptions) {
    for layout in Layout::ALL {
        let result = transform(
            source,
            Language::Yaml,
            TransformOptions {
                layout,
                scan: options.clone(),
            },
        );
        assert_eq!(
            result.output,
            want,
            "{:?} under {layout} wrote {:?}",
            String::from_utf8_lossy(source),
            String::from_utf8_lossy(&result.output)
        );
    }
}

/// YAML 1.2.2, 8.1.1: the comment that terminates a block scalar body is
/// sometimes the only thing standing between the body and a line that would
/// rejoin it.
///
/// `l-trail-comments` ends a body at the first line shallower than the
/// content, and the lines under *that* are the mapping's, whatever their
/// indentation. Take that first comment away — its whole line, which is the
/// most a removal can take — and the next surviving line is read against the
/// body again. A kept comment indented to the content depth is then content,
/// and the value grows a line that was never in it.
///
/// No removal preserves the value, so the comment is kept, and the reason says
/// what kept it: it is structure, not commentary.
#[test]
fn a_yaml_trail_comment_a_block_scalar_leans_on_is_kept() {
    let source = b"k: |\n  a\n# shallow\n  # yamllint disable\nz: 1\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert_eq!(report.comments.len(), 2, "found {:?}", report.comments);
    assert_eq!(
        report.comments[0].disposition,
        Disposition::Keep {
            reason: STRUCTURAL.to_owned()
        },
        "the shallow comment is what ends the body"
    );
    assert_eq!(
        report.comments[1].disposition,
        Disposition::Keep {
            reason: "tool or language directive".to_owned()
        }
    );
    yaml_layouts_write(source, source, ScanOptions::default());
}

/// The same shape under every chomping indicator, and with the empty line a
/// `+` body would have claimed sitting between the two comments: the removal
/// that takes a keep-chomped comment takes the blanks it was sheltering too,
/// which only brings the deeper comment up faster.
#[test]
fn a_structural_yaml_trail_comment_is_kept_under_every_chomping_indicator() {
    for header in [
        b"|".as_slice(),
        b"|-",
        b"|+",
        b">",
        b">-",
        b">+",
        b"|1",
        b"|1+",
    ] {
        for trail in [
            b"# shallow\n  # yamllint disable\n".as_slice(),
            b"# shallow\n\n  # yamllint disable\n",
            b"# shallow\n  # ordinary\n  # yamllint disable\n",
        ] {
            let mut source = b"k: ".to_vec();
            source.extend_from_slice(header);
            source.extend_from_slice(b"\n  a\n");
            source.extend_from_slice(trail);
            source.extend_from_slice(b"z: 1\n");
            let report = scan(&source, Language::Yaml, ScanOptions::default());
            assert_eq!(
                report.comments[0].disposition,
                Disposition::Keep {
                    reason: STRUCTURAL.to_owned()
                },
                "{:?}",
                String::from_utf8_lossy(&source)
            );
        }
    }
}

/// The rule is about the *content* indentation, not about the floor a body
/// line has to clear.
///
/// A body detects its indentation from its first non-empty line (8.1.1.1), so
/// a comment shallower than that ends the scalar wherever it sits under the
/// owner — and a removal above it changes nothing. Reading the floor instead
/// would keep a comment no value depends on.
#[test]
fn a_yaml_trail_comment_shallower_than_the_body_content_is_still_removable() {
    let expected: [(&[u8], &[u8]); 3] = [
        /* NOTE: The body is four deep, so the directive two deep is a comment
         * before the removal and a comment after it. */
        (
            b"k: |\n    a\n# shallow\n  # yamllint disable\nz: 1\n",
            b"k: |\n    a\n  # yamllint disable\nz: 1\n",
        ),
        /* NOTE: A sequence entry hangs its body off the `-`, so the floor is
         * one and the content is three. */
        (
            b"- |\n   a\n# shallow\n  # yamllint disable\n",
            b"- |\n   a\n  # yamllint disable\n",
        ),
        /* NOTE: An explicit indicator names the content depth outright, and a
         * body written deeper than it does not move it. */
        (
            b"k: |2\n    a\n# shallow\n # yamllint disable\nz: 1\n",
            b"k: |2\n    a\n # yamllint disable\nz: 1\n",
        ),
    ];
    for (source, want) in expected {
        let report = scan(source, Language::Yaml, ScanOptions::default());
        assert!(
            report.comments[0].disposition.is_remove(),
            "{:?} kept a comment no value leans on: {:?}",
            String::from_utf8_lossy(source),
            report.comments[0].disposition
        );
        yaml_layouts_write(source, want, ScanOptions::default());
    }
}

/// Nothing is kept for a trail whose every comment goes: with the deeper
/// comment removed too, the shallow one shelters nothing and both take their
/// lines.
#[test]
fn a_yaml_trail_whose_deeper_comment_is_removable_keeps_neither() {
    yaml_layouts_write(
        b"k: |\n  a\n# shallow\n  # deep\nz: 1\n",
        b"k: |\n  a\nz: 1\n",
        ScanOptions::default(),
    );
    /* NOTE: `all` removes the directive as well, which is what turns the first
     * shape of this file back into an ordinary pair of removals. */
    yaml_layouts_write(
        b"k: |\n  a\n# shallow\n  # yamllint disable\nz: 1\n",
        b"k: |\n  a\nz: 1\n",
        ScanOptions {
            policy: Policy::All,
            ..Default::default()
        },
    );
}

/// `--policy all` is not a way out of this one. The rule is about what
/// survives the run, so a comment an override keeps still holds the body open,
/// and the comment above it is still what closes it.
#[test]
fn a_structural_yaml_trail_keep_outlives_policy_all() {
    let options = ScanOptions {
        policy: Policy::All,
        keep_regex: vec!["KEEPME".to_owned()],
        ..Default::default()
    };
    let source = b"k: |\n  a\n# shallow\n  # KEEPME\nz: 1\n";
    let report = scan(source, Language::Yaml, options.clone());
    assert_eq!(
        report.comments[0].disposition,
        Disposition::Keep {
            reason: STRUCTURAL.to_owned()
        }
    );
    yaml_layouts_write(source, source, options);
}

/// A body hanging off a nested key, and the same shape one column shallower.
/// The owner's column decides the floor and the first body line decides the
/// content, so both have to be read off the scan rather than off the header.
#[test]
fn a_structural_yaml_trail_keep_follows_a_nested_owner() {
    let source = b"outer:\n  inner: |\n    x\n  # shallow\n    # yamllint disable\nz: 1\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert_eq!(
        report.comments[0].disposition,
        Disposition::Keep {
            reason: STRUCTURAL.to_owned()
        }
    );
    yaml_layouts_write(source, source, ScanOptions::default());
    /* NOTE: One column shallower than the content and the directive is a
     * comment on both sides of the removal. */
    yaml_layouts_write(
        b"outer:\n  inner: |\n    x\n  # shallow\n   # yamllint disable\nz: 1\n",
        b"outer:\n  inner: |\n    x\n   # yamllint disable\nz: 1\n",
        ScanOptions::default(),
    );
}

/// CRLF changes where the lines end and nothing else: the same trail is read
/// the same way and the same comment is kept.
#[test]
fn a_structural_yaml_trail_keep_survives_crlf_line_endings() {
    let source = b"k: |\r\n  a\r\n# shallow\r\n  # yamllint disable\r\nz: 1\r\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert_eq!(
        report.comments[0].disposition,
        Disposition::Keep {
            reason: STRUCTURAL.to_owned()
        }
    );
    yaml_layouts_write(source, source, ScanOptions::default());
}

/// A kept comment shallower than the content closes the body on its own, so
/// the comment above it is ordinary and goes.
#[test]
fn a_kept_yaml_trail_comment_shallower_than_the_content_shields_the_rest() {
    yaml_layouts_write(
        b"k: |\n  a\n# shallow\n# yamllint disable\n  # deeper\nz: 1\n",
        b"k: |\n  a\n# yamllint disable\nz: 1\n",
        ScanOptions::default(),
    );
}

/// A quoted scalar that no closing quote ends runs to the end of the file and
/// is an error, so nothing is edited until `force_invalid` says to edit what
/// is known anyway.
#[test]
fn every_unterminated_yaml_construct_stops_a_fix_until_it_is_forced() {
    for source in [
        b"a: \"unclosed # not a comment\n".as_slice(),
        b"a: 'unclosed # not a comment\n".as_slice(),
        b"a: \"first line\n  second line\n".as_slice(),
    ] {
        let result = transform(source, Language::Yaml, TransformOptions::default());
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
        assert!(
            result.report.comments.is_empty(),
            "{:?} found {:?}",
            String::from_utf8_lossy(source),
            result.report.comments
        );
    }
    let forced = transform(
        b"# note\na: \"unclosed\n",
        Language::Yaml,
        TransformOptions {
            scan: ScanOptions {
                force_invalid: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(!forced.report.valid);
    assert_eq!(forced.output, b"\na: \"unclosed\n");
}

/// Every marker a tool reads out of a YAML comment is kept where an ordinary
/// comment is removed.
#[test]
fn yaml_tool_directives_are_protected() {
    let source = b"# yaml-language-server: $schema=https://example.test/schema.json\n\
# yamllint disable-line rule:line-length\n\
# renovate: datasource=docker depName=alpine\n\
# checkov:skip=CKV_AWS_20:public by design\n\
# trivy:ignore:AVD-AWS-0089\n\
# nosec\n\
# kics-scan ignore-line\n\
# @schema type: string\n\
# prettier-ignore\n\
# noqa\n\
# ordinary\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let kinds: Vec<CommentKind> = report.comments.iter().map(|comment| comment.kind).collect();
    assert_eq!(
        kinds,
        [
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Line,
        ],
        "{:?}",
        report.comments
    );
    assert_eq!(removable(&report), 1);
    /* NOTE: Each of these is the whole word the tool answers to, so prose that
     * merely runs letters on past it is not addressed to the tool and is an
     * ordinary comment. */
    let prose =
        b"# yamllintish note\n# noseclike note\n# kics-scanning note\n# a note about @schema\n";
    let report = scan(prose, Language::Yaml, ScanOptions::default());
    assert_eq!(removable(&report), 4, "{:?}", report.comments);
}

/// The `#!` rule is the file's own preamble rule and belongs to the first byte
/// of the first line: an interpreter line further down is an ordinary comment.
#[test]
fn a_yaml_hash_bang_line_is_a_preamble_only_at_the_first_byte() {
    let source = b"#!/usr/bin/env ansible-playbook\nkey: 1 # yes\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 2, "{:?}", report.comments);
    assert_eq!(report.comments[0].kind, CommentKind::Shebang);
    assert_eq!(removable(&report), 1);

    let later = b"key: 1\n#!/usr/bin/env ansible-playbook\n";
    let report = scan(later, Language::Yaml, ScanOptions::default());
    assert_eq!(report.comments.len(), 1, "{:?}", report.comments);
    assert_eq!(report.comments[0].kind, CommentKind::Line);
    assert_eq!(removable(&report), 1);
}

/// Every construct that runs over a line break reads a CRLF pair as the one
/// line break YAML 1.2.2, 5.4 says it is.
#[test]
fn yaml_multi_line_constructs_survive_crlf_line_endings() {
    let source = b"a: \"first # no\r\n  second # no\"\r\nb: 'x # no\r\n  y'\r\nblock: |\r\n  # no\r\n  body\r\nc: 1 # yes\r\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1, "{:?}", report.comments);
    assert_eq!(report.comments[0].span.start, offset_of(source, b"# yes"));
    assert_eq!(report.comments[0].span.end, source.len() - b"\r\n".len());
}

/// A tab is white space, so it separates a comment from the token in front of
/// it, but it is never indentation (YAML 1.2.2, 6.1), so a tab inside the
/// white space of a block scalar body line is content of that body.
#[test]
fn a_yaml_tab_separates_a_comment_but_never_indents_a_line() {
    let source = b"block: |\n  \t# still the body\n  text\nafter: 1\t# yes\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1, "{:?}", report.comments);
    assert_eq!(report.comments[0].span.start, offset_of(source, b"# yes"));
}

/// A `%` directive line and a flow collection carry no comment rule of their
/// own: what separates a comment from a token separates it there too.
#[test]
fn yaml_directive_lines_and_flow_collections_keep_the_one_comment_rule() {
    let source = b"%YAML 1.2\n%TAG !e! tag:example.test,2000:app/\n---\nflow: [a, \"b # no\", 'c # no'] # yes\nmap: {x: 1} # also yes\n";
    let report = scan(source, Language::Yaml, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let starts: Vec<usize> = report
        .comments
        .iter()
        .map(|comment| comment.span.start)
        .collect();
    assert_eq!(
        starts,
        [
            offset_of(source, b"# yes"),
            offset_of(source, b"# also yes")
        ],
        "{:?}",
        report.comments
    );
}

#[test]
fn yaml_is_detected_from_its_extensions_and_reserved_names() {
    for name in ["ci.yml", "docker-compose.yaml", ".pre-commit-config.yaml"] {
        let found = detect_language(Some(Path::new(name)), b"")
            .unwrap_or_else(|| panic!("`{name}` is detected as nothing"));
        assert_eq!(
            (found.language, found.dialect, found.reason),
            (Language::Yaml, Dialect::Standard, "extension"),
            "`{name}`"
        );
    }
    for name in [".clang-format", ".clang-tidy", ".yamllint"] {
        let found = detect_language(Some(Path::new(name)), b"")
            .unwrap_or_else(|| panic!("`{name}` is detected as nothing"));
        assert_eq!(
            (found.language, found.dialect, found.reason),
            (Language::Yaml, Dialect::Standard, "reserved-filename"),
            "`{name}`"
        );
    }
    /* NOTE: The reserved names are the ones a project writes with no extension
     * at all; a file called `clang-format` is a program rather than one of
     * them. */
    assert!(detect_language(Some(Path::new("clang-format")), b"").is_none());
}

#[test]
fn yaml_layouts_leave_a_line_columns_or_nothing() {
    let source = b"# alone\nkey: 1 # trailing\n";
    let lines = transform(source, Language::Yaml, TransformOptions::default());
    assert_eq!(lines.output, b"\nkey: 1 \n");
    let columns = transform(
        source,
        Language::Yaml,
        TransformOptions {
            layout: Layout::Columns,
            ..Default::default()
        },
    );
    let padded = format!("{}\nkey: 1 {}\n", " ".repeat(7), " ".repeat(10));
    assert_eq!(columns.output, padded.as_bytes());
    let compact = transform(
        source,
        Language::Yaml,
        TransformOptions {
            layout: Layout::Compact,
            ..Default::default()
        },
    );
    assert_eq!(compact.output, b"key: 1\n");
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

/// The diagnostic for an unterminated literal names the construct that was
/// left open, per language, rather than saying `literal` and leaving a reader
/// to guess. The message is user-facing, so it is pinned here as well as by
/// the byte-for-byte differential comparison against the OCaml reference —
/// `spec/fixtures/v1`'s recorded blocks hold the code and the span but not the
/// text.
#[test]
fn an_unterminated_literal_is_named_after_its_construct() {
    let cases: &[(Language, &[u8], &str)] = &[
        (
            Language::Rust,
            b"let s = \"unclosed // not a comment\n",
            "unterminated string",
        ),
        // NOTE: `'x ` is a lifetime, not a literal, so the character literal
        // NOTE: that fails here has to be one `rust_char_start` accepts: a
        // NOTE: non-ASCII character with a `'` close enough behind it to look
        // NOTE: like the closing quote, which the line break arrives before.
        (
            Language::Rust,
            "let c = '\u{e4}\n';\n".as_bytes(),
            "unterminated character literal",
        ),
        (
            Language::Go,
            b"s := \"unclosed // not a comment\n",
            "unterminated string or rune literal",
        ),
        (
            Language::Go,
            b"r := 'x // not a comment\n",
            "unterminated string or rune literal",
        ),
        (
            Language::Css,
            b"a::before { content: \"unclosed\n",
            "unterminated CSS string",
        ),
        (
            Language::Css,
            b"a::before { content: 'unclosed\n",
            "unterminated CSS string",
        ),
        (
            Language::Jsonc,
            b"{ \"key: 1 }\n",
            "unterminated JSON string",
        ),
        (
            Language::Jsonc,
            b"{ 'key: 1 }\n",
            "unterminated JSON string",
        ),
        (
            Language::Kotlin,
            b"val c = 'x // not a comment\n",
            "unterminated Kotlin character literal",
        ),
        (
            Language::C,
            b"const char *s = \"unclosed\n",
            "unterminated string or character literal",
        ),
        (
            Language::Rust,
            b"let s = r\"unclosed\n",
            "unterminated Rust raw string",
        ),
        (
            Language::Yaml,
            b"key: \"unclosed # not a comment\n",
            "unterminated YAML double-quoted scalar",
        ),
        (
            Language::Yaml,
            b"key: 'unclosed # not a comment\n",
            "unterminated YAML single-quoted scalar",
        ),
    ];
    for (language, source, message) in cases {
        let report = scan(source, *language, ScanOptions::default());
        assert!(!report.valid, "{language:?} {source:?} should not be valid");
        assert_eq!(
            report
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>(),
            vec![*message],
            "{language:?} {source:?}",
        );
        assert_eq!(report.diagnostics[0].code, "unterminated-string");
    }
}

/// JSON5 4.4 lets a string be written with apostrophes, and the `jsonc`
/// language is documented as `JSON with comments, including JSON5` and owns
/// the `.json5` extension, so `'` opens a string here exactly as `"` does. A
/// `//` inside one is content.
#[test]
fn jsonc_reads_a_single_quoted_json5_string() {
    let source = b"{ 'note': '// not a comment', \"other\": 1 } // remove\n";
    let report = scan(source, Language::Jsonc, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        source.len() - b"// remove\n".len()
    );
}

/// Rust Reference, Lifetimes and loop labels: `'` followed by an identifier
/// that no second `'` closes is a lifetime and opens no literal, so a line
/// comment after one on the same line is a comment.
#[test]
fn a_rust_lifetime_opens_no_character_literal() {
    let source = b"let r: &'a str = s; // remove\n";
    let report = scan(source, Language::Rust, ScanOptions::default());
    assert!(report.valid);
    assert!(report.diagnostics.is_empty());
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span,
        ByteSpan::new(source.len() - b"// remove\n".len(), source.len() - 1)
    );
}

/// Rust Reference, raw string literals: `r"` and `r#"` close only at their own
/// closer, so one that never closes runs to the end of the file and is an
/// error spanning what the lexer consumed.
#[test]
fn an_unterminated_rust_raw_string_is_an_error_to_the_end_of_the_file() {
    for source in [
        b"let s = r\"unclosed\n".as_slice(),
        b"let s = r#\"unclosed // not a comment\n".as_slice(),
    ] {
        let report = scan(source, Language::Rust, ScanOptions::default());
        assert!(!report.valid, "{source:?}");
        assert!(report.comments.is_empty(), "{source:?}");
        assert_eq!(report.diagnostics.len(), 1, "{source:?}");
        assert_eq!(
            report.diagnostics[0].span,
            ByteSpan::new(b"let s = ".len(), source.len()),
            "{source:?}",
        );
    }
}

/// A UTF-8 BOM is consumed before the first line is read — CPython's
/// `check_bom`, Lua's `skipBOM` — so a `#!` line behind one is still the first
/// line and still a preamble. A shell is the exception: `#` opens a comment
/// only where no word has begun, and the BOM bytes begin one.
#[test]
fn a_byte_order_mark_does_not_hide_the_first_line() {
    let python = b"\xef\xbb\xbf#!/usr/bin/env python3\nx = 1\n";
    let report = scan(python, Language::Python, ScanOptions::default());
    assert_eq!(report.comments.len(), 1);
    assert_eq!(report.comments[0].kind, CommentKind::Shebang);
    assert!(!report.comments[0].disposition.is_remove());
    assert_eq!(
        transform(python, Language::Python, TransformOptions::default()).output,
        python
    );

    let lua = b"\xef\xbb\xbf#!/usr/bin/lua\nx = 1\n";
    let report = scan(lua, Language::Lua, ScanOptions::default());
    assert_eq!(report.comments.len(), 1);
    assert_eq!(report.comments[0].kind, CommentKind::Shebang);
    assert_eq!(report.comments[0].span, ByteSpan::new(3, lua.len() - 7));
    assert_eq!(
        transform(lua, Language::Lua, TransformOptions::default()).output,
        lua
    );

    let lua_comment = b"\xef\xbb\xbf# not a shebang\nx = 1\n";
    let report = scan(lua_comment, Language::Lua, ScanOptions::default());
    assert_eq!(report.comments.len(), 1);
    assert_eq!(report.comments[0].kind, CommentKind::Line);
    assert!(report.comments[0].disposition.is_remove());

    let shell = b"\xef\xbb\xbf#!/bin/sh\necho 1\n";
    let report = scan(shell, Language::Shell, ScanOptions::default());
    assert!(report.comments.is_empty());
}

/// A directive named after the tool that reads it ends at a boundary, and the
/// end of the comment is one: the argument is merely missing, and a bare
/// keyword is the instruction it is about to become. Running letters on past
/// the keyword is still prose.
#[test]
fn a_keyword_directive_survives_a_missing_argument() {
    let kept: &[(Language, &[u8])] = &[
        (Language::Toml, b"#:schema\nkey = 1\n"),
        (Language::Shell, b"# shellcheck\ncat x\n"),
        (Language::Shell, b"# hadolint\nRUN true\n"),
    ];
    for (language, source) in kept {
        let report = scan(source, *language, ScanOptions::default());
        assert_eq!(report.comments.len(), 1, "{source:?}");
        assert_eq!(
            report.comments[0].kind,
            CommentKind::Directive,
            "{source:?}"
        );
        assert!(!report.comments[0].disposition.is_remove(), "{source:?}");
    }
    let removed: &[(Language, &[u8])] = &[
        (Language::Toml, b"#:schemata are plural\nkey = 1\n"),
        (Language::Shell, b"# shellcheckish note\ncat x\n"),
    ];
    for (language, source) in removed {
        let report = scan(source, *language, ScanOptions::default());
        assert_eq!(report.comments.len(), 1, "{source:?}");
        assert!(report.comments[0].disposition.is_remove(), "{source:?}");
    }
}

/// ECMA-262 12.2 counts <VT> U+000B as `WhiteSpace`, which
/// [`u8::is_ascii_whitespace`] does not. Reading it as an ordinary character
/// makes `a\u{b}<div>` look like the start of a JSX element instead of a
/// comparison, and the element then swallows the rest of the file.
#[test]
fn javascript_reads_a_vertical_tab_as_whitespace() {
    for (language, dialect) in [
        (Language::JavaScript, Dialect::Jsx),
        (Language::TypeScript, Dialect::Tsx),
    ] {
        let source = b"let ok = a\x0b<b; // remove\n";
        let report = scan(source, language, options(dialect));
        assert!(report.valid, "{language:?}");
        assert!(report.diagnostics.is_empty(), "{language:?}");
        assert_eq!(report.comments.len(), 1, "{language:?}");
        assert_eq!(
            report.comments[0].span.start,
            source.len() - b"// remove\n".len(),
            "{language:?}",
        );
    }
    // NOTE: ECMA-262 B.1.3: `-->` opens a comment where only whitespace
    // NOTE: precedes it on the line, and a vertical tab is whitespace there too.
    let source = b"\x0b--> remove\nlet x = 1;\n";
    let report = scan(source, Language::JavaScript, ScanOptions::default());
    assert_eq!(report.comments.len(), 1);
    assert_eq!(report.comments[0].span, ByteSpan::new(1, 11));
}

/// POSIX Shell Command Language 2.7.4: a here-document delimiter is a word, and
/// a word ends at an unquoted operator character. `>` is one, so `<<EOF>out` is
/// a here-document named `EOF` and a redirection — not a delimiter `EOF>out`
/// that no line ever matches, which would swallow the rest of the file.
#[test]
fn a_shell_heredoc_delimiter_ends_at_a_redirection() {
    let source = b"cat <<EOF>out\ndata\nEOF\n# remove\n";
    let report = scan(source, Language::Shell, ScanOptions::default());
    assert!(report.valid);
    assert!(report.diagnostics.is_empty());
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        source.len() - b"# remove\n".len()
    );
}

/// C++ [lex.string]: a d-char is any member of the basic source character set
/// except space, `(`, `)`, `\`, and the control characters horizontal tab,
/// vertical tab, form feed and new-line. A vertical tab in the delimiter makes
/// this no raw string, so the quote opens an ordinary literal instead —
/// [`u8::is_ascii_whitespace`] would have let the raw string through.
///
/// [`lex.token`] adds the other half: `R"` opens a raw string only where it
/// opens a token, so letters running into it leave an identifier and a plain
/// string literal behind.
#[test]
fn a_cpp_raw_string_needs_its_delimiter_and_its_boundary() {
    let vertical_tab = b"R\"a\x0bb(x\ny)a\x0bb\" // remove\n";
    let report = scan(vertical_tab, Language::Cpp, ScanOptions::default());
    assert!(!report.valid);
    assert!(report.comments.is_empty());
    assert_eq!(report.diagnostics.len(), 2);

    let identifier = b"aR\"(x)\" // remove\n";
    let report = scan(identifier, Language::Cpp, ScanOptions::default());
    assert!(report.valid);
    assert_eq!(report.comments.len(), 1);
    assert_eq!(
        report.comments[0].span.start,
        identifier.len() - b"// remove\n".len()
    );
}

/// PHP manual, Language Reference → Basic syntax → Escaping from HTML: a file
/// opens in inline-HTML mode, `<?php` followed by white space or the end of
/// the file enters PHP mode, `<?=` is the short echo tag and enters it too,
/// and `?>` leaves it again. `short_open_tag` is off by default, so a bare
/// `<?` is inline text and an XML declaration opens nothing; everything in
/// inline HTML is content, comment markers included.
#[test]
fn php_opens_code_only_at_a_real_opening_tag() {
    let source = b"<?xml version=\"1.0\" ?>\n<p># not a comment</p>\n<?php echo 1; // remove\n?>\n<?= $x // also remove\n?>\n<p>/* still html */</p>\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let starts: Vec<usize> = report
        .comments
        .iter()
        .map(|comment| comment.span.start)
        .collect();
    assert_eq!(
        starts,
        [
            offset_of(source, b"// remove"),
            offset_of(source, b"// also remove"),
        ],
        "{:?}",
        report.comments
    );
    assert_eq!(removable(&report), 2);
}

/// PHP manual, Comments: a `//` or `#` comment ends at the end of the line or
/// at the closing tag, whichever comes first, and the `?>` is not part of it.
/// The closing tag then carries one line break away with it, which is what
/// keeps a template from emitting a blank line for every block of code.
#[test]
fn a_php_close_tag_ends_a_line_comment_and_swallows_one_newline() {
    let source = b"<?php // note ?>tail\n<?php # hash ?>\n<p>x</p>\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 2, "{:?}", report.comments);
    assert_eq!(
        report.comments[0].span,
        ByteSpan::new(6, offset_of(source, b"?>tail"))
    );
    assert_eq!(
        report.comments[1].span,
        ByteSpan::new(
            offset_of(source, b"# hash"),
            offset_of(source, b"?>\n<p>x</p>")
        )
    );
    let stripped = transform(source, Language::Php, TransformOptions::default());
    assert_eq!(stripped.output, b"<?php ?>tail\n<?php ?>\n<p>x</p>\n");
}

/// PHP 8.0 gave `#[` to attributes (PHP manual, Attributes → Attribute
/// syntax), so a `#` with a bracket behind it opens no comment at all — and
/// the string inside the attribute is an ordinary string.
#[test]
fn a_php_attribute_is_not_a_comment() {
    let source = b"<?php\n#[Attribute(\"# not a comment\")]\nclass A {}\n# remove\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1, "{:?}", report.comments);
    assert_eq!(
        report.comments[0].span.start,
        offset_of(source, b"# remove")
    );
}

/// The tokenizer makes a documentation comment of `/**` only when white space
/// follows it (`zend_language_scanner.l`: `"/*"|"/**"{WHITESPACE}`), so `/**/`
/// and `/**text*/` are ordinary block comments. PHP has no `///` and no `/*!`
/// documentation form either, so both of those are ordinary comments too.
#[test]
fn php_comment_forms_carry_their_kinds() {
    let source =
        b"<?php\n/** doc */\n/**/\n/**not doc*/\n/*! not doxygen */\n/* block */\n// line\n/// line too\n# hash\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let kinds: Vec<CommentKind> = report.comments.iter().map(|comment| comment.kind).collect();
    assert_eq!(
        kinds,
        [
            CommentKind::DocBlock,
            CommentKind::Block,
            CommentKind::Block,
            CommentKind::Block,
            CommentKind::Block,
            CommentKind::Line,
            CommentKind::Line,
            CommentKind::Line,
        ],
        "{:?}",
        report.comments
    );
    assert_eq!(removable(&report), 8);
}

/// PHP manual, Strings: a single-quoted string escapes only `\'` and `\\`, a
/// double-quoted one takes the full escape set and interpolates, a backtick
/// string is the execution operator, and a heredoc and a nowdoc carry their
/// body verbatim. Every comment opener inside any of them is content, and the
/// braces of `{$...}` are opaque as well.
#[test]
fn php_strings_and_heredocs_hide_comment_openers() {
    let source = b"<?php\n$a = 'it\\'s // not a comment';\n$b = \"x # not one {$y /* nor this */} z\";\n$c = `ls // no`;\n$d = <<<EOT\n// not a comment\nEOT;\n$e = <<<'NOW'\n# not one either\nNOW;\n// remove\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1, "{:?}", report.comments);
    assert_eq!(
        report.comments[0].span.start,
        offset_of(source, b"// remove")
    );
}

/// PHP manual, Strings → Complex (curly) syntax: `{$...}` holds an expression,
/// and the two things inside one that can carry a brace of their own are a
/// nested string and a comment. Counting braces without them would end the
/// interpolation early, close the string at the next quote, and read the rest
/// of a perfectly valid file as code.
#[test]
fn a_php_interpolation_ends_where_its_own_braces_balance() {
    let source =
        b"<?php\n$a = \"x{$b['}']}y\"; // remove\n$c = \"x{$d /* } */}y\"; // remove too\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let starts: Vec<usize> = report
        .comments
        .iter()
        .map(|comment| comment.span.start)
        .collect();
    assert_eq!(
        starts,
        [
            offset_of(source, b"// remove"),
            offset_of(source, b"// remove too"),
        ],
        "{:?}",
        report.comments
    );
}

/// PHP manual, Heredoc text: the body ends at the first line whose first
/// non-blank content is the label and whose next byte cannot continue a label.
/// Since PHP 7.3 that line may be indented and the label may be followed by
/// `;`, `,`, `)`, or the line ending. A nowdoc quotes the label with
/// apostrophes and a heredoc may quote it with `"`.
#[test]
fn a_php_heredoc_ends_only_at_its_own_label() {
    let source = b"<?php\n$a = <<<EOT\n  body\n  EOT;\n$b = <<<\"HTML\"\n<div><!-- x --></div>\nHTML;\n$c = f(<<<'X'\ntext\nX);\n$d = <<<EOT\nEOTX is not the end\nEOT\n// remove\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 1, "{:?}", report.comments);
    assert_eq!(
        report.comments[0].span.start,
        offset_of(source, b"// remove")
    );
}

/// A `?>` inside a string, a heredoc, or a block comment is bytes of that
/// construct: only one the scanner meets in code leaves PHP mode.
#[test]
fn a_php_close_tag_inside_a_literal_leaves_no_code() {
    let source = b"<?php $s = \"?> not html\"; $t = '?>'; /* ?> */ echo <<<EOT\n?> still the body\nEOT;\n// remove\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let starts: Vec<usize> = report
        .comments
        .iter()
        .map(|comment| comment.span.start)
        .collect();
    assert_eq!(
        starts,
        [
            offset_of(source, b"/* ?> */"),
            offset_of(source, b"// remove")
        ],
        "{:?}",
        report.comments
    );
}

/// The CLI strips a `#!` line from the very first line of a script before the
/// engine sees it, so that line is a preamble rather than the inline HTML the
/// rest of the file opens as. One anywhere else is inline HTML like every
/// other byte of it.
#[test]
fn a_php_hash_bang_line_is_a_preamble_only_at_the_first_byte() {
    let source = b"#!/usr/bin/env php\n<?php // remove\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 2, "{:?}", report.comments);
    assert_eq!(report.comments[0].kind, CommentKind::Shebang);
    assert!(!report.comments[0].disposition.is_remove());
    assert_eq!(removable(&report), 1);
    assert_eq!(
        transform(source, Language::Php, TransformOptions::default()).output,
        b"#!/usr/bin/env php\n<?php \n"
    );

    let later = b"<p>x</p>\n#!/usr/bin/env php\n<?php // remove\n";
    let report = scan(later, Language::Php, ScanOptions::default());
    assert_eq!(report.comments.len(), 1, "{:?}", report.comments);
    assert_eq!(report.comments[0].kind, CommentKind::Line);
}

/// Every construct that runs over a line break reads a CRLF pair as one line
/// ending, the closing tag included.
#[test]
fn php_multi_line_constructs_survive_crlf_line_endings() {
    let source =
        b"<?php // note\r\n/* multi\r\nline */\r\n$s = <<<EOT\r\n// body\r\nEOT;\r\n?>\r\n<p>x</p>\r\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.comments.len(), 2, "{:?}", report.comments);
    assert_eq!(
        report.comments[0].span.end,
        offset_of(source, b"\r\n/* multi")
    );
    assert_eq!(
        report.comments[1].span,
        ByteSpan::new(
            offset_of(source, b"/* multi"),
            offset_of(source, b"\r\n$s = ")
        )
    );
}

/// A block comment, a string, a heredoc, and a nowdoc that no closer ends run
/// to the end of the file and are errors, so nothing is edited until
/// `force_invalid` says to edit what is known anyway.
#[test]
fn every_unterminated_php_construct_stops_a_fix_until_it_is_forced() {
    let cases: &[(&[u8], &str, &str)] = &[
        (
            b"<?php /* unclosed // not a comment\n",
            "unterminated-comment",
            "unterminated PHP block comment",
        ),
        (
            b"<?php $s = 'unclosed // not a comment\n",
            "unterminated-string",
            "unterminated PHP single-quoted string",
        ),
        (
            b"<?php $s = \"unclosed // not a comment\n",
            "unterminated-string",
            "unterminated PHP double-quoted string",
        ),
        (
            b"<?php $s = `unclosed // not a comment\n",
            "unterminated-string",
            "unterminated PHP backtick string",
        ),
        (
            b"<?php $s = <<<EOT\n// not a comment\n",
            "unterminated-string",
            "unterminated PHP heredoc",
        ),
        (
            b"<?php $s = <<<'NOW'\n# not a comment\n",
            "unterminated-string",
            "unterminated PHP nowdoc",
        ),
    ];
    for (source, code, message) in cases {
        let result = transform(source, Language::Php, TransformOptions::default());
        assert!(!result.report.valid, "{source:?} was accepted");
        assert_eq!(
            result
                .report
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code.as_str(), diagnostic.message.as_str()))
                .collect::<Vec<_>>(),
            vec![(*code, *message)],
            "{source:?}",
        );
        assert!(result.edits.is_empty(), "{source:?} was edited anyway");
    }
    let forced = transform(
        b"<?php // note\n$s = 'unclosed\n",
        Language::Php,
        TransformOptions {
            scan: ScanOptions {
                force_invalid: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(!forced.report.valid);
    assert_eq!(forced.output, b"<?php \n$s = 'unclosed\n");
}

/// Every marker a PHP tool reads out of a comment is kept where an ordinary
/// comment is removed.
#[test]
fn php_tool_directives_are_protected() {
    let source = b"<?php\n// phpcs:ignore Squiz.Commenting.FunctionComment\n# phpcs:disable\n// @phpstan-ignore-next-line\n/** @psalm-suppress InvalidReturnType */\n// @codeCoverageIgnoreStart\n// noinspection PhpUnusedLocalVariableInspection\n// ordinary\n";
    let report = scan(source, Language::Php, ScanOptions::default());
    assert!(report.valid, "diagnostics: {:?}", report.diagnostics);
    let kinds: Vec<CommentKind> = report.comments.iter().map(|comment| comment.kind).collect();
    assert_eq!(
        kinds,
        [
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Directive,
            CommentKind::Line,
        ],
        "{:?}",
        report.comments
    );
    assert_eq!(removable(&report), 1);
    /* NOTE: `phpcs:` and `@psalm-suppress` end at a boundary, so prose that
     * merely runs letters on past either of them is an ordinary comment. */
    let prose = b"<?php\n// phpcsish note\n// @psalm-suppressish note\n";
    let report = scan(prose, Language::Php, ScanOptions::default());
    assert_eq!(removable(&report), 2, "{:?}", report.comments);
}

#[test]
fn php_is_detected_from_its_extensions_and_its_shebang() {
    for name in ["index.php", "page.phtml", "case.phpt"] {
        let found = detect_language(Some(Path::new(name)), b"")
            .unwrap_or_else(|| panic!("`{name}` is detected as nothing"));
        assert_eq!(
            (found.language, found.dialect, found.reason),
            (Language::Php, Dialect::Standard, "extension"),
            "`{name}`"
        );
    }
    let piped = detect_language(None, b"#!/usr/bin/env php\n<?php\n")
        .expect("a `#!` line naming php is detected");
    assert_eq!(
        (piped.language, piped.reason),
        (Language::Php, "shebang"),
        "{piped:?}"
    );
}

#[test]
fn php_layouts_leave_a_line_columns_or_nothing() {
    let source = b"<?php\n// alone\n$x = 1; // trailing\n";
    let lines = transform(source, Language::Php, TransformOptions::default());
    assert_eq!(lines.output, b"<?php\n\n$x = 1; \n");
    let columns = transform(
        source,
        Language::Php,
        TransformOptions {
            layout: Layout::Columns,
            ..Default::default()
        },
    );
    let padded = format!("<?php\n{}\n$x = 1; {}\n", " ".repeat(8), " ".repeat(11));
    assert_eq!(columns.output, padded.as_bytes());
    let compact = transform(
        source,
        Language::Php,
        TransformOptions {
            layout: Layout::Compact,
            ..Default::default()
        },
    );
    assert_eq!(compact.output, b"<?php\n$x = 1;\n");
}
