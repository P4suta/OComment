use ocomment_core::{
    ByteSpan, CommentKind, Dialect, Disposition, Language, Layout, Policy, ScanOptions,
    TransformOptions, scan, transform,
};

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
#[test]
fn dockerfile_parser_and_linter_directives_are_protected() {
    let source = b"# syntax=docker/dockerfile:1\n# explanatory\n# hadolint ignore=DL3018\nRUN apk add --no-cache musl-dev\n# shellcheck disable=SC2086\n";
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
        ]
    );
    assert_eq!(removable(&report), 1);
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
