use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::Path,
    process::{Command, ExitStatus, Output, Stdio},
};
use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ocomment")
}

fn run(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(binary())
        .current_dir(directory)
        .env("PATH", "/usr/bin:/bin")
        .args(arguments)
        .output()
        .unwrap()
}

/// Run the binary with `input` piped to its standard input.
fn run_stdin(directory: &Path, arguments: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(binary())
        .current_dir(directory)
        .env("PATH", "/usr/bin:/bin")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .expect("standard input was piped")
        .write_all(input)
        .unwrap();
    child.wait_with_output().unwrap()
}

fn git(directory: &Path, arguments: &[&str]) -> Vec<u8> {
    let output = Command::new("/usr/bin/git")
        .current_dir(directory)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[cfg(unix)]
fn git_with_path(directory: &Path, arguments: &[&str], path: &std::ffi::OsStr) -> Vec<u8> {
    let output = Command::new("/usr/bin/git")
        .current_dir(directory)
        .args(arguments)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// What a file OComment has no built-in scanner for is skipped with. The
/// sentence is pinned literally by `an_unknown_language_skip_says_how_to_force_one`;
/// every other test names it through this constant.
const NO_LANGUAGE: &str =
    "no built-in language for this file (see `ocomment languages`; use --language to force)";

fn repository() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "-q"]);
    let hooks = directory.path().join(".git/no-hooks");
    fs::create_dir(&hooks).unwrap();
    git(
        directory.path(),
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    git(
        directory.path(),
        &["config", "user.email", "test@example.test"],
    );
    git(directory.path(), &["config", "user.name", "OComment Test"]);
    git(directory.path(), &["config", "commit.gpgsign", "false"]);
    directory
}

#[test]
fn check_diff_and_fix_follow_the_exit_contract() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sample.rs");
    fs::write(&path, b"fn main() { // remove\r\n}\r\n").unwrap();

    let checked = run(directory.path(), &["check", "sample.rs"]);
    assert_eq!(checked.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&checked.stdout).contains("removable"));

    let diff = run(directory.path(), &["diff", "sample.rs"]);
    assert_eq!(diff.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&diff.stdout).contains("--- a/sample.rs"));

    let fixed = run(directory.path(), &["fix", "sample.rs"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(fs::read(&path).unwrap(), b"fn main() { \r\n}\r\n");
    assert_eq!(
        run(directory.path(), &["check", "sample.rs"]).status.code(),
        Some(0)
    );
}

/* NOTE: A lock file carries no extension the detector can use, so the whole
 * name has to reach it through the binary for the run to scan the file at all.
 * `Cargo.lock` is the one every Rust checkout has. */
#[test]
fn a_toml_lock_file_is_scanned_under_its_reserved_name() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Cargo.lock");
    fs::write(&path, b"# generated\nname = \"# opaque\" # remove\n").unwrap();

    let scanned = run(
        directory.path(),
        &["scan", "Cargo.lock", "--format", "json"],
    );
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "toml");
    assert_eq!(report["comments"].as_array().unwrap().len(), 2);

    let fixed = run(directory.path(), &["fix", "Cargo.lock"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(fs::read(&path).unwrap(), b"\nname = \"# opaque\" \n");
}

/* NOTE: A `.clang-format` file is YAML with no extension for the detector to go
 * on and a hidden name besides, so naming it is what gets it scanned at all:
 * the whole name reaches the detector, and an explicitly named path lifts the
 * hidden-file rule the walk applies on its own. */
#[test]
fn a_yaml_configuration_is_scanned_under_its_reserved_name() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(".clang-format");
    fs::write(
        &path,
        b"# yamllint disable-line rule:line-length\nColumnLimit: 100 # remove\n",
    )
    .unwrap();

    let scanned = run(
        directory.path(),
        &["scan", ".clang-format", "--format", "json"],
    );
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "yaml");
    assert_eq!(report["comments"].as_array().unwrap().len(), 2);

    let fixed = run(directory.path(), &["fix", ".clang-format"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"# yamllint disable-line rule:line-length\nColumnLimit: 100 \n"
    );
}

/* NOTE: A Lua script installed as a command carries no extension at all, so the
 * `#!` line is the only evidence the run has; this is the path from the file
 * name through the detector and out the other side as a Lua scan. */
#[test]
fn a_lua_script_is_scanned_from_its_shebang_alone() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hook");
    fs::write(
        &path,
        b"#!/usr/bin/env lua\nprint(\"-- opaque\") -- remove\n",
    )
    .unwrap();

    let scanned = run(directory.path(), &["scan", "hook", "--format", "json"]);
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "lua");
    assert_eq!(report["comments"].as_array().unwrap().len(), 2);

    let fixed = run(directory.path(), &["fix", "hook"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"#!/usr/bin/env lua\nprint(\"-- opaque\") \n"
    );
}

/* NOTE: A PHP template is two languages in one file and only the code half is
 * scanned: the inline HTML around the tags is content, so the `<!-- -->` comment
 * in it survives a run that removes the `//` comment inside them. */
#[test]
fn a_php_template_is_scanned_only_inside_its_tags() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("page.phtml");
    fs::write(
        &path,
        b"<!-- inline html -->\n<?php // remove\necho \"# opaque\";\n?>\n",
    )
    .unwrap();

    let scanned = run(
        directory.path(),
        &["scan", "page.phtml", "--format", "json"],
    );
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "php");
    assert_eq!(report["comments"].as_array().unwrap().len(), 1);

    let fixed = run(directory.path(), &["fix", "page.phtml"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"<!-- inline html -->\n<?php \necho \"# opaque\";\n?>\n"
    );
}

/* NOTE: Zig is the one built-in language with no block comment, and this is what
 * that costs a run end to end: `// zig fmt: off` is the only instruction the
 * formatter reads out of a comment and is kept, the `//` written on a
 * multiline string literal line is content the way one inside a quoted string
 * is, and only the ordinary comment beside them is removed. `zig ast-check`
 * (0.16.0) accepts the file below. */
#[test]
fn a_zig_file_keeps_its_fmt_directive_and_its_multiline_string() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.zig");
    fs::write(
        &path,
        b"// zig fmt: off\nconst text =\n    \\\\a // not a comment\n;\nconst n: u32 = 1; // remove\n",
    )
    .unwrap();

    let scanned = run(directory.path(), &["scan", "main.zig", "--format", "json"]);
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "zig");
    assert_eq!(report["comments"].as_array().unwrap().len(), 2);
    assert_eq!(report["comments"][0]["kind"], "directive");
    assert_eq!(report["comments"][0]["disposition"]["action"], "keep");
    assert_eq!(report["comments"][1]["disposition"]["action"], "remove");

    let fixed = run(directory.path(), &["fix", "main.zig"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"// zig fmt: off\nconst text =\n    \\\\a // not a comment\n;\nconst n: u32 = 1; \n"
    );
}

/* NOTE: Dart is the one built-in C-family language whose block comment nests, and
 * this is what that plus its interpolation costs a run end to end: the outer
 * `/*` is closed by the second `*/` and not the first, `${ ... }` is code so
 * the comment written inside the string is a comment of its own, and
 * `// dart format off` is one of the four instructions a Dart tool reads and is
 * kept. Ground truth, Dart SDK 3.13.2 `scanString`: `SINGLE_LINE_COMMENT` at
 * [0,18), `MULTI_LINE_COMMENT "/* who */
"` at [48,57) inside the interpolation,
 * `SINGLE_LINE_COMMENT` at [61,70), and `MULTI_LINE_COMMENT` at [71,106).
 * `dart analyze` accepts both the file below and the bytes `fix` leaves. */
#[test]
fn a_dart_file_keeps_its_format_directive_and_nests_its_block_comment() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.dart");
    fs::write(
        &path,
        b"// dart format off\nvar greeting = 'hi ${'there' /* who */}'; // remove\n/* outer /* inner */ still outer */\n",
    )
    .unwrap();

    let scanned = run(directory.path(), &["scan", "main.dart", "--format", "json"]);
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "dart");
    assert_eq!(report["comments"].as_array().unwrap().len(), 4);
    assert_eq!(report["comments"][0]["kind"], "directive");
    assert_eq!(report["comments"][0]["disposition"]["action"], "keep");
    assert_eq!(report["comments"][1]["span"]["start"], 48);
    assert_eq!(report["comments"][1]["span"]["end"], 57);
    assert_eq!(report["comments"][3]["span"]["start"], 71);
    assert_eq!(report["comments"][3]["span"]["end"], 106);

    let fixed = run(directory.path(), &["fix", "main.dart"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"// dart format off\nvar greeting = 'hi ${'there' }'; \n\n"
    );
}

/* NOTE: Swift is the one built-in language whose regular expression literal can
 * carry a `//` with no quote in front of it, and this is what that costs a run
 * end to end: `#/https://x/#` holds two slashes that are pattern rather than
 * comment, `\( ... )` is code so the block comment written inside the string is
 * a comment of its own, the outer `/*` is closed by the second `*/` and not the
 * first, and `// swift-tools-version:` is kept because SwiftPM reads it before
 * it reads a manifest at all. Ground truth, the SwiftSyntax parser of the Swift
 * 6.3.3 toolchain: `lineComment` at [0,26), a `regexLiteralPattern` at [39,48),
 * `lineComment` at [52,61), `blockComment` at [92,101) inside the
 * interpolation, `lineComment` at [105,114), and `blockComment` at [115,150).
 * `swift-frontend -dump-parse -swift-version 6` accepts both the file below and
 * the bytes `fix` leaves. */
#[test]
fn a_swift_file_keeps_its_tools_version_and_hides_a_slash_pair_in_a_regex() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Package.swift");
    fs::write(
        &path,
        b"// swift-tools-version:5.9\nlet url = #/https://x/#  // remove\nlet greeting = \"hi \\( \"there\" /* who */ )\" // remove\n/* outer /* inner */ still outer */\n",
    )
    .unwrap();

    let scanned = run(
        directory.path(),
        &["scan", "Package.swift", "--format", "json"],
    );
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "swift");
    assert_eq!(report["comments"].as_array().unwrap().len(), 5);
    assert_eq!(report["comments"][0]["kind"], "directive");
    assert_eq!(report["comments"][0]["disposition"]["action"], "keep");
    assert_eq!(report["comments"][1]["span"]["start"], 52);
    assert_eq!(report["comments"][2]["span"]["start"], 92);
    assert_eq!(report["comments"][2]["span"]["end"], 101);
    assert_eq!(report["comments"][4]["span"]["start"], 115);
    assert_eq!(report["comments"][4]["span"]["end"], 150);

    let fixed = run(directory.path(), &["fix", "Package.swift"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"// swift-tools-version:5.9\nlet url = #/https://x/#  \nlet greeting = \"hi \\( \"there\"  )\" \n\n"
    );
}

/* NOTE: C# is the one built-in language whose *lines* are lexed two ways, and
 * this is what that costs a run end to end: `#region` takes the rest of its line
 * as the label an editor folds under, so the `//` in it is not a comment, while
 * the `//` behind `#endregion` is one; the format clause behind the `:` of an
 * interpolation hole is text, so the `//` in it is not a comment either; a
 * verbatim string carries its `\` and hides the `//` inside it; and a block
 * comment does not nest, so its first closing delimiter ends it and the `//`
 * behind the leftovers opens a comment of its own. `// <auto-generated/>` is kept because Roslyn exempts a
 * file carrying one from every analyzer that opts out of generated code. Ground
 * truth, the Roslyn lexer the .NET SDK 10.0.400 ships:
 * `SingleLineCommentTrivia` at [0,20), `PreprocessingMessageTrivia` at [29,53),
 * `SingleLineCommentTrivia` at [125,134), `MultiLineCommentTrivia` at [135,155),
 * and `SingleLineCommentTrivia` at [156,165) and [177,186). It reports no
 * lexical diagnostic for the file below, nor for the bytes `fix` leaves. */
#[test]
fn a_csharp_file_keeps_its_generated_marker_and_its_directive_lines() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Program.cs");
    fs::write(
        &path,
        b"// <auto-generated/>\n#region Helpers // not a comment\nvar path = @\"C:\\dir // no\";\nvar text = $\"{path.Length:D4 // no} tail\"; // remove\n/* outer /* inner */ // remove\n#endregion // remove\n",
    )
    .unwrap();

    let scanned = run(
        directory.path(),
        &["scan", "Program.cs", "--format", "json"],
    );
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "csharp");
    assert_eq!(report["comments"].as_array().unwrap().len(), 5);
    assert_eq!(report["comments"][0]["kind"], "directive");
    assert_eq!(report["comments"][0]["disposition"]["action"], "keep");
    assert_eq!(report["comments"][1]["span"]["start"], 125);
    assert_eq!(report["comments"][2]["kind"], "block");
    assert_eq!(report["comments"][2]["span"]["start"], 135);
    assert_eq!(report["comments"][2]["span"]["end"], 155);
    assert_eq!(report["comments"][4]["span"]["start"], 177);
    assert_eq!(report["comments"][4]["span"]["end"], 186);

    let fixed = run(directory.path(), &["fix", "Program.cs"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"// <auto-generated/>\n#region Helpers // not a comment\nvar path = @\"C:\\dir // no\";\nvar text = $\"{path.Length:D4 // no} tail\"; \n \n#endregion \n"
    );
}

/* NOTE: R is the one built-in language whose extension is written in upper case
 * as often as in lower — `analysis.R` and `analysis.r` are the same kind of
 * file — so this is the run that proves the suffix is folded before it is
 * looked up. It is also what a roxygen comment costs end to end: `#'` is
 * documentation and the default policy takes it, `# nolint` is lintr's
 * instruction and is kept, and the `#` inside the raw string is content. R
 * 4.3.3 `getParseData` reads the file below as `COMMENT` at [0,19), [20,30),
 * [60,68) and [116,124), with `STR_CONST` covering [80,104). */
#[test]
fn an_r_file_keeps_its_lint_directive_and_its_raw_string() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("analysis.R");
    fs::write(
        &path,
        b"#' Add two numbers.\n#' @export\nadd <- function(a, b) a + b  # nolint\npattern <- r\"(\\d+ # not a comment)\"\ntotal <- 1 # remove\n",
    )
    .unwrap();

    let scanned = run(
        directory.path(),
        &["scan", "analysis.R", "--format", "json"],
    );
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "r");
    assert_eq!(report["comments"].as_array().unwrap().len(), 4);
    assert_eq!(report["comments"][0]["kind"], "doc-line");
    assert_eq!(report["comments"][2]["kind"], "directive");
    assert_eq!(report["comments"][2]["disposition"]["action"], "keep");
    assert_eq!(report["comments"][3]["disposition"]["action"], "remove");

    let fixed = run(directory.path(), &["fix", "analysis.R"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"\n\nadd <- function(a, b) a + b  # nolint\npattern <- r\"(\\d+ # not a comment)\"\ntotal <- 1 \n"
    );
}

/* NOTE: A `Gemfile` carries no extension, so it reaches the Ruby scanner by its
 * whole name alone — and once there, the magic comment at the head of it is a
 * directive the default policy keeps, where the embedded document below it is
 * an ordinary comment the same run removes. */
#[test]
fn a_gemfile_is_scanned_as_ruby_by_its_name_alone() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Gemfile");
    fs::write(
        &path,
        b"# frozen_string_literal: true\nsource '# opaque' # remove\n=begin\nnotes\n=end\n",
    )
    .unwrap();

    let scanned = run(directory.path(), &["scan", "Gemfile", "--format", "json"]);
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "ruby");
    assert_eq!(report["comments"].as_array().unwrap().len(), 3);
    assert_eq!(report["comments"][0]["kind"], "directive");
    assert_eq!(report["comments"][0]["disposition"]["action"], "keep");

    let fixed = run(directory.path(), &["fix", "Gemfile"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"# frozen_string_literal: true\nsource '# opaque' \n\n\n\n"
    );
}

#[test]
fn invalid_input_returns_two_and_fix_is_non_destructive() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("broken.c");
    let original = b"int x; /* unterminated";
    fs::write(&path, original).unwrap();
    let output = run(directory.path(), &["fix", "broken.c"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn force_invalid_applies_known_spans_but_keeps_error_exit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("broken.c");
    fs::write(&path, b"int x; // remove\n\"unterminated").unwrap();
    let output = run(directory.path(), &["fix", "broken.c", "--force-invalid"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(path).unwrap(), b"int x; \n\"unterminated");
}

#[test]
fn json_and_jsonl_are_machine_readable() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("x.py"), b"x = 1 # remove\n").unwrap();
    let json = run(directory.path(), &["scan", "x.py", "--format", "json"]);
    assert_eq!(json.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["files"][0]["language"], "python");
    let jsonl = run(directory.path(), &["scan", "x.py", "--format", "jsonl"]);
    assert_eq!(
        jsonl
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count(),
        1
    );
}

#[test]
fn strict_configuration_suggests_unknown_keys() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\npolciy = \"safe\"\n",
    )
    .unwrap();
    let output = run(directory.path(), &["config", "show"]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("unknown field"));
    assert!(error.contains("policy"));
}

#[test]
fn declarative_profile_is_used_by_extension() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        br#"version = 1

[profiles.demo]
extensions = ["demo"]

[[profiles.demo.line_comments]]
start = ";;"

[[profiles.demo.strings]]
start = "\""
end = "\""
escape = "\\"
"#,
    )
    .unwrap();
    let path = directory.path().join("x.demo");
    fs::write(&path, b"\";; string\" ;; remove\n").unwrap();
    let output = run(directory.path(), &["fix", "x.demo"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(path).unwrap(), b"\";; string\" \n");
}

#[test]
fn init_lefthook_preserves_partial_stage_contract() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["init", "lefthook", "--fix"]);
    assert_eq!(output.status.code(), Some(0));
    let generated = fs::read_to_string(directory.path().join("lefthook.yml")).unwrap();
    assert!(generated.contains("ocomment fix --staged"));
    assert!(!generated.contains("stage_fixed"));
}

/// The starter file is a decision the reader may already have made
/// differently: a second `init` must not quietly replace the config they have
/// been editing, and the refusal has to name both ways out.
#[test]
fn init_config_refuses_to_overwrite_an_existing_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(".ocomment.toml");
    let mine = b"version = 1\n[policy]\nmode = \"all\"\n";
    fs::write(&path, mine).unwrap();

    let output = run(directory.path(), &["init", "config"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains(
            ".ocomment.toml already exists; use --force to overwrite or --stdout to print the \
             template"
        ),
        "{error}"
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        mine,
        "the refusal edited the file"
    );
}

/// The same refusal guards the hook file, and `--force` is the way past it.
#[test]
fn init_force_overwrites_an_existing_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(".ocomment.toml");
    fs::write(&path, b"stale\n").unwrap();

    let output = run(directory.path(), &["init", "config", "--force"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.contains("version = 1"), "{written}");
    assert!(
        !written.contains("stale"),
        "the old bytes survived: {written}"
    );

    let hook = directory.path().join("lefthook.yml");
    fs::write(&hook, b"stale\n").unwrap();
    let refused = run(directory.path(), &["init", "lefthook"]);
    assert_eq!(refused.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("lefthook.yml already exists"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(fs::read(&hook).unwrap(), b"stale\n");
    let forced = run(directory.path(), &["init", "lefthook", "--force"]);
    assert_eq!(forced.status.code(), Some(0));
    assert!(fs::read_to_string(&hook).unwrap().contains("pre-commit:"));
}

/// `--stdout` is the read-only door: the template goes to the pipe and the
/// working directory is left exactly as it was found.
#[test]
fn init_stdout_prints_the_template_and_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["init", "config", "--stdout"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8(output.stdout).unwrap();
    assert!(printed.contains("version = 1"), "{printed}");
    assert!(
        !printed.contains("created "),
        "nothing was created, so nothing may claim it was: {printed}"
    );
    assert!(!directory.path().join(".ocomment.toml").exists());

    let hook = run(directory.path(), &["init", "lefthook", "--fix", "--stdout"]);
    assert_eq!(hook.status.code(), Some(0));
    let printed = String::from_utf8(hook.stdout).unwrap();
    assert!(printed.contains("ocomment fix --staged"), "{printed}");
    assert!(!directory.path().join("lefthook.yml").exists());
}

/// A config in a parent directory already governs this one, so a new starter
/// file here layers over it rather than starting from nothing. The note says
/// so once the file exists, and does not stop it being written.
#[test]
fn init_notes_a_project_config_that_already_applies() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n[policy]\nmode = \"all\"\n",
    )
    .unwrap();
    let nested = directory.path().join("crate");
    fs::create_dir(&nested).unwrap();

    let output = run(&nested, &["init", "config"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let note = String::from_utf8_lossy(&output.stderr);
    assert!(
        note.contains("note: ")
            && note.contains(".ocomment.toml already applies to this directory"),
        "{note}"
    );
    assert!(
        nested.join(".ocomment.toml").is_file(),
        "the note replaced the file"
    );

    /* NOTE: The config the run itself just created is this directory's own, not an
     * inherited one, so a first `init` in a bare directory says nothing. */
    let bare = tempfile::tempdir().unwrap();
    let quiet = run(bare.path(), &["init", "config"]);
    assert_eq!(quiet.status.code(), Some(0));
    assert!(
        !String::from_utf8_lossy(&quiet.stderr).contains("already applies"),
        "{}",
        String::from_utf8_lossy(&quiet.stderr)
    );

    /* NOTE: The note is advice about a file that was just created. A refused `init`
     * created nothing, so it has nothing to advise about: the error stands
     * alone rather than trailing guidance for a file that does not exist. */
    let refused = run(&nested, &["init", "config"]);
    assert_eq!(refused.status.code(), Some(2));
    let error = String::from_utf8_lossy(&refused.stderr);
    assert!(error.contains(".ocomment.toml already exists"), "{error}");
    assert!(
        !error.contains("already applies"),
        "a refused init still advised about the inherited config:\n{error}"
    );
}

/// Creating the file is not the end of the task, so the line that reports it
/// names the step that is.
#[test]
fn init_success_messages_name_the_next_step() {
    let directory = tempfile::tempdir().unwrap();
    let config = run(directory.path(), &["init", "config"]);
    assert_eq!(config.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&config.stdout).trim_end(),
        "created .ocomment.toml \u{2014} edit [policy] and run `ocomment check`"
    );

    let hook = run(directory.path(), &["init", "lefthook"]);
    assert_eq!(hook.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&hook.stdout).trim_end(),
        "created lefthook.yml \u{2014} run `lefthook install` to activate the hook"
    );
}

/// One writes a file and the other refuses to; asking for both is a mistake
/// clap can catch before anything is opened.
#[test]
fn init_refuses_force_together_with_stdout() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["init", "config", "--force", "--stdout"]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("--force"), "{error}");
    assert!(error.contains("--stdout"), "{error}");
    assert!(!directory.path().join(".ocomment.toml").exists());
}

/// The two new switches are documented where a reader looks for them.
#[test]
fn init_help_documents_force_and_stdout() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["init", "--help"]);
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8(output.stdout).unwrap();
    for needle in ["--force", "--stdout"] {
        assert!(help.contains(needle), "`{needle}` is missing from:\n{help}");
    }
}

#[test]
fn staged_fix_does_not_stage_unrelated_working_tree_changes() {
    let directory = repository();
    let path = directory.path().join("partial stage.rs");
    fs::write(&path, b"let a = 1; // existing\nlet b = 2;\n").unwrap();
    git(directory.path(), &["add", "partial stage.rs"]);
    git(
        directory.path(),
        &["commit", "--quiet", "--message", "base"],
    );

    fs::write(&path, b"let a = 1; // existing\nlet b = 2; // staged\n").unwrap();
    git(directory.path(), &["add", "partial stage.rs"]);
    fs::write(
        &path,
        b"let a = 1; // existing\nlet b = 2; // staged\nlet c = 3; // unstaged\n",
    )
    .unwrap();

    let output = run(directory.path(), &["fix", "--staged"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let staged = git(directory.path(), &["show", ":partial stage.rs"]);
    assert_eq!(staged, b"let a = 1; // existing\nlet b = 2; \n");
    assert_eq!(
        fs::read(&path).unwrap(),
        b"let a = 1; // existing\nlet b = 2; \nlet c = 3; // unstaged\n"
    );
    let unstaged =
        String::from_utf8(git(directory.path(), &["diff", "--", "partial stage.rs"])).unwrap();
    assert!(unstaged.contains("// unstaged"));
    let cached = String::from_utf8(git(
        directory.path(),
        &["diff", "--cached", "--", "partial stage.rs"],
    ))
    .unwrap();
    assert!(!cached.contains("// unstaged"));
}

/// `--staged` reads its paths from the index rather than from a walk, but
/// `[files]` says which of the project's files OComment is allowed to touch
/// either way. A path the configuration excludes is not the commit hook's
/// business: it is not reported, and `fix --staged` leaves its blob alone.
#[test]
fn staged_runs_honour_the_files_exclude_globs() {
    let directory = repository();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[files]\nexclude = [\"vendor/**\"]\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join("vendor")).unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("vendor/x.rs"),
        b"let a = 1; // vendored\n",
    )
    .unwrap();
    fs::write(directory.path().join("src/y.rs"), b"let b = 2; // ours\n").unwrap();
    git(directory.path(), &["add", "vendor/x.rs", "src/y.rs"]);

    let checked = run(directory.path(), &["check", "--staged"]);
    let report = String::from_utf8(checked.stdout).unwrap();
    assert_eq!(
        checked.status.code(),
        Some(1),
        "`check --staged` said:\n{report}"
    );
    assert!(report.contains("src/y.rs"), "{report}");
    assert!(
        !report.contains("vendor/x.rs"),
        "an excluded path was reported:\n{report}"
    );

    let fixed = run(directory.path(), &["fix", "--staged"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        git(directory.path(), &["show", ":vendor/x.rs"]),
        b"let a = 1; // vendored\n",
        "an excluded blob was rewritten"
    );
    assert_eq!(
        git(directory.path(), &["show", ":src/y.rs"]),
        b"let b = 2; \n"
    );
}

/// The other half of the same rule: an `include` list narrows a staged run to
/// the paths it names, exactly as it narrows a walk.
#[test]
fn staged_runs_honour_the_files_include_globs() {
    let directory = repository();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[files]\ninclude = [\"src/**\"]\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join("vendor")).unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("vendor/x.rs"),
        b"let a = 1; // vendored\n",
    )
    .unwrap();
    fs::write(directory.path().join("src/y.rs"), b"let b = 2; // ours\n").unwrap();
    git(directory.path(), &["add", "vendor/x.rs", "src/y.rs"]);

    let checked = run(directory.path(), &["check", "--staged"]);
    let report = String::from_utf8(checked.stdout).unwrap();
    assert_eq!(
        checked.status.code(),
        Some(1),
        "`check --staged` said:\n{report}"
    );
    assert!(report.contains("src/y.rs"), "{report}");
    assert!(
        !report.contains("vendor/x.rs"),
        "a path outside the include list was reported:\n{report}"
    );
}

/// `git` names a staged path relative to the repository root and a `[files]`
/// glob is written relative to the project root, so the two meet wherever the
/// command was typed. A run from a subdirectory must reach the same verdict as
/// a run from the top.
#[test]
fn staged_globs_stay_root_relative_from_a_subdirectory() {
    let directory = repository();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[files]\nexclude = [\"vendor/**\"]\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join("vendor")).unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("vendor/x.rs"),
        b"let a = 1; // vendored\n",
    )
    .unwrap();
    fs::write(directory.path().join("src/y.rs"), b"let b = 2; // ours\n").unwrap();
    git(directory.path(), &["add", "vendor/x.rs", "src/y.rs"]);

    let checked = run(&directory.path().join("src"), &["check", "--staged"]);
    let report = String::from_utf8(checked.stdout).unwrap();
    assert_eq!(
        checked.status.code(),
        Some(1),
        "`check --staged` said:\n{report}"
    );
    assert!(report.contains("src/y.rs"), "{report}");
    assert!(
        !report.contains("vendor/x.rs"),
        "an excluded path was reported from a subdirectory:\n{report}"
    );
}

/// `[files]` bounds a walk with more than its two glob lists: `hidden` decides
/// whether a dot-directory is looked into at all, and `max_size` decides how
/// much of a file is worth reading. A staged path is a walked path rather than
/// a named one, so a staged run is bounded by the same two settings — a commit
/// that touches `.cache/generated.rs` or a two-megabyte fixture must not put
/// either through a hook that would never have walked into them.
#[test]
fn staged_runs_honour_the_hidden_and_size_limits() {
    let directory = repository();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[files]\nmax_size = 20\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join(".hidden")).unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(directory.path().join(".hidden/x.rs"), b"let a = 1; // x\n").unwrap();
    fs::write(
        directory.path().join("big.rs"),
        b"let big = 3; // past the limit\n",
    )
    .unwrap();
    fs::write(directory.path().join("src/y.rs"), b"let b = 2; // ours\n").unwrap();
    git(
        directory.path(),
        &["add", ".hidden/x.rs", "big.rs", "src/y.rs"],
    );

    let checked = run(directory.path(), &["check", "--staged"]);
    let report = String::from_utf8(checked.stdout).unwrap();
    let summary = String::from_utf8(checked.stderr).unwrap();
    assert_eq!(
        checked.status.code(),
        Some(1),
        "`check --staged` said:\n{report}{summary}"
    );
    assert!(report.contains("src/y.rs"), "{report}");
    assert!(
        !report.contains(".hidden/x.rs"),
        "a hidden staged path was reported while `hidden` is off:\n{report}"
    );
    assert!(
        !report.contains("big.rs"),
        "a staged blob past `max_size` was reported:\n{report}"
    );
    /* NOTE: A size skip is a fact about the run, so it is folded into the summary
     * under the same short label a walk gives it; a hidden path was never a
     * candidate and is not a skip at all. */
    assert!(
        summary.contains("1 file skipped (too large: 1"),
        "the oversized staged blob was passed over silently:\n{summary}"
    );

    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[files]\nmax_size = 20\nhidden = true\n",
    )
    .unwrap();
    let visible = run(directory.path(), &["check", "--staged"]);
    let report = String::from_utf8(visible.stdout).unwrap();
    assert_eq!(
        visible.status.code(),
        Some(1),
        "`check --staged` said:\n{report}"
    );
    assert!(
        report.contains(".hidden/x.rs"),
        "`hidden = true` did not reach the staged run:\n{report}"
    );
    assert!(
        !report.contains("big.rs"),
        "`hidden = true` also lifted the size limit:\n{report}"
    );
}

/// The other half of the same rule. `[files]` bounds what a run *finds*, and
/// a path the caller typed was never found: they named it, so it is checked
/// whatever `hidden` and `max_size` would have said about it. A walk has said
/// so since `discover_with_scope`, and a staged run says it about the same two
/// settings — otherwise `ocomment check --staged .hidden/x.rs` answers about
/// nothing at all, which reads as a clean file rather than as a path outside
/// the project's bounds.
#[test]
fn staged_paths_the_caller_names_bypass_the_hidden_and_size_limits() {
    let directory = repository();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[files]\nmax_size = 20\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join(".hidden")).unwrap();
    fs::write(directory.path().join(".hidden/x.rs"), b"let a = 1; // x\n").unwrap();
    fs::write(
        directory.path().join("big.rs"),
        b"let big = 3; // past the limit\n",
    )
    .unwrap();
    git(directory.path(), &["add", ".hidden/x.rs", "big.rs"]);

    for named in [".hidden/x.rs", "big.rs"] {
        let checked = run(directory.path(), &["check", "--staged", named]);
        let report = String::from_utf8(checked.stdout).unwrap();
        assert_eq!(
            checked.status.code(),
            Some(1),
            "`check --staged {named}` said:\n{report}{}",
            String::from_utf8_lossy(&checked.stderr)
        );
        assert!(
            report.contains(named),
            "the caller named {named} and it was filtered out anyway:\n{report}"
        );
    }

    /* NOTE: A named directory carries the same licence to everything under it,
     * exactly as an explicitly walked directory does. */
    let named_directory = run(directory.path(), &["check", "--staged", ".hidden"]);
    let report = String::from_utf8(named_directory.stdout).unwrap();
    assert_eq!(named_directory.status.code(), Some(1), "{report}");
    assert!(
        report.contains(".hidden/x.rs"),
        "a staged path under a named directory was filtered out:\n{report}"
    );

    /* NOTE: Nobody named anything here, so both limits apply again. */
    let bare = run(directory.path(), &["check", "--staged"]);
    let report = String::from_utf8(bare.stdout).unwrap();
    assert_eq!(
        bare.status.code(),
        Some(0),
        "a bare staged run reported a hidden or oversized path:\n{report}"
    );
    assert!(!report.contains(".hidden/x.rs"), "{report}");
    assert!(!report.contains("big.rs"), "{report}");
}

/// A pathspec is not always the prefix of the path `git` answers with.
///
/// `git diff --cached` names a staged path relative to the repository root,
/// while the pathspec beside it is written however the caller found it
/// convenient: as an absolute path, or with a wildcard `git` expands itself.
/// Comparing the two as text answers "nobody named this" for both spellings,
/// and the project's limits then hide the very file the caller asked about, so
/// the question goes to `git` instead — the only party that knows what a
/// pathspec covers.
#[test]
fn a_staged_pathspec_names_its_paths_however_it_is_spelled() {
    let directory = repository();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[files]\nmax_size = 20\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join(".hidden")).unwrap();
    fs::write(directory.path().join(".hidden/x.rs"), b"let a = 1; // x\n").unwrap();
    git(directory.path(), &["add", ".hidden/x.rs"]);

    let absolute = directory.path().join(".hidden/x.rs");
    let named = run(
        directory.path(),
        &["check", "--staged", absolute.to_str().unwrap()],
    );
    let report = String::from_utf8(named.stdout).unwrap();
    assert_eq!(
        named.status.code(),
        Some(1),
        "an absolute staged pathspec named nothing:\n{report}"
    );
    assert!(report.contains(".hidden/x.rs"), "{report}");

    let expanded = run(directory.path(), &["check", "--staged", ".hidden/*.rs"]);
    let report = String::from_utf8(expanded.stdout).unwrap();
    assert_eq!(
        expanded.status.code(),
        Some(1),
        "a wildcard staged pathspec named nothing:\n{report}"
    );
    assert!(report.contains(".hidden/x.rs"), "{report}");
}

/// A repository whose staged paths sit on both sides of every `[files]` limit:
/// one hidden, one oversized at the top, one oversized under `src`, and one
/// ordinary file `src` is scanned for.
fn repository_with_limits() -> TempDir {
    let directory = repository();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[files]\nmax_size = 20\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join(".hidden")).unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(directory.path().join(".hidden/x.rs"), b"let a = 1; // x\n").unwrap();
    fs::write(
        directory.path().join("wide.rs"),
        b"let wide = 3; // past the limit\n",
    )
    .unwrap();
    fs::write(directory.path().join("src/y.rs"), b"let b = 2; // ours\n").unwrap();
    fs::write(
        directory.path().join("src/tall.rs"),
        b"let tall = 4; // past the limit\n",
    )
    .unwrap();
    git(
        directory.path(),
        &["add", ".hidden/x.rs", "wide.rs", "src/y.rs", "src/tall.rs"],
    );
    directory
}

/// Where a pathspec was typed decides what it covers.
///
/// `ocomment check .` from `src/` walks `src/`, so `--staged .` from the same
/// directory has to mean the same subtree — and to mean it in both directions:
/// what sits above `src` is no business of the run, and what sits under it was
/// named, so the project's limits are lifted from it exactly as a walk lifts
/// them from a directory the caller pointed at.
#[test]
fn a_staged_pathspec_is_resolved_where_it_was_typed() {
    let directory = repository_with_limits();

    let checked = run(&directory.path().join("src"), &["check", "--staged", "."]);
    let report = String::from_utf8(checked.stdout).unwrap();
    assert_eq!(
        checked.status.code(),
        Some(1),
        "`check --staged .` from a subdirectory said:\n{report}"
    );
    assert!(report.contains("src/y.rs"), "{report}");
    assert!(
        report.contains("src/tall.rs"),
        "`.` named the subtree and the size limit was applied to it anyway:\n{report}"
    );
    assert!(
        !report.contains(".hidden/x.rs") && !report.contains("wide.rs"),
        "`.` typed in src/ reached outside it:\n{report}"
    );
}

/// The whole tree is what a staged run already covers, so naming it says
/// nothing.
///
/// A hook that spells its run `ocomment check --staged .` from the top of the
/// repository is asking for the same run as `ocomment check --staged`, and it
/// must get the same answer: every `[files]` limit still applies. Only a
/// pathspec that narrows the run is a request about particular paths, which is
/// what earns the licence to look past those limits.
#[test]
fn a_whole_tree_staged_pathspec_keeps_the_project_limits() {
    let directory = repository_with_limits();

    let whole_tree = run(directory.path(), &["check", "--staged", "."]);
    let report = String::from_utf8(whole_tree.stdout).unwrap();
    let summary = String::from_utf8(whole_tree.stderr).unwrap();
    assert_eq!(
        whole_tree.status.code(),
        Some(1),
        "`check --staged .` said:\n{report}{summary}"
    );
    assert!(report.contains("src/y.rs"), "{report}");
    assert!(
        !report.contains(".hidden/x.rs"),
        "a bare `.` lifted `hidden` from the whole tree:\n{report}"
    );
    assert!(
        !report.contains("wide.rs") && !report.contains("src/tall.rs"),
        "a bare `.` lifted `max_size` from the whole tree:\n{report}"
    );
    assert!(
        summary.contains("2 files skipped (too large: 2"),
        "the oversized staged blobs were passed over silently:\n{summary}"
    );

    let bare = run(directory.path(), &["check", "--staged"]);
    assert_eq!(bare.status.code(), whole_tree.status.code());
    assert_eq!(
        String::from_utf8(bare.stdout).unwrap(),
        report,
        "`check --staged .` and `check --staged` disagreed about the same tree"
    );
    assert_eq!(String::from_utf8(bare.stderr).unwrap(), summary);
}

/// A staged path the caller named and nothing could read is answered on a line
/// of its own.
///
/// The rule is the walk's: what a run merely came across is folded into the
/// end-of-run summary, and what the caller asked about is answered directly.
/// `ocomment check --staged notes.md` that says only "nothing to check" reads
/// as a clean file rather than as a file with no scanner.
#[test]
fn a_named_staged_path_without_a_scanner_gets_its_own_line() {
    let directory = repository();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();
    git(directory.path(), &["add", "notes.md"]);

    let named = run(directory.path(), &["check", "--staged", "notes.md"]);
    let report = String::from_utf8(named.stdout).unwrap();
    assert_eq!(named.status.code(), Some(0), "{report}");
    assert!(
        report.contains(&format!("notes.md: skipped: {NO_LANGUAGE}")),
        "a staged path the caller named was passed over without a word:\n{report}"
    );

    let bare = run(directory.path(), &["check", "--staged"]);
    let folded = String::from_utf8(bare.stdout).unwrap();
    assert!(
        !folded.contains("notes.md: skipped"),
        "a staged path nobody named was listed per file:\n{folded}"
    );
}

/// A staged blob OComment has no scanner for is passed over, and a run says so
/// the way a walk says it: folded onto the end-of-run summary under the same
/// short label, rather than dropped without a word. A pre-commit hook that
/// stages a PNG and a Markdown file is otherwise indistinguishable from one
/// that scanned them and found nothing.
#[test]
fn staged_blobs_without_a_scanner_are_counted_in_the_summary() {
    let directory = repository();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();
    fs::write(directory.path().join("image.dat"), b"\x89PNG\0\r\n").unwrap();
    fs::write(directory.path().join("y.rs"), b"let b = 2; // ours\n").unwrap();
    git(directory.path(), &["add", "notes.md", "image.dat", "y.rs"]);

    let checked = run(directory.path(), &["check", "--staged"]);
    let report = String::from_utf8(checked.stdout).unwrap();
    let summary = String::from_utf8(checked.stderr).unwrap();
    assert_eq!(
        checked.status.code(),
        Some(1),
        "`check --staged` said:\n{report}{summary}"
    );
    assert!(report.contains("y.rs"), "{report}");
    assert!(
        summary.contains("2 files skipped (binary: 1, unknown language: 1"),
        "the staged blobs nothing could read were passed over silently:\n{summary}"
    );
    /* NOTE: Nobody typed either path, so neither is annotated on a line of its
     * own until `-v` asks for the list. */
    assert!(
        !report.contains("notes.md") && !report.contains("image.dat"),
        "a folded skip was reported per file:\n{report}"
    );

    let verbose = run(directory.path(), &["check", "--staged", "-v"]);
    let listed = String::from_utf8(verbose.stdout).unwrap();
    assert!(
        listed.contains(&format!("notes.md: skipped: {NO_LANGUAGE}")),
        "-v did not list the staged path with no language:\n{listed}"
    );
    assert!(
        listed.contains("image.dat: skipped: binary file (NUL byte)"),
        "-v did not list the staged binary blob:\n{listed}"
    );
}

#[test]
fn staged_fix_stops_on_existing_block_comment_interior() {
    let directory = repository();
    let path = directory.path().join("block.c");
    fs::write(&path, b"/* existing\nbase\n*/\nint x;\n").unwrap();
    git(directory.path(), &["add", "block.c"]);
    git(
        directory.path(),
        &["commit", "--quiet", "--message", "base"],
    );
    let staged_source = b"/* existing\nbase\nadded\n*/\nint x;\n";
    fs::write(&path, staged_source).unwrap();
    git(directory.path(), &["add", "block.c"]);
    let before_index = git(directory.path(), &["show", ":block.c"]);

    let output = run(directory.path(), &["fix", "--staged"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stdout).contains("staged-existing-block-comment"));
    assert_eq!(git(directory.path(), &["show", ":block.c"]), before_index);
    assert_eq!(fs::read(path).unwrap(), staged_source);
}

#[test]
fn staged_new_rename_delete_and_unusual_paths_are_handled_from_index_blobs() {
    let directory = repository();
    fs::write(directory.path().join("renamed.rs"), b"let base = 1;\n").unwrap();
    fs::write(directory.path().join("deleted.rs"), b"let deleted = 1;\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(
        directory.path(),
        &["commit", "--quiet", "--message", "base"],
    );

    git(directory.path(), &["mv", "renamed.rs", "renamed target.rs"]);
    fs::write(
        directory.path().join("renamed target.rs"),
        b"let base = 1;\nlet renamed = 2; // staged rename\n",
    )
    .unwrap();
    git(directory.path(), &["add", "renamed target.rs"]);
    fs::remove_file(directory.path().join("deleted.rs")).unwrap();
    git(directory.path(), &["add", "deleted.rs"]);
    let unusual = "odd\n名前.rs";
    fs::write(
        directory.path().join(unusual),
        b"let new = 1; // new file\n",
    )
    .unwrap();
    git(directory.path(), &["add", unusual]);

    let output = run(directory.path(), &["fix", "--staged", "--index-only"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        git(directory.path(), &["show", ":renamed target.rs"]),
        b"let base = 1;\nlet renamed = 2; \n"
    );
    assert_eq!(
        git(directory.path(), &["show", &format!(":{unusual}")]),
        b"let new = 1; \n"
    );
    assert_eq!(
        fs::read(directory.path().join("renamed target.rs")).unwrap(),
        b"let base = 1;\nlet renamed = 2; // staged rename\n"
    );
    assert_eq!(
        fs::read(directory.path().join(unusual)).unwrap(),
        b"let new = 1; // new file\n"
    );
}

#[cfg(unix)]
#[test]
fn staged_non_utf8_paths_remain_os_native() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let directory = repository();
    let name = std::ffi::OsString::from_vec(b"non-\xff.rs".to_vec());
    let path = directory.path().join(&name);
    fs::write(&path, b"let value = 1; // remove\n").unwrap();
    git_with_path(directory.path(), &["add", "--"], &name);

    let output = run(directory.path(), &["fix", "--staged", "--index-only"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut specification = std::ffi::OsString::from(":");
    specification.push(&name);
    assert_eq!(
        git_with_path(
            directory.path(),
            &["cat-file", "blob"],
            specification.as_os_str()
        ),
        b"let value = 1; \n"
    );
    assert_eq!(name.as_bytes(), b"non-\xff.rs");
}

#[test]
fn ambiguous_staged_mapping_changes_nothing_and_suggests_index_only() {
    let directory = repository();
    let path = directory.path().join("ambiguous.rs");
    fs::write(&path, b"let base = 1;\n").unwrap();
    git(directory.path(), &["add", "ambiguous.rs"]);
    git(
        directory.path(),
        &["commit", "--quiet", "--message", "base"],
    );

    let staged = b"let base = 1;\nlet staged = 2; // remove\n";
    fs::write(&path, staged).unwrap();
    git(directory.path(), &["add", "ambiguous.rs"]);
    let working = [staged.as_slice(), staged.as_slice()].concat();
    fs::write(&path, &working).unwrap();
    let before_index = git(directory.path(), &["show", ":ambiguous.rs"]);

    let output = run(directory.path(), &["fix", "--staged"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--index-only"));
    assert_eq!(
        git(directory.path(), &["show", ":ambiguous.rs"]),
        before_index
    );
    assert_eq!(fs::read(&path).unwrap(), working);

    let index_only = run(directory.path(), &["fix", "--staged", "--index-only"]);
    assert_eq!(index_only.status.code(), Some(0));
    assert_eq!(
        git(directory.path(), &["show", ":ambiguous.rs"]),
        b"let base = 1;\nlet staged = 2; \n"
    );
    assert_eq!(fs::read(path).unwrap(), working);
}

#[test]
fn normal_walk_respects_gitignore() {
    let directory = repository();
    fs::write(directory.path().join(".gitignore"), b"ignored.rs\n").unwrap();
    fs::write(directory.path().join("ignored.rs"), b"// ignored\n").unwrap();
    fs::write(directory.path().join("seen.rs"), b"// seen\n").unwrap();
    let output = run(directory.path(), &[]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("seen.rs"));
    assert!(!text.contains("ignored.rs"));
}

#[test]
fn explicit_io_failure_returns_two_and_blocks_the_whole_fix() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("good.rs");
    let original = b"let x = 1; // remove\n";
    fs::write(&path, original).unwrap();
    let output = run(directory.path(), &["fix", "good.rs", "missing.rs"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(path).unwrap(), original);
    assert!(String::from_utf8_lossy(&output.stdout).contains("path does not exist"));
}

/// A command that names no path checks the current directory, the way every
/// other file-walking developer tool does. The repository root is still where
/// the configuration is discovered and where the override globs are anchored,
/// but it is no longer what a bare `ocomment` walks: run from a subdirectory,
/// the command must not reach back up to files the caller cannot see.
#[test]
fn no_argument_scan_uses_the_current_directory_not_the_repository_root() {
    let directory = repository();
    fs::write(directory.path().join("root.rs"), b"// root comment\n").unwrap();
    let nested = directory.path().join("nested/deeper");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("deep.rs"), b"// deep comment\n").unwrap();

    let output = run(&nested, &[]);
    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(
        !report.contains("root.rs"),
        "the bare command reached above the current directory:\n{report}"
    );
    assert!(
        report.contains("deep.rs:1:1: removable"),
        "the bare command never checked the current directory:\n{report}"
    );
    /* NOTE: The implicit target is `.`, and a walk rooted there prefixes every entry
     * with `./`. `ocomment` and `ocomment check deep.rs` report one file under
     * one name, so that prefix is not part of it. */
    assert!(
        !report.contains("./"),
        "the implicit target leaked its `./` into the report:\n{report}"
    );

    /* NOTE: `-v` names both halves of the answer: the root the configuration came
     * from, and the target that root no longer decides. */
    let traced = run(&nested, &["-v"]);
    let trace = String::from_utf8(traced.stderr).unwrap();
    let repository_name = directory.path().file_name().unwrap().to_str().unwrap();
    assert!(
        trace
            .lines()
            .any(|line| line.starts_with("root: ") && line.ends_with(repository_name)),
        "the trace did not root the run at the repository:\n{trace}"
    );
    assert!(
        trace.lines().any(|line| line == "target: ."),
        "the trace did not name the implicit target:\n{trace}"
    );
}

/// The root keeps the two jobs it did not lose: it is where `.ocomment.toml`
/// is found, and it is what `files.include`, `files.exclude`, and every
/// `[[overrides]].paths` glob is written relative to. A path named on the
/// command line is relative to the working directory instead, so the two only
/// line up once a path is resolved against the directory it was typed in —
/// whichever of the three ways the file was named.
#[test]
fn project_config_and_overrides_apply_from_a_subdirectory() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[files]\nexclude = [\"nested/skip/**\"]\n\n[[overrides]]\npaths = [\"nested/**\"]\npolicy = \"all\"\n",
    )
    .unwrap();
    let nested = directory.path().join("nested");
    fs::create_dir_all(nested.join("skip")).unwrap();
    /* NOTE: A directive is kept under the default `safe` policy and removed under
     * `all`, so the line it is reported on is the override speaking. */
    fs::write(nested.join("kept.rs"), b"let x = 1; // rustfmt::skip\n").unwrap();
    fs::write(nested.join("skip/ignored.rs"), b"let y = 2; // remove\n").unwrap();

    for arguments in [&[][..], &["check", "."][..], &["check", "kept.rs"][..]] {
        let output = run(&nested, arguments);
        let report = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "`ocomment {}` did not apply the override:\n{report}",
            arguments.join(" ")
        );
        assert!(
            report.contains("kept.rs:1:12: removable directive comment"),
            "`ocomment {}` did not apply the override:\n{report}",
            arguments.join(" ")
        );
        assert!(
            !report.contains("ignored.rs"),
            "`ocomment {}` walked into the excluded directory:\n{report}",
            arguments.join(" ")
        );
    }
}

/// `fix` is the command that writes, so the change of target matters most
/// there: run from a subdirectory it rewrites that subdirectory, and the
/// files above it are none of its business.
#[test]
fn fix_from_a_subdirectory_leaves_the_repository_root_alone() {
    let directory = repository();
    let untouched = directory.path().join("root.rs");
    let original = b"let a = 1; // root comment\n";
    fs::write(&untouched, original).unwrap();
    let nested = directory.path().join("nested");
    fs::create_dir(&nested).unwrap();
    let rewritten = nested.join("deep.rs");
    fs::write(&rewritten, b"let b = 2; // deep comment\n").unwrap();

    let output = run(&nested, &["fix"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(&untouched).unwrap(),
        original,
        "`fix` from a subdirectory rewrote the repository root"
    );
    assert_eq!(fs::read(&rewritten).unwrap(), b"let b = 2; \n");
}

/// A reader who has only ever run `ocomment fix` from the top of a repository
/// can read the bare command as "fix the project", so the one run that writes
/// says where it is pointed and where the project it belongs to starts. From
/// the root the two are the same directory and the note would be noise.
#[test]
fn fix_from_a_subdirectory_notes_the_project_root() {
    let directory = repository();
    let nested = directory.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("deep.rs"), b"let b = 2; // deep comment\n").unwrap();

    let output = run(&nested, &["fix"]);
    assert_eq!(output.status.code(), Some(0));
    let note = String::from_utf8(output.stderr).unwrap();
    assert!(
        note.contains("note: fixing files under . (project root: "),
        "`fix` never said what it was pointed at:\n{note}"
    );
    assert_eq!(
        note.matches("note: fixing files under").count(),
        1,
        "the scope was noted more than once:\n{note}"
    );

    let from_root = run(directory.path(), &["fix"]);
    assert_eq!(from_root.status.code(), Some(0));
    let quiet = String::from_utf8(from_root.stderr).unwrap();
    assert!(
        !quiet.contains("note: fixing files under"),
        "the note was printed where the target and the root agree:\n{quiet}"
    );
}

#[test]
fn explicit_directory_bypasses_hidden_and_size_limits() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n[files]\nmax_size = 1\n",
    )
    .unwrap();
    fs::write(
        directory.path().join(".hidden.rs"),
        b"// hidden and larger than one byte\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "."]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains(".hidden.rs"));
}

/// The target a command with no PATH stands in for is not an explicitly named
/// one: `.` substituted for a missing argument walks with the ordinary hidden
/// and size limits, so a bare run reports what a run naming its files would.
/// Naming the same directory is a request, and still bypasses both.
#[test]
fn an_implicit_target_keeps_the_hidden_and_size_limits() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n[files]\nmax_size = 1000\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join(".hidden")).unwrap();
    fs::write(
        directory.path().join(".hidden/b.rs"),
        b"let b = 1; // hidden\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/a.rs"),
        b"let a = 1; // remove me\n",
    )
    .unwrap();
    let mut big = String::from("// oversized\n");
    while big.len() <= 100_000 {
        big.push_str("let x = 1;\n");
    }
    fs::write(directory.path().join("src/big.rs"), big.as_bytes()).unwrap();

    let bare = run(directory.path(), &[]);
    let stdout = String::from_utf8_lossy(&bare.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&bare.stderr).into_owned();
    assert_eq!(
        bare.status.code(),
        Some(1),
        "bare run said:\n{stdout}{stderr}"
    );
    assert!(
        stdout.contains("src/a.rs:1:12: removable line comment"),
        "a bare run missed the one file it should report:\n{stdout}"
    );
    assert!(
        !stdout.contains(".hidden"),
        "a bare run reached into a hidden directory:\n{stdout}"
    );
    assert!(
        !stdout.contains("big.rs"),
        "a bare run scanned a file over files.max_size:\n{stdout}"
    );
    assert!(
        stderr.contains("Found 1 removable comment in 1 file (1 file scanned)."),
        "a bare run counted more than the one file it may walk:\n{stderr}"
    );
    assert!(
        stderr.contains("1 file skipped (too large: 1"),
        "a bare run did not fold the oversized file into its skips:\n{stderr}"
    );
}

/// `.git` is hidden, so nothing a bare run does may look inside it — and `fix`
/// least of all: the sample hooks git writes into a fresh repository are full
/// of comments, and rewriting them is not what "fix my project" asked for.
#[test]
fn a_bare_run_never_reaches_into_the_git_directory() {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init"]);
    fs::write(directory.path().join(".ocomment.toml"), b"version = 1\n").unwrap();
    let hook = directory.path().join(".git/hooks/x.sample");
    fs::write(&hook, b"let x = 1; // sample hook comment\n").unwrap();
    let before = fs::read(&hook).unwrap();
    fs::write(directory.path().join("a.rs"), b"let a = 1; // remove me\n").unwrap();

    let check = run(directory.path(), &[]);
    let listing = String::from_utf8_lossy(&check.stdout).into_owned();
    assert!(
        !listing.contains(".git"),
        "a bare check listed something under .git:\n{listing}"
    );

    let fixed = run(directory.path(), &["fix"]);
    let report = String::from_utf8_lossy(&fixed.stdout).into_owned();
    assert!(
        !report.contains(".git"),
        "a bare fix reported something under .git:\n{report}"
    );
    assert_eq!(
        fs::read(&hook).unwrap(),
        before,
        "a bare fix rewrote a file under .git"
    );
}

/// Naming the directory lifts the hidden-file rule, and so does `files.hidden`;
/// neither may lift the one that keeps git's own storage out of a walk. `git`
/// itself never offers `.git` as a candidate for anything, and a tool that
/// rewrites files in place may do so least of all: `ocomment fix .` in a fresh
/// repository would otherwise rewrite every sample hook git had just written.
#[test]
fn a_named_directory_never_reaches_into_the_git_directory() {
    for configuration in ["version = 1\n", "version = 1\n[files]\nhidden = true\n"] {
        let directory = tempfile::tempdir().unwrap();
        git(directory.path(), &["init", "-q"]);
        fs::write(directory.path().join(".ocomment.toml"), configuration).unwrap();
        let hook = directory.path().join(".git/hooks/x.sample");
        fs::write(&hook, b"#!/bin/sh\necho hi # sample hook comment\n").unwrap();
        let before = fs::read(&hook).unwrap();
        /* NOTE: A submodule or a linked worktree keeps its `.git` as a *file*; it
         * points at git's storage and is no more a candidate than the
         * directory it stands in for. */
        fs::create_dir(directory.path().join("vendor")).unwrap();
        fs::write(
            directory.path().join("vendor/.git"),
            b"gitdir: ../.git/modules/vendor\n",
        )
        .unwrap();
        fs::write(directory.path().join("a.rs"), b"let a = 1; // remove me\n").unwrap();

        let checked = run(directory.path(), &["check", "-v", "."]);
        let listing = format!(
            "{}{}",
            String::from_utf8_lossy(&checked.stdout),
            String::from_utf8_lossy(&checked.stderr)
        );
        assert!(
            !listing.contains(".git"),
            "`check .` under {configuration:?} reached into git's storage:\n{listing}"
        );
        assert!(
            listing.contains("a.rs:1:12: removable line comment"),
            "`check .` under {configuration:?} missed the project file:\n{listing}"
        );

        let fixed = run(directory.path(), &["fix", "."]);
        let report = format!(
            "{}{}",
            String::from_utf8_lossy(&fixed.stdout),
            String::from_utf8_lossy(&fixed.stderr)
        );
        assert!(
            !report.contains(".git"),
            "`fix .` under {configuration:?} reported something under .git:\n{report}"
        );
        assert_eq!(
            fs::read(&hook).unwrap(),
            before,
            "`fix .` under {configuration:?} rewrote a file under .git"
        );
    }
}

/// The exclusion is about where a walk may wander, not about what a caller may
/// ask for. A path typed on the command line is a request, so a hook the
/// caller pointed at is still reported.
#[test]
fn a_path_named_inside_the_git_directory_is_still_honoured() {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "-q"]);
    let hook = directory.path().join(".git/hooks/x.sample");
    fs::write(&hook, b"#!/bin/sh\necho hi # sample hook comment\n").unwrap();

    let output = run(directory.path(), &["check", ".git/hooks/x.sample"]);
    let listing = String::from_utf8_lossy(&output.stdout).into_owned();
    assert_eq!(
        output.status.code(),
        Some(1),
        "a named hook was not reported:\n{listing}"
    );
    assert!(
        listing.contains(".git/hooks/x.sample:2:9: removable line comment"),
        "a named hook was not reported:\n{listing}"
    );
}

#[cfg(unix)]
#[test]
fn symlink_following_is_explicitly_configurable() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("source.txt"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    symlink("source.txt", directory.path().join("link.rs")).unwrap();

    let skipped = run(directory.path(), &["check", "link.rs"]);
    assert_eq!(skipped.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&skipped.stdout).contains("symbolic link"));

    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n[files]\nfollow_symlinks = true\n",
    )
    .unwrap();
    let followed = run(directory.path(), &["check", "link.rs"]);
    assert_eq!(followed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&followed.stdout).contains("removable"));
}

fn subcommand_lines(help: &str) -> Vec<(String, String)> {
    let mut lines = help.lines().skip_while(|line| *line != "Commands:");
    lines.next();
    lines
        .take_while(|line| !line.trim().is_empty())
        .map(|line| {
            let trimmed = line.trim_start();
            match trimmed.split_once("  ") {
                Some((name, description)) => (name.to_owned(), description.trim().to_owned()),
                None => (trimmed.to_owned(), String::new()),
            }
        })
        .collect()
}

#[test]
fn help_describes_every_subcommand() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--help"]);
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8(output.stdout).unwrap();
    let listed = subcommand_lines(&help);
    assert!(!listed.is_empty(), "no Commands section in:\n{help}");
    for (name, description) in &listed {
        assert!(
            !description.is_empty(),
            "subcommand `{name}` has no description in:\n{help}"
        );
    }
    let expected = [
        ("check", "Report removable comments (default command)"),
        (
            "fix",
            "Remove comments in place through an atomic, rollback-backed transaction",
        ),
        ("diff", "Print a unified diff of the changes fix would make"),
        (
            "scan",
            "List every comment with its kind, disposition and byte span",
        ),
        (
            "strip",
            "Read source on stdin and write the stripped result to stdout",
        ),
        ("lsp", "Run the LSP 3.18 server over stdio"),
        (
            "init",
            "Write a starter .ocomment.toml or Lefthook configuration",
        ),
        (
            "config",
            "Show, locate, explain, or export the resolved configuration",
        ),
        (
            "languages",
            "List built-in languages, extensions, and dialects",
        ),
        ("plugin", "Manage sandboxed WASM scanner plugins"),
        ("completions", "Generate shell completions"),
        (
            "doctor",
            "Diagnose the environment (config, git, plugins, tools)",
        ),
        ("man", "Render the roff manual page to stdout"),
    ];
    for (name, description) in expected {
        let found = listed
            .iter()
            .find(|(listed_name, _)| listed_name == name)
            .unwrap_or_else(|| panic!("subcommand `{name}` is missing from:\n{help}"));
        assert_eq!(found.1, description, "wrong description for `{name}`");
    }
    assert!(
        listed.iter().any(|(name, _)| name == "man"),
        "the `man` subcommand must be documented in:\n{help}"
    );
}

#[test]
fn help_documents_exit_status_files_and_examples() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--help"]);
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8(output.stdout).unwrap();
    for needle in [
        "EXIT STATUS",
        "FILES",
        "EXAMPLES",
        ".ocomment.toml",
        ".ocommentignore",
        ".ocomment.lock",
    ] {
        assert!(help.contains(needle), "`--help` lacks {needle}:\n{help}");
    }
}

#[test]
fn check_help_groups_options_and_lists_possible_values() {
    let directory = tempfile::tempdir().unwrap();
    let short = run(directory.path(), &["check", "-h"]);
    assert_eq!(short.status.code(), Some(0));
    let short = String::from_utf8(short.stdout).unwrap();
    assert!(
        short.contains("[possible values: safe, legal, all]"),
        "`check -h` lacks the policy values:\n{short}"
    );
    assert!(short.contains("Policy:"), "no Policy heading:\n{short}");
    assert!(short.contains("Output:"), "no Output heading:\n{short}");

    let long = run(directory.path(), &["check", "--help"]);
    assert_eq!(long.status.code(), Some(0));
    let long = String::from_utf8(long.stdout).unwrap();
    assert!(long.contains("Policy:"), "no Policy heading:\n{long}");
    assert!(long.contains("Output:"), "no Output heading:\n{long}");
    for needle in [
        "- safe:",
        "- legal:",
        "- all:",
        "- lines:",
        "- rust:",
        "- doc-line:",
    ] {
        assert!(
            long.contains(needle),
            "`check --help` lacks documented value {needle}:\n{long}"
        );
    }
}

#[test]
fn unknown_policy_value_reports_the_possible_values() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--policy", "foo"]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("invalid value 'foo'"), "{error}");
    assert!(
        error.contains("[possible values: safe, legal, all]"),
        "{error}"
    );
}

#[test]
fn language_aliases_are_accepted_on_the_command_line() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    for alias in ["rs", "c++", "RUST"] {
        let output = run(
            directory.path(),
            &["check", "sample.rs", "--language", alias],
        );
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            !error.contains("invalid value"),
            "`--language {alias}` was rejected: {error}"
        );
        assert_eq!(
            output.status.code(),
            Some(1),
            "`--language {alias}`: {error}"
        );
    }
}

#[test]
fn unsupported_dialect_names_the_supported_ones() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(
        directory.path(),
        &[
            "check",
            "sample.rs",
            "--language",
            "rust",
            "--dialect",
            "jsx",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("unsupported dialect `jsx` for rust"),
        "{error}"
    );
    assert!(error.contains("supported: standard"), "{error}");
}

#[test]
fn man_subcommand_renders_a_roff_page() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["man"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let page = String::from_utf8(output.stdout).unwrap();
    /* NOTE: roff requires the `\*(Aq` string definition before the title macro, so
     * `.TH` is the first macro that is not a string definition. */
    let header = page
        .lines()
        .find(|line| !line.starts_with(".ie ") && !line.starts_with(".el "))
        .unwrap_or_default();
    assert!(
        header.starts_with(".TH"),
        "man page starts with:\n{page:.120}"
    );
    assert!(
        header.contains("ocomment"),
        "the .TH header does not name the tool"
    );
    assert!(page.contains(".SH NAME"), "man page has no NAME section");
}

/// The shipped page had an uppercase title, the "User Commands" manual, and a
/// SEE ALSO pointer; the generated page must keep all three.
#[test]
fn man_page_keeps_the_shipped_title_manual_and_see_also() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["man"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let page = String::from_utf8(output.stdout).unwrap();
    for needle in [
        ".TH OCOMMENT 1",
        "User Commands",
        "SEE ALSO",
        "The complete schemas and guides are available in the OComment repository.",
    ] {
        assert!(page.contains(needle), "man page lacks {needle}:\n{page}");
    }
}

#[test]
fn bash_completions_carry_the_policy_values() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["completions", "bash"]);
    assert_eq!(output.status.code(), Some(0));
    let script = String::from_utf8(output.stdout).unwrap();
    for value in ["safe", "legal", "all"] {
        assert!(
            script.contains(value),
            "bash completions lack the policy value {value}"
        );
    }
}

/// Rust `Debug` spellings that must never reach a terminal again. Bare
/// `Remove` is checked separately: SARIF legitimately says "Remove comment
/// with OComment" in its fix description.
const DEBUG_LEAKS: [&str; 3] = ["DocBlock", "Keep {", "Shebang"];

fn assert_no_debug_leak(context: &str, text: &str) {
    for leak in DEBUG_LEAKS {
        assert!(
            !text.contains(leak),
            "{context} leaks the Rust Debug token `{leak}`:\n{text}"
        );
    }
}

#[test]
fn human_check_names_comment_kinds_in_canonical_spelling() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("doc.rs"),
        b"/** doc */\nfn main() {}\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "doc.rs"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("removable doc-block comment"),
        "check output is:\n{stdout}"
    );
    assert_no_debug_leak("human check output", &stdout);
    assert!(!stdout.contains("Remove"), "check output is:\n{stdout}");
}

#[test]
fn human_scan_lines_use_canonical_kinds_and_dispositions() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("a.py"),
        b"#!/usr/bin/env python3\nx = 1  # remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["scan", "a.py"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout,
        "a.py:1:1: shebang keep (required source preamble) 0..22: #!/usr/bin/env python3\n\
         a.py:2:8: line remove 30..38: # remove\n"
    );
    assert_no_debug_leak("human scan output", &stdout);
}

#[test]
fn human_diagnostics_lowercase_the_severity() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("broken.c"), b"int x; /* open").unwrap();
    let output = run(directory.path(), &["check", "broken.c"]);
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("error[unterminated-comment]: unterminated block comment"),
        "check output is:\n{stdout}"
    );
    assert!(
        !stdout.contains("Error["),
        "check output still Debug-prints the severity:\n{stdout}"
    );
}

#[test]
fn config_explain_prints_canonical_policy_and_layout() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["config", "explain"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("policy: safe; layout: lines"),
        "config explain output is:\n{stdout}"
    );
    /* NOTE: Only the policy line is pinned: the surrounding lines print filesystem
     * paths that may legitimately contain any spelling. */
    let policy_line = stdout
        .lines()
        .find(|line| line.starts_with("policy:"))
        .unwrap_or_else(|| panic!("config explain has no policy line:\n{stdout}"));
    assert!(
        !policy_line.contains("Safe") && !policy_line.contains("Lines"),
        "config explain still Debug-prints the enums:\n{policy_line}"
    );
}

#[test]
fn github_annotations_use_kebab_comment_kinds() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("doc.rs"),
        b"/** doc */\nfn main() {}\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "doc.rs", "--format", "github"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout,
        "::notice file=doc.rs,line=1,col=1::removable doc-block comment\n"
    );
    assert_no_debug_leak("github annotations", &stdout);
    assert!(!stdout.contains("Remove"), "github output is:\n{stdout}");
}

#[test]
fn sarif_keeps_kebab_rule_ids_and_canonical_messages() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("doc.rs"),
        b"/** doc */\nfn main() {}\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "doc.rs", "--format", "sarif"]);
    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let result = &value["runs"][0]["results"][0];
    assert_eq!(result["ruleId"], "removable-doc-block");
    assert_eq!(result["message"]["text"], "removable doc-block comment");
    assert_no_debug_leak("SARIF report", &String::from_utf8(output.stdout).unwrap());
}

/// A SARIF `fix` is an offer to rewrite the file, and a tool that takes it up
/// has to end with the bytes `ocomment fix` would have written. Under
/// `--layout compact` the edit is wider than the comment — a comment alone on
/// its line takes the whole line with it — so a fix cut to the comment's own
/// span would delete the comment and leave the blank line behind, which is the
/// output of a layout nobody asked for. The region reported is the edit's.
#[test]
fn a_sarif_fix_reproduces_what_fix_writes_under_compact_layout() {
    let directory = tempfile::tempdir().unwrap();
    let source = "fn main() {\n    // note\n    let x = 1; // trailing\n}\n";
    fs::write(directory.path().join("main.rs"), source).unwrap();
    let reported = run(
        directory.path(),
        &[
            "check", "main.rs", "--layout", "compact", "--format", "sarif",
        ],
    );
    assert_eq!(reported.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&reported.stdout).unwrap();
    let results = value["runs"][0]["results"].as_array().unwrap();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    for result in results {
        let replacement = &result["fixes"][0]["artifactChanges"][0]["replacements"][0];
        let region = &replacement["deletedRegion"];
        let number = |name: &str| usize::try_from(region[name].as_u64().unwrap()).unwrap();
        replacements.push((
            byte_offset(source, number("startLine"), number("startColumn")),
            byte_offset(source, number("endLine"), number("endColumn")),
            replacement["insertedContent"]["text"]
                .as_str()
                .unwrap()
                .to_owned(),
        ));
    }
    // NOTE: The whole-line comment: from the start of its line to the start of
    // NOTE: the next one, so the line itself goes rather than being blanked.
    assert_eq!(
        replacements[0],
        (12, 24, String::new()),
        "the whole-line comment's fix is not the compact edit"
    );
    let mut patched = String::new();
    let mut cursor = 0;
    for (start, end, inserted) in &replacements {
        assert!(*start >= cursor, "the SARIF fixes overlap");
        patched.push_str(&source[cursor..*start]);
        patched.push_str(inserted);
        cursor = *end;
    }
    patched.push_str(&source[cursor..]);
    let fixed = run(directory.path(), &["fix", "main.rs", "--layout", "compact"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    let written = fs::read_to_string(directory.path().join("main.rs")).unwrap();
    assert_eq!(
        patched, written,
        "applying the SARIF fixes is not what `ocomment fix --layout compact` writes"
    );
}

/// Where a 1-based line and a 1-based byte column land in the source, so a
/// SARIF region can be turned back into the bytes it names.
fn byte_offset(source: &str, line: usize, column: usize) -> usize {
    let mut offset = 0;
    for _ in 1..line {
        offset += source[offset..]
            .find('\n')
            .expect("the region names a line the source has")
            + 1;
    }
    offset + column - 1
}

/// `strip` writes the stripped source and `config` answers a question about
/// the configuration; neither has a report to render, so a format that
/// describes one is refused rather than silently ignored — the way `languages`
/// refuses the same flags.
#[test]
fn strip_and_config_refuse_the_formats_they_cannot_honour() {
    let directory = tempfile::tempdir().unwrap();
    for format in ["json", "jsonl", "sarif", "github"] {
        let stripped = run_stdin(
            directory.path(),
            &["strip", "--language", "rust", "--format", format],
            b"let x = 1; // note\n",
        );
        assert_eq!(
            stripped.status.code(),
            Some(2),
            "`strip --format {format}` was accepted"
        );
        assert!(
            stripped.stdout.is_empty(),
            "`strip --format {format}` stripped the source anyway"
        );
        let error = String::from_utf8(stripped.stderr).unwrap();
        assert!(
            error.contains("`ocomment strip` is only available with --format human"),
            "`strip --format {format}` said:\n{error}"
        );
        for action in ["show", "locate", "explain", "schema"] {
            let answered = run(directory.path(), &["config", action, "--format", format]);
            assert_eq!(
                answered.status.code(),
                Some(2),
                "`config {action} --format {format}` was accepted"
            );
            assert!(
                answered.stdout.is_empty(),
                "`config {action} --format {format}` answered anyway"
            );
            let error = String::from_utf8(answered.stderr).unwrap();
            assert!(
                error.contains("`ocomment config` is only available with --format human"),
                "`config {action} --format {format}` said:\n{error}"
            );
        }
    }
    let stripped = run_stdin(
        directory.path(),
        &["strip", "--language", "rust", "--format", "human"],
        b"let x = 1; // note\n",
    );
    assert_eq!(
        stripped.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&stripped.stderr)
    );
    assert_eq!(stripped.stdout, b"let x = 1; \n");
    for action in ["show", "locate", "explain", "schema"] {
        let answered = run(directory.path(), &["config", action, "--format", "human"]);
        assert_eq!(
            answered.status.code(),
            Some(0),
            "`config {action}` was refused: {}",
            String::from_utf8_lossy(&answered.stderr)
        );
        assert!(
            !answered.stdout.is_empty(),
            "`config {action}` answered with nothing"
        );
    }
}

/// Every comment kind a rule id can name, in the spelling `CommentKind`
/// serialises. A kind added without a rule to describe it fails this test.
const SARIF_KINDS: [&str; 11] = [
    "line",
    "block",
    "doc-line",
    "doc-block",
    "directive",
    "license",
    "html-comment",
    "shebang",
    "encoding",
    "optimizer-hint",
    "version-comment",
];

/// The SARIF failure levels OComment reports at. `none` is a level too, but
/// nothing OComment writes uses it.
const SARIF_LEVELS: [&str; 3] = ["error", "warning", "note"];

/// A code-scanning UI shows a finding through the rule it names: the title, the
/// sentence under it, and the link it offers all come from
/// `tool.driver.rules`, which a result reaches by `ruleIndex`. A rule the tool
/// never describes leaves the finding with nothing but its id, so every id a
/// run can emit is described and every result points at its own description.
#[test]
fn sarif_describes_every_rule_it_reports() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("a.rs"), b"let value = 1; // remove\n").unwrap();
    fs::write(directory.path().join("bad.rs"), b"/* never ends\n").unwrap();
    fs::write(directory.path().join("plain.txt"), b"nothing to scan\n").unwrap();
    let output = run(
        directory.path(),
        &["check", "a.rs", "bad.rs", "plain.txt", "--format", "sarif"],
    );
    assert_eq!(output.status.code(), Some(2));
    let report = String::from_utf8(output.stdout).unwrap();
    let document: serde_json::Value = serde_json::from_str(&report).unwrap();
    let driver = &document["runs"][0]["tool"]["driver"];
    assert_eq!(driver["name"], "ocomment");
    assert_eq!(
        driver["version"],
        env!("CARGO_PKG_VERSION"),
        "the driver does not report the version that produced the run:\n{report}"
    );
    assert_eq!(
        driver["informationUri"],
        "https://github.com/P4suta/OComment"
    );

    let rules = driver["rules"]
        .as_array()
        .unwrap_or_else(|| panic!("tool.driver.rules is not an array:\n{report}"));
    let mut described = BTreeSet::new();
    for rule in rules {
        let id = rule["id"]
            .as_str()
            .unwrap_or_else(|| panic!("a rule has no string id:\n{report}"));
        assert!(described.insert(id.to_owned()), "`{id}` is described twice");
        for field in ["shortDescription", "fullDescription"] {
            let text = rule[field]["text"].as_str().unwrap_or_default();
            assert!(!text.is_empty(), "rule `{id}` has no {field}:\n{report}");
        }
        let help = rule["helpUri"].as_str().unwrap_or_default();
        assert!(
            help.starts_with("https://"),
            "rule `{id}` links nowhere: {help:?}"
        );
        let level = rule["defaultConfiguration"]["level"]
            .as_str()
            .unwrap_or_default();
        assert!(
            SARIF_LEVELS.contains(&level),
            "rule `{id}` defaults to {level:?}, which is not a SARIF level"
        );
    }
    let removable: BTreeSet<String> = described
        .iter()
        .filter(|id| id.starts_with("removable-"))
        .cloned()
        .collect();
    let expected: BTreeSet<String> = SARIF_KINDS
        .iter()
        .map(|kind| format!("removable-{kind}"))
        .collect();
    assert_eq!(
        removable, expected,
        "the rules do not describe exactly one removable kind each"
    );
    let doc_block = rules
        .iter()
        .find(|rule| rule["id"] == "removable-doc-block")
        .expect("`removable-doc-block` is described");
    assert_eq!(
        doc_block["shortDescription"]["text"],
        "Removable doc-block comment"
    );
    assert_eq!(doc_block["defaultConfiguration"]["level"], "note");

    let results = document["runs"][0]["results"].as_array().unwrap();
    let mut reported = BTreeSet::new();
    for result in results {
        let id = result["ruleId"]
            .as_str()
            .unwrap_or_else(|| panic!("a result has no ruleId:\n{report}"));
        let index = result["ruleIndex"]
            .as_u64()
            .unwrap_or_else(|| panic!("the `{id}` result has no ruleIndex:\n{report}"));
        assert_eq!(
            rules[index as usize]["id"], id,
            "the `{id}` result points at rule {index}, which describes something else"
        );
        let level = result["level"].as_str().unwrap_or_default();
        assert!(
            SARIF_LEVELS.contains(&level),
            "the `{id}` result is at level {level:?}, which is not a SARIF level"
        );
        reported.insert(id.to_owned());
    }
    for id in [
        "removable-line",
        "removable-block",
        "unterminated-comment",
        "skipped-file",
    ] {
        assert!(
            reported.contains(id),
            "the run reported no `{id}` result:\n{report}"
        );
    }
    assert_no_debug_leak("SARIF report", &report);
}

/// A file OComment cannot read is reported as a result too, and it names a rule
/// like any other finding.
#[cfg(unix)]
#[test]
fn sarif_describes_the_io_error_rule_when_it_reports_one() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let unreadable = directory.path().join("locked.rs");
    fs::write(&unreadable, b"let value = 1; // remove\n").unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
    let output = run(
        directory.path(),
        &["check", "locked.rs", "--format", "sarif"],
    );
    assert_eq!(output.status.code(), Some(2));
    let report = String::from_utf8(output.stdout).unwrap();
    let document: serde_json::Value = serde_json::from_str(&report).unwrap();
    let result = &document["runs"][0]["results"][0];
    assert_eq!(result["ruleId"], "io-error");
    assert_eq!(result["level"], "error");
    let index = result["ruleIndex"]
        .as_u64()
        .unwrap_or_else(|| panic!("the io-error result has no ruleIndex:\n{report}"));
    let rule = &document["runs"][0]["tool"]["driver"]["rules"][index as usize];
    assert_eq!(rule["id"], "io-error");
    assert_eq!(rule["defaultConfiguration"]["level"], "error");
}

/// A code-scanning UI resolves `artifactLocation.uri` against the checkout, so
/// a reported path has to be spelled the way the repository spells it: forward
/// slashes, no `./` standing in for the directory the run started in, and
/// `%SRCROOT%` saying what the rest is relative to. Every location in the
/// document is read that way, the ones under `fixes` included.
#[test]
fn sarif_locates_reported_files_under_the_source_root() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("sub")).unwrap();
    fs::write(
        directory.path().join("sub/doc.rs"),
        b"/** doc */\nfn main() {}\n",
    )
    .unwrap();
    let output = run(
        directory.path(),
        &["check", "sub/./doc.rs", "--format", "sarif"],
    );
    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).unwrap();
    let document: serde_json::Value = serde_json::from_str(&report).unwrap();
    let locations = artifact_locations(&document);
    assert!(
        !locations.is_empty(),
        "the report locates nothing:\n{report}"
    );
    for location in &locations {
        let uri = location["uri"].as_str().unwrap_or_default();
        assert_eq!(uri, "sub/doc.rs", "a location is spelled {uri:?}");
        assert_eq!(
            location["uriBaseId"], "%SRCROOT%",
            "a relative location says nothing about what it is relative to:\n{report}"
        );
    }
}

/// A path the user typed as an absolute one is not under the checkout, so it
/// keeps its absolute spelling and names no base id — a base id would say it is
/// relative to the source root, which it is not.
#[test]
fn sarif_leaves_an_absolute_path_absolute_and_unbased() {
    let directory = tempfile::tempdir().unwrap();
    let absolute = directory.path().join("a.rs");
    fs::write(&absolute, b"let value = 1; // remove\n").unwrap();
    let output = run(
        directory.path(),
        &["check", absolute.to_str().unwrap(), "--format", "sarif"],
    );
    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).unwrap();
    let document: serde_json::Value = serde_json::from_str(&report).unwrap();
    let expected = absolute.to_string_lossy().replace('\\', "/");
    for location in artifact_locations(&document) {
        assert_eq!(location["uri"], expected);
        assert!(
            location.get("uriBaseId").is_none(),
            "an absolute location claims a base id:\n{report}"
        );
    }
}

/// Standard input has no place in the checkout either, so the pseudo-path it is
/// reported under is left alone rather than resolved against the source root.
#[test]
fn sarif_leaves_the_stdin_pseudo_path_unbased() {
    let directory = tempfile::tempdir().unwrap();
    let output = run_stdin(
        directory.path(),
        &["check", "-", "--language", "rust", "--format", "sarif"],
        b"let value = 1; // remove\n",
    );
    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).unwrap();
    let document: serde_json::Value = serde_json::from_str(&report).unwrap();
    for location in artifact_locations(&document) {
        assert_eq!(location["uri"], "<stdin>");
        assert!(
            location.get("uriBaseId").is_none(),
            "the standard-input pseudo-path claims a base id:\n{report}"
        );
    }
}

/// A relative URI is read as a URI, and RFC 3986 gives a first segment holding
/// a colon back to the scheme: `c:/a.rs` parses as the scheme `c` rather than
/// as a path, and a Windows reader sees a drive letter in it besides. A POSIX
/// checkout is free to hold a directory named `c:`, so the emitter puts the one
/// `.` segment a URI is allowed to keep in front of that path — `./c:/a.rs`,
/// still measured from `%SRCROOT%` — and no reader can misread it.
///
/// A GitHub annotation is matched against the paths the repository uses rather
/// than parsed as a URI, so `file=` keeps the plain spelling — with the `%3A`
/// the annotation format already owes a colon, which is a property delimiter
/// there.
#[cfg(unix)]
#[test]
fn sarif_disambiguates_a_leading_segment_that_reads_as_a_drive_letter() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("c:")).unwrap();
    fs::write(
        directory.path().join("c:/a.rs"),
        b"let value = 1; // remove\n",
    )
    .unwrap();

    let output = run(directory.path(), &["check", "c:/a.rs", "--format", "sarif"]);
    let report = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "`check --format sarif` said:\n{report}"
    );
    let document: serde_json::Value = serde_json::from_str(&report).unwrap();
    let locations = artifact_locations(&document);
    assert!(
        !locations.is_empty(),
        "the report locates nothing:\n{report}"
    );
    for location in &locations {
        assert_eq!(
            location["uri"], "./c:/a.rs",
            "a drive-letter first segment was left ambiguous:\n{report}"
        );
        assert_eq!(
            location["uriBaseId"], "%SRCROOT%",
            "the disambiguated path lost the base it is measured from:\n{report}"
        );
    }

    let annotated = run(
        directory.path(),
        &["check", "c:/a.rs", "--format", "github"],
    );
    let stdout = String::from_utf8(annotated.stdout).unwrap();
    assert!(
        stdout.contains("::notice file=c%3A/a.rs,"),
        "a GitHub annotation lost the path the repository spells:\n{stdout}"
    );
}

/// Every `artifactLocation` in a SARIF document, from the locations a result
/// reports and from the changes its fix would make.
fn artifact_locations(document: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut found = Vec::new();
    for run in document["runs"].as_array().into_iter().flatten() {
        for result in run["results"].as_array().into_iter().flatten() {
            for location in result["locations"].as_array().into_iter().flatten() {
                found.push(location["physicalLocation"]["artifactLocation"].clone());
            }
            for fix in result["fixes"].as_array().into_iter().flatten() {
                for change in fix["artifactChanges"].as_array().into_iter().flatten() {
                    found.push(change["artifactLocation"].clone());
                }
            }
        }
    }
    found
}

/// GitHub matches an annotation to a line of the diff by the path in `file=`,
/// and matches it against the paths the repository uses. A `./` the walk left
/// behind is enough to lose the annotation.
#[test]
fn github_annotations_report_repository_paths() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("sub")).unwrap();
    fs::write(
        directory.path().join("sub/doc.rs"),
        b"/** doc */\nfn main() {}\n",
    )
    .unwrap();
    let output = run(
        directory.path(),
        &["check", "sub/./doc.rs", "--format", "github"],
    );
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout,
        "::notice file=sub/doc.rs,line=1,col=1::removable doc-block comment\n"
    );
}

#[test]
fn json_and_jsonl_serde_names_are_frozen() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.py"),
        b"#!/usr/bin/env python3\n# SPDX-License-Identifier: MIT\nx = 1  # remove\n",
    )
    .unwrap();

    let jsonl = run(
        directory.path(),
        &["scan", "sample.py", "--format", "jsonl"],
    );
    assert_eq!(jsonl.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(jsonl.stdout).unwrap(),
        concat!(
            r#"{"path":"sample.py","language":"python","changed":true,"report":{"language":"python","#,
            r#""comments":[{"span":{"start":0,"end":22},"kind":"shebang","disposition":{"action":"keep","#,
            r#""reason":"required source preamble"}},{"span":{"start":23,"end":53},"kind":"license","#,
            r#""disposition":{"action":"remove"}},{"span":{"start":61,"end":69},"kind":"line","#,
            r#""disposition":{"action":"remove"}}],"diagnostics":[],"valid":true},"#,
            r#""edits":[{"span":{"start":23,"end":53},"replacement":""},"#,
            r#"{"span":{"start":61,"end":69},"replacement":""}],"#,
            r#""source_map":{"segments":[{"original":{"start":0,"end":23},"output":{"start":0,"end":23},"exact":true},"#,
            r#"{"original":{"start":23,"end":53},"output":{"start":23,"end":23},"exact":false},"#,
            r#"{"original":{"start":53,"end":61},"output":{"start":23,"end":31},"exact":true},"#,
            r#"{"original":{"start":61,"end":69},"output":{"start":31,"end":31},"exact":false},"#,
            r#"{"original":{"start":69,"end":70},"output":{"start":31,"end":32},"exact":true}]}}"#,
            "\n"
        ),
        "the JSONL protocol changed"
    );

    let json = run(directory.path(), &["scan", "sample.py", "--format", "json"]);
    assert_eq!(json.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let comments = value["files"][0]["report"]["comments"].as_array().unwrap();
    assert_eq!(
        comments
            .iter()
            .map(|comment| comment["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["shebang", "license", "line"]
    );
    assert_eq!(comments[0]["disposition"]["action"], "keep");
    assert_eq!(
        comments[0]["disposition"]["reason"],
        "required source preamble"
    );
    assert_eq!(comments[1]["disposition"]["action"], "remove");
    assert_eq!(value["files"][0]["language"], "python");
}

#[test]
fn json_diagnostics_keep_the_lower_case_serde_severity() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("broken.c"), b"int x; /* open").unwrap();
    let output = run(directory.path(), &["scan", "broken.c", "--format", "json"]);
    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostic = &value["files"][0]["report"]["diagnostics"][0];
    assert_eq!(diagnostic["severity"], "error");
    assert_eq!(diagnostic["code"], "unterminated-comment");
}

/// The end-of-run summary belongs on standard error so that `check` keeps a
/// grep-able `path:line:col` stream on standard output.
#[test]
fn check_writes_its_summary_to_standard_error() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "sample.rs"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stdout.contains("removable line comment"),
        "check output is:\n{stdout}"
    );
    assert!(
        !stdout.contains("Found"),
        "the summary leaked onto stdout:\n{stdout}"
    );
    assert_eq!(
        stderr,
        "Found 1 removable comment in 1 file (1 file scanned). \
         Run `ocomment fix` to remove it.\n"
    );
}

#[test]
fn a_clean_check_summarizes_the_files_it_scanned() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("sample.rs"), b"let x = 1;\n").unwrap();
    let output = run(directory.path(), &["check", "sample.rs"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "No removable comments in 1 file.\n"
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
}

/// `diff` must keep standard output a clean patch.
#[test]
fn diff_keeps_the_patch_on_stdout_and_summarizes_on_stderr() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["diff", "sample.rs"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("--- a/sample.rs"), "diff is:\n{stdout}");
    assert!(
        !stdout.contains("Found"),
        "the summary leaked into the patch:\n{stdout}"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Found 1 removable comment in 1 file (1 file scanned). \
         Run `ocomment fix` to apply the patch.\n"
    );
}

#[test]
fn fix_reports_every_changed_file_and_summarizes_on_stderr() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["fix", "sample.rs"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "fixed sample.rs: removed 1 comment\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Removed 1 comment in 1 file (1 file scanned).\n"
    );
}

#[test]
fn a_clean_fix_says_there_was_nothing_to_do() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("sample.rs"), b"let x = 1;\n").unwrap();
    let output = run(directory.path(), &["fix", "sample.rs"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Nothing to fix in 1 file.\n"
    );
}

#[test]
fn scan_summarizes_the_comment_counts() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("a.py"),
        b"#!/usr/bin/env python3\nx = 1  # remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["scan", "a.py"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Scanned 1 file: 2 comments (1 removable, 1 kept).\n"
    );
}

#[test]
fn quiet_silences_a_check_that_still_exits_one() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "-q", "sample.rs"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn quiet_and_verbose_cannot_be_combined() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["check", "-q", "-v"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot be used with"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn verbose_traces_the_root_target_config_and_kinds() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "-v", "sample.rs"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("root: "), "verbose trace is:\n{stderr}");
    assert!(
        stderr.contains("target: sample.rs"),
        "verbose trace is:\n{stderr}"
    );
    assert!(stderr.contains("config: "), "verbose trace is:\n{stderr}");
    assert!(
        stderr.contains("kinds: line 1 removable"),
        "verbose trace is:\n{stderr}"
    );
}

#[test]
fn directory_walks_fold_skipped_files_into_the_summary() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();
    let output = run(directory.path(), &["check", "."]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stdout.contains("notes.md"),
        "a walked skip was listed individually:\n{stdout}"
    );
    assert!(
        stderr.contains("1 file skipped (unknown language: 1; use -v to list)."),
        "summary is:\n{stderr}"
    );
}

#[test]
fn verbose_lists_the_folded_skips() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();
    let output = run(directory.path(), &["check", "-v", "."]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("notes.md: skipped: {NO_LANGUAGE}")),
        "verbose check output is:\n{stdout}"
    );
}

#[test]
fn an_explicit_unknown_language_argument_is_still_listed() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();
    let output = run(directory.path(), &["check", "notes.md"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("notes.md: skipped: {NO_LANGUAGE}")),
        "check output is:\n{stdout}"
    );
}

#[test]
fn machine_formats_never_emit_the_summary() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    for format in ["json", "jsonl", "sarif", "github"] {
        let output = run(
            directory.path(),
            &[
                "check",
                "sample.rs",
                "-v",
                "--progress",
                "always",
                "--format",
                format,
            ],
        );
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            "",
            "the {format} format wrote to standard error"
        );
    }
}

/// A single invalid file blocks the whole transaction, so the summary must not
/// claim removals that never reached the disk.
#[test]
fn a_blocked_fix_does_not_claim_removals() {
    let directory = tempfile::tempdir().unwrap();
    let good = directory.path().join("good.rs");
    let original = b"let x = 1; // remove\n";
    fs::write(&good, original).unwrap();
    fs::write(directory.path().join("broken.c"), b"int x; /* open").unwrap();
    let output = run(directory.path(), &["fix", "."]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(&good).unwrap(), original);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stdout.contains("fixed "),
        "fix claimed a write that was blocked:\n{stdout}"
    );
    assert!(
        stderr.contains("1 file has invalid syntax; nothing was written for it (use --force-invalid to apply known-safe edits)."),
        "summary is:\n{stderr}"
    );
    assert!(
        !stderr.contains("Removed "),
        "summary claims removals that were blocked:\n{stderr}"
    );
}

#[test]
fn staged_runs_also_emit_the_summary() {
    let directory = repository();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    git(directory.path(), &["add", "sample.rs"]);
    let output = run(directory.path(), &["check", "--staged"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Found 1 removable comment in 1 file (1 file scanned). \
         Run `ocomment fix` to remove it.\n"
    );
}

#[test]
fn help_lists_the_verbosity_and_progress_flags() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["check", "--help"]);
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8(output.stdout).unwrap();
    for needle in ["-q, --quiet", "-v, --verbose", "--progress <WHEN>"] {
        assert!(
            help.contains(needle),
            "`check --help` lacks {needle}:\n{help}"
        );
    }
    assert!(
        help.contains("live scanning counter"),
        "`--progress` does not say that it draws the live counter:\n{help}"
    );
    assert!(
        !help.contains("progress indicator"),
        "`--progress` describes the live counter it draws, not a vague \
         indicator:\n{help}"
    );
}

/// `-q` does not print nothing: it drops the commentary and keeps whatever the
/// command was asked to produce — the findings, the patch, the listing. A help
/// line that claims otherwise sends a reader hunting for output that was never
/// dropped, or piping a run they think is silent.
#[test]
fn quiet_help_says_what_it_keeps() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["check", "--help"]);
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(
        help.contains("Drop the run summary and notes"),
        "`-q` does not say what it drops:\n{help}"
    );
    assert!(
        help.contains("still written"),
        "`-q` does not say what it keeps:\n{help}"
    );
    assert!(
        !help.contains("Print nothing but errors"),
        "`-q` still claims to print nothing:\n{help}"
    );
}

/// Fill a directory with `count` one-comment Rust files.
fn many_files(count: usize) -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    for index in 0..count {
        fs::write(
            directory.path().join(format!("file{index:03}.rs")),
            b"let x = 1; // remove\n",
        )
        .unwrap();
    }
    directory
}

/// `--progress always` draws a live counter on standard error and still leaves
/// the end-of-run summary readable once the counter line is cleared.
#[test]
fn progress_always_draws_a_live_counter_and_keeps_the_summary() {
    let directory = many_files(120);
    let output = run(directory.path(), &["check", "--progress", "always", "."]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("ocomment: scanning 120/120 files"),
        "progress counter is missing from:\n{stderr:?}"
    );
    assert!(
        stderr.contains("\r\x1b[2K"),
        "the counter line was never cleared:\n{stderr:?}"
    );
    assert!(
        stderr.ends_with(
            "Found 120 removable comments in 120 files (120 files scanned). \
             Run `ocomment fix` to remove them.\n"
        ),
        "summary is missing from:\n{stderr:?}"
    );
}

#[test]
fn progress_never_draws_nothing_and_quiet_wins_over_progress() {
    let directory = many_files(120);
    let output = run(directory.path(), &["check", "--progress", "never", "."]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stderr.contains("scanning"),
        "`--progress never` still drew a counter:\n{stderr:?}"
    );
    let quiet = run(
        directory.path(),
        &["check", "-q", "--progress", "always", "."],
    );
    assert_eq!(quiet.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(quiet.stderr).unwrap(),
        "",
        "`-q` did not silence the progress counter"
    );
}

/// The human report says what the comment is, not merely that one is there.
#[test]
fn check_previews_the_comment_text_on_the_reported_line() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // TODO remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "sample.rs"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(": removable line comment: // TODO remove"),
        "check output is:\n{stdout}"
    );
    assert_eq!(
        stdout,
        "sample.rs:1:12: removable line comment: // TODO remove\n"
    );
}

#[test]
fn no_preview_restores_the_bare_reported_line() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // TODO remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "--no-preview", "sample.rs"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "sample.rs:1:12: removable line comment\n"
    );
}

/// `scan` previews too, and a multi-line comment stays on one line.
#[test]
fn scan_previews_the_comment_text_folded_onto_one_line() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("block.c"),
        b"int x; /* first\n   second */\n",
    )
    .unwrap();
    let output = run(directory.path(), &["scan", "block.c"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with("block.c:1:8: block "),
        "scan output is:\n{stdout}"
    );
    assert!(
        stdout.contains("7..28: /* first second */\n"),
        "scan output is:\n{stdout}"
    );
    assert_eq!(stdout.lines().count(), 1, "scan output is:\n{stdout}");
}

/// A comment carrying terminal escapes must not be able to drive the terminal.
#[test]
fn a_previewed_comment_cannot_inject_escape_sequences() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("evil.rs"),
        b"let x = 1; // \x1b[31mred\x1b[0m boom\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "evil.rs"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains('\x1b'),
        "an escape byte reached the terminal:\n{stdout:?}"
    );
    assert!(
        stdout.contains("// \u{fffd}[31mred\u{fffd}[0m boom"),
        "check output is:\n{stdout:?}"
    );
}

/// A file name is chosen by whoever made the file, so the half of a report
/// line that shows a path is untrusted input on its way to a terminal exactly
/// like the preview beside it. It gets the same treatment, and is cut nowhere:
/// a path ending in an ellipsis names no file.
#[cfg(unix)]
#[test]
fn a_reported_path_cannot_inject_escape_sequences() {
    use std::os::unix::ffi::OsStringExt;

    let directory = tempfile::tempdir().unwrap();
    let name = std::ffi::OsString::from_vec(b"evil\x1b[2Jname.rs".to_vec());
    fs::write(directory.path().join(&name), b"let x = 1; // remove me\n").unwrap();
    let unreadable = std::ffi::OsString::from_vec(b"evil\x1b[2Jskip.bin".to_vec());
    fs::write(directory.path().join(&unreadable), b"\x00\x01binary\n").unwrap();

    let output = run(directory.path(), &["check", "-v", "."]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.contains(&0x1b),
        "an escape byte reached the report: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !output.stderr.contains(&0x1b),
        "an escape byte reached the summary: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("evil\u{fffd}[2Jname.rs:1:12: removable line comment"),
        "the report lost the file it names:\n{stdout}"
    );
    assert!(
        stdout.contains("evil\u{fffd}[2Jskip.bin: skipped: binary file"),
        "the skip lost the file it names:\n{stdout}"
    );

    /* NOTE: Asked for hyperlinks, the report writes escape bytes of its own: the
     * OSC 8 frame is delimited by them. They are the only ones it may write.
     * The name goes into the frame's *target* as well as its text, so the
     * target percent-encodes what it is given rather than forwarding it. */
    let linked = run(
        directory.path(),
        &["check", "-v", ".", "--hyperlinks", "always"],
    );
    assert_eq!(
        linked.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let linked_stdout = String::from_utf8(linked.stdout).unwrap();
    assert!(
        linked_stdout.contains("\x1b]8;;file://"),
        "no hyperlink was written to link a path with:\n{linked_stdout}"
    );
    let unframed = linked_stdout.replace("\x1b]8;;", "").replace("\x1b\\", "");
    assert!(
        !unframed.contains('\x1b'),
        "an escape byte reached the report outside the hyperlink frame: {unframed:?}"
    );
    assert!(
        linked_stdout.contains("%1B%5B2Jname.rs"),
        "the link target forwarded the name instead of encoding it:\n{linked_stdout}"
    );
}

/// A file name is not commentary. The spaces and tabs in it are the name — a
/// reader who cannot see them cannot type the name back, and a report that
/// quietly drops them names a file the checkout does not have. Every control
/// character is still replaced, the tab included, so the row stays one row.
#[cfg(unix)]
#[test]
fn a_reported_path_keeps_the_spacing_of_the_name_it_reports() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("ta\tb.rs"),
        b"let x = 1; // remove me\n",
    )
    .unwrap();
    fs::write(
        directory.path().join(" lead.rs"),
        b"let x = 1; // remove me\n",
    )
    .unwrap();

    let output = run(directory.path(), &["check", "."]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("ta\u{fffd}b.rs:1:12: removable line comment"),
        "the tab in a file name vanished from the report:\n{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line == " lead.rs:1:12: removable line comment: // remove me"),
        "the leading space in a file name vanished from the report:\n{stdout}"
    );
}

/// A directory and a file inside it are both named, so the walk meets the file
/// twice. It is one file: the report says so once, exactly as it does for a
/// file it can scan.
#[test]
fn a_file_reached_twice_is_skipped_once() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("plain.txt"), b"nothing to scan\n").unwrap();

    let output = run(
        directory.path(),
        &["check", ".", "plain.txt", "--format", "github"],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let annotations: Vec<_> = stdout
        .lines()
        .filter(|line| line.contains("plain.txt"))
        .collect();
    assert_eq!(
        annotations.len(),
        1,
        "one file was annotated more than once:\n{stdout}"
    );
    assert!(
        annotations[0].starts_with("::notice file=plain.txt,title=OComment skipped file::"),
        "the skip was not reported as a notice:\n{stdout}"
    );
}

/// A configuration file is read from the project, and the pattern in it is
/// echoed back on the line that rejects it. That makes it untrusted input on
/// its way to a terminal, and it is folded like every other one.
#[test]
fn an_invalid_policy_regex_cannot_inject_escape_sequences() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("a.rs"), b"let x = 1; // remove me\n").unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        "version = 1\n[policy]\nkeep_regex = [\"\\u001B[2J(\"]\n",
    )
    .unwrap();

    let output = run(directory.path(), &["check", "a.rs"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "an invalid regex was accepted:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stderr.contains(&0x1b),
        "an escape byte reached the terminal: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.contains("invalid comment policy regex `\u{fffd}[2J(`"),
        "the error lost the pattern it rejects:\n{error}"
    );
}

/// The same rule for the other pattern a project file carries. A `[files]`
/// glob is echoed back on the line that rejects it — twice, because `globset`
/// quotes the glob inside its own parse error — so both halves are folded
/// before either reaches a terminal.
#[test]
fn an_invalid_file_glob_cannot_inject_escape_sequences() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("a.rs"), b"let x = 1; // remove me\n").unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        "version = 1\n[files]\nexclude = [\"\\u001B[2J[\"]\n",
    )
    .unwrap();

    let output = run(directory.path(), &["check", "a.rs"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "an invalid glob was accepted:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stderr.contains(&0x1b),
        "an escape byte reached the terminal: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.contains("invalid file glob `\u{fffd}[2J[`"),
        "the error lost the pattern it rejects:\n{error}"
    );
    assert!(
        error.lines().count() == 1,
        "the error spread over more than one line:\n{error}"
    );
}

/// The GitHub renderer annotates a pull request, and an annotation costs the
/// reader a line in the checks tab. So it folds a skip away exactly as the
/// human renderer does: an I/O error and a path the caller named are always
/// worth saying, while a file a walk merely wandered past is `-v` material.
#[test]
fn github_annotations_fold_walked_skips_away_unless_asked() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("a.rs"), b"let x = 1; // remove\n").unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();

    let quiet = run(directory.path(), &["check", "--format", "github"]);
    let stdout = String::from_utf8(quiet.stdout).unwrap();
    assert!(stdout.contains("::notice file=a.rs"), "{stdout}");
    assert!(
        !stdout.contains("notes.md"),
        "a walked skip was annotated without -v:\n{stdout}"
    );

    let loud = run(directory.path(), &["check", "--format", "github", "-v"]);
    let verbose = String::from_utf8(loud.stdout).unwrap();
    assert!(
        verbose.contains("::notice file=notes.md,title=OComment skipped file::"),
        "-v lost the walked skip:\n{verbose}"
    );

    let named = run(
        directory.path(),
        &["check", "notes.md", "--format", "github"],
    );
    let explicit = String::from_utf8(named.stdout).unwrap();
    assert!(
        explicit.contains("::notice file=notes.md,title=OComment skipped file::"),
        "a path the caller named lost its annotation:\n{explicit}"
    );

    let missing = run(
        directory.path(),
        &["check", "gone.rs", "--format", "github"],
    );
    let failure = String::from_utf8(missing.stdout).unwrap();
    assert!(
        failure.contains("::error file=gone.rs,title=OComment I/O error::"),
        "an I/O error lost its annotation:\n{failure}"
    );
}

/// `-q` trims the human report down to what went wrong; it is a human-format
/// concept and has no business reaching a machine format. A GitHub annotation
/// is the product of `--format github`, so a hook that runs quietly still
/// annotates the path the caller named and the file it could not read — the
/// walked skip stays folded because `-v`, not `-q`, is what decides that.
#[test]
fn quiet_does_not_take_annotations_off_a_machine_format() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("a.rs"), b"let x = 1; // remove\n").unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();

    let named = run(
        directory.path(),
        &["check", "notes.md", "--format", "github", "-q"],
    );
    let explicit = String::from_utf8(named.stdout).unwrap();
    assert!(
        explicit.contains("::notice file=notes.md,title=OComment skipped file::"),
        "-q took the annotation off a path the caller named:\n{explicit}"
    );

    let missing = run(
        directory.path(),
        &["check", "gone.rs", "--format", "github", "-q"],
    );
    let failure = String::from_utf8(missing.stdout).unwrap();
    assert!(
        failure.contains("::error file=gone.rs,title=OComment I/O error::"),
        "-q took the annotation off an I/O error:\n{failure}"
    );

    let walked = run(directory.path(), &["check", "--format", "github", "-q"]);
    let stdout = String::from_utf8(walked.stdout).unwrap();
    assert!(stdout.contains("::notice file=a.rs"), "{stdout}");
    assert!(
        !stdout.contains("notes.md"),
        "a walked skip was annotated without -v:\n{stdout}"
    );
}

/// The `regex` crate writes a parse error over several lines, with a caret
/// under the byte it stopped at. The caret means nothing once the pattern is
/// folded, but the sentence after it is the whole answer, so the report keeps
/// every word of it and puts the lot on the one line an error is.
#[test]
fn an_invalid_policy_regex_is_reported_whole_on_one_line() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        "version = 1\n[policy]\nkeep_regex = [\"[\\u001Ba-\"]\n",
    )
    .unwrap();

    let output = run(directory.path(), &["check"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        !output.stderr.contains(&0x1b),
        "an escape byte reached the terminal: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let error = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        error.trim_end().lines().count(),
        1,
        "the parse error spilled over more than one line:\n{error}"
    );
    assert!(
        error.contains(
            "invalid comment policy regex `[\u{fffd}a-`: \
             regex parse error: [\u{fffd}a- ^ error: unclosed character class"
        ),
        "the parse error was not folded, or was cut short of the reason:\n{error}"
    );
}

/// The same rule for the file as a whole. A `toml` parse error quotes the
/// line it stopped on, and that line came out of a project file, so it carries
/// whatever bytes the file carries — an escape sequence among them — over four
/// lines of caret diagram. The verdict is one line, and every byte of it is
/// printable.
#[test]
fn an_invalid_configuration_is_reported_whole_on_one_line() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[files]\ninclude = [\"a\x07 b\"]\n",
    )
    .unwrap();

    let output = run(directory.path(), &["check"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        !output.stderr.contains(&0x07),
        "a control byte reached the terminal: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let error = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        error.trim_end().lines().count(),
        1,
        "the parse error spilled over more than one line:\n{error}"
    );
    assert!(
        error.contains("invalid configuration "),
        "the error lost the file it rejects:\n{error}"
    );
    assert!(
        error.contains("include = [\"a\u{fffd} b\"]"),
        "the quoted line was not folded, or lost the byte that is wrong with it:\n{error}"
    );
    assert!(
        error.contains("invalid basic string"),
        "the error was cut short of the reason:\n{error}"
    );
}

/// The other half of that line: the path in front of the colon.
///
/// A configuration file is named by the directory it was found in, and a
/// directory name carries whatever bytes the file system allowed — a `\x07`
/// that rings the terminal's bell among them. The name is still the answer to
/// "which file?", so it is printed rather than withheld, and it gets the
/// treatment every other path in the report gets.
#[test]
fn an_invalid_configuration_names_its_file_without_ringing_the_terminal() {
    let parent = tempfile::tempdir().unwrap();
    let directory = parent.path().join("ring\u{7}ing");
    fs::create_dir(&directory).unwrap();
    fs::write(
        directory.join(".ocomment.toml"),
        b"version = 1\n\n[files]\ninclude = [\n",
    )
    .unwrap();

    let output = run(&directory, &["check"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stderr.contains(&0x07),
        "a control byte from the path reached the terminal: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.contains("invalid configuration ") && error.contains(".ocomment.toml"),
        "the error lost the file it rejects:\n{error}"
    );
    assert!(
        error.contains("ring\u{fffd}ing"),
        "the directory the file was found in was dropped from the error:\n{error}"
    );
}

/// A long comment is cut to a readable width rather than flooding the report.
#[test]
fn a_long_comment_preview_is_truncated_with_an_ellipsis() {
    let directory = tempfile::tempdir().unwrap();
    let comment = "x".repeat(200);
    fs::write(
        directory.path().join("long.rs"),
        format!("let x = 1; // {comment}\n"),
    )
    .unwrap();
    let output = run(directory.path(), &["check", "long.rs"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let (_, preview) = stdout.trim_end().rsplit_once(": ").unwrap();
    assert!(preview.ends_with('…'), "preview is:\n{preview}");
    assert_eq!(preview.chars().count(), 72, "preview is:\n{preview}");
}

#[test]
fn help_documents_the_preview_switch() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["check", "--help"]);
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(
        help.contains("--no-preview"),
        "`check --help` lacks --no-preview:\n{help}"
    );
}

/// `clap_mangen` dumps `after_long_help` as one opaque `.SH EXTRA` blob; the
/// manual must carry the same content as real roff sections instead.
#[test]
fn man_page_renders_real_sections_instead_of_one_extra_blob() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["man"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let page = String::from_utf8(output.stdout).unwrap();
    for needle in [
        ".SH EXIT STATUS",
        ".SH FILES",
        ".SH EXAMPLES",
        ".SH SEE ALSO",
    ] {
        assert!(page.contains(needle), "man page lacks {needle}:\n{page}");
    }
    assert!(
        !page.contains(".SH EXTRA"),
        "the help blob is still dumped verbatim:\n{page}"
    );
}

/// A bidirectional override can make a comment render as its own reverse; the
/// preview must neutralize the whole format-control class, not only C0.
#[test]
fn a_previewed_comment_cannot_reorder_the_line_with_bidi_controls() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("bidi.rs"),
        "let x = 1; // \u{202e}drowssap\u{202c} end\n".as_bytes(),
    )
    .unwrap();
    let output = run(directory.path(), &["check", "bidi.rs"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains('\u{202e}'),
        "a bidi override reached the terminal:\n{stdout:?}"
    );
    assert_eq!(
        stdout,
        "bidi.rs:1:12: removable line comment: // \u{fffd}drowssap\u{fffd} end\n"
    );
}

/// An explicitly named skip already has its own line on standard output, so
/// the folded clause must not count it a second time.
#[test]
fn a_named_skip_is_not_counted_twice_in_the_summary() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();
    let output = run(directory.path(), &["check", "sample.rs", "notes.md"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stdout.contains(&format!("notes.md: skipped: {NO_LANGUAGE}")),
        "check output is:\n{stdout}"
    );
    assert_eq!(
        stderr,
        "Found 1 removable comment in 1 file (1 file scanned). \
         Run `ocomment fix` to remove it.\n"
    );
}

/// Scanning nothing at all is not "no removable comments in 0 files": say what
/// actually happened to the files that were passed over.
#[test]
fn a_run_that_scans_nothing_reports_the_skips_instead() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();
    let output = run(directory.path(), &["check", "."]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Nothing to check: 1 file skipped (unknown language: 1; use -v to list).\n"
    );
}

/// Every noun in the summary is pluralized; `file(s)` never reaches a user.
#[test]
fn the_summary_pluralizes_every_noun() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("a.rs"),
        b"let x = 1; // one\nlet y = 2; // two\n",
    )
    .unwrap();
    fs::write(directory.path().join("b.rs"), b"let z = 3; // three\n").unwrap();
    let output = run(directory.path(), &["check", "a.rs", "b.rs"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr,
        "Found 3 removable comments in 2 files (2 files scanned). \
         Run `ocomment fix` to remove them.\n"
    );
    assert!(!stderr.contains("(s)"), "summary is:\n{stderr}");
}

/// An unreadable path is an I/O error, and the summary must own up to it.
#[cfg(unix)]
#[test]
fn the_summary_counts_io_errors() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "sample.rs", "missing.rs"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("1 I/O error."),
        "the summary hides the I/O error:\n{stderr}"
    );
}

/// `-q` silences the chatter, never the product: a patch is the whole point of
/// `diff`, so it survives.
#[test]
fn quiet_diff_still_writes_the_patch() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["diff", "-q", "sample.rs"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("--- a/sample.rs"), "diff is:\n{stdout}");
    assert!(
        stdout.contains("-let x = 1; // remove"),
        "diff is:\n{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

/// The same rule for `scan`: the listing is the product.
#[test]
fn quiet_scan_still_writes_the_listing() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("a.py"),
        b"#!/usr/bin/env python3\nx = 1  # remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["scan", "-q", "a.py"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 2, "scan output is:\n{stdout}");
    assert!(
        stdout.contains("a.py:2:8: line remove "),
        "scan output is:\n{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

/// Nothing was scanned and the only skip was named on the command line, where
/// it already has its own line: the summary says so without repeating it.
#[test]
fn a_run_of_only_named_skips_does_not_repeat_them() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();
    let output = run(directory.path(), &["check", "notes.md"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains(&format!("notes.md: skipped: {NO_LANGUAGE}"))
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Nothing to check.\n"
    );
}

/// `-` in the PATH list is standard input: it is scanned like any other file
/// and reported under the pseudo path `<stdin>`.
#[test]
fn a_dash_reads_standard_input_as_a_file() {
    let directory = tempfile::tempdir().unwrap();
    let output = run_stdin(
        directory.path(),
        &["check", "--language", "rust", "-"],
        b"let x = 1; // note\n",
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "<stdin>:1:12: removable line comment: // note\n"
    );
}

/// The patch for standard input names the same pseudo path.
#[test]
fn a_dash_diffs_standard_input_under_the_pseudo_path() {
    let directory = tempfile::tempdir().unwrap();
    let output = run_stdin(
        directory.path(),
        &["diff", "--language", "rust", "-"],
        b"let x = 1; // note\n",
    );
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("--- a/<stdin>\n"), "diff is:\n{stdout}");
    assert!(stdout.contains("-let x = 1; // note"), "diff is:\n{stdout}");
}

/// The machine formats carry the pseudo path too, so a piped run is as
/// scriptable as a walked one.
#[test]
fn a_dash_names_standard_input_in_json() {
    let directory = tempfile::tempdir().unwrap();
    let output = run_stdin(
        directory.path(),
        &["check", "--format", "json", "--language", "rust", "-"],
        b"let x = 1; // note\n",
    );
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("\"path\": \"<stdin>\""),
        "json is:\n{stdout}"
    );
}

/// Standard input has no name to detect a language from, so bytes that carry
/// no signature are a usage error with an actionable message.
#[test]
fn undetectable_standard_input_asks_for_a_language() {
    let directory = tempfile::tempdir().unwrap();
    let output = run_stdin(directory.path(), &["check", "-"], b"let x = 1; // note\n");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "ocomment: cannot detect the language of standard input; \
         pass --language <LANGUAGE> (see `ocomment languages`)\n"
    );
}

/// `strip` and every command that accepts `-` read the same standard input,
/// so they must fail with the same words when they cannot tell what it is.
/// One constant is what makes that true; this test is what keeps it true.
#[test]
fn strip_and_check_agree_on_the_undetectable_input_message() {
    let directory = tempfile::tempdir().unwrap();
    let expected = "ocomment: cannot detect the language of standard input; \
                    pass --language <LANGUAGE> (see `ocomment languages`)\n";
    for arguments in [
        vec!["strip"],
        vec!["check", "-"],
        vec!["diff", "-"],
        vec!["scan", "-"],
    ] {
        let output = run_stdin(directory.path(), &arguments, b"let x = 1; // note\n");
        assert_eq!(output.status.code(), Some(2), "`ocomment {arguments:?}`");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            expected,
            "`ocomment {arguments:?}`"
        );
    }
}

/// A pipe cannot be rewritten in place; `fix` says so and names the command
/// that does write a stripped stream.
#[test]
fn fix_refuses_standard_input() {
    let directory = tempfile::tempdir().unwrap();
    let output = run_stdin(
        directory.path(),
        &["fix", "--language", "rust", "-"],
        b"let x = 1; // note\n",
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "ocomment: cannot rewrite standard input in place; use `ocomment strip`\n"
    );
}

/// There is only one standard input, so naming it twice is a usage error
/// rather than a silently deduplicated target.
#[test]
fn standard_input_may_be_named_only_once() {
    let directory = tempfile::tempdir().unwrap();
    let output = run_stdin(
        directory.path(),
        &["check", "--language", "rust", "-", "-"],
        b"let x = 1; // note\n",
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cannot read standard input twice"),
        "the second `-` was accepted"
    );
}

/// `--staged` reads the Git index; a pipe cannot be one of its entries.
#[test]
fn standard_input_conflicts_with_staged() {
    let directory = repository();
    let output = run_stdin(
        directory.path(),
        &["check", "--staged", "--language", "rust", "-"],
        b"let x = 1; // note\n",
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "ocomment: cannot read standard input with --staged; the index is the source\n"
    );
}

/// `fix --dry-run` is `diff` with fix vocabulary: the patch goes to standard
/// output, the file keeps every byte, and the exit code still reports a
/// pending change.
#[test]
fn fix_dry_run_writes_a_patch_and_leaves_the_file_alone() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sample.rs");
    let before = b"let x = 1; // remove\n";
    fs::write(&path, before).unwrap();

    let output = run(directory.path(), &["fix", "--dry-run", "sample.rs"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(&path).unwrap(), before);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with("--- a/sample.rs\n"),
        "diff is:\n{stdout}"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Would remove 1 comment in 1 file. Rerun without --dry-run to apply.\n"
    );
}

/// With nothing to take out, the preview says what a real `fix` would say.
#[test]
fn fix_dry_run_on_a_clean_file_reports_nothing_to_fix() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("clean.rs");
    fs::write(&path, b"let x = 1;\n").unwrap();

    let output = run(directory.path(), &["fix", "--dry-run", "clean.rs"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Nothing to fix in 1 file.\n"
    );
}

/// A skipped path can be the whole answer to the run, so the preview still has
/// to name it — but `fix --dry-run` promises a patch on standard output, and a
/// reader piping that into `git apply` cannot be handed a prose line in the
/// middle of it. The reason goes to standard error instead, directly above the
/// summary that counts it, word for word what the `fix` it stands in for says.
///
/// Spec change: the preview used to print that line on standard output, where
/// it corrupted the patch. Plain `fix` writes no patch and keeps its skips on
/// standard output; plain `diff` folds them into the summary as before.
#[test]
fn fix_dry_run_lists_a_skipped_path_the_way_fix_does() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();

    let skip = format!("notes.md: skipped: {NO_LANGUAGE}\n");
    let previewed = run(directory.path(), &["fix", "--dry-run", "notes.md"]);
    assert_eq!(previewed.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(previewed.stdout).unwrap(),
        "",
        "the preview put prose on the standard output it promises as a patch"
    );
    assert_eq!(
        String::from_utf8(previewed.stderr).unwrap(),
        format!("{skip}Nothing to fix.\n"),
        "the preview never said why it had nothing to fix"
    );

    let fixed = run(directory.path(), &["fix", "notes.md"]);
    assert_eq!(
        String::from_utf8(fixed.stdout).unwrap(),
        skip,
        "`fix` stopped listing the skip on standard output"
    );

    let diffed = run(directory.path(), &["diff", "notes.md"]);
    assert_eq!(diffed.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(diffed.stdout).unwrap(),
        "",
        "`diff` reserves its standard output for the patch"
    );
    assert_eq!(
        String::from_utf8(diffed.stderr).unwrap(),
        "Nothing to diff.\n",
        "plain `diff` folds a skip into its summary rather than listing it"
    );
}

/// Both new entry points are discoverable from `--help`.
#[test]
fn help_documents_standard_input_and_the_dry_run() {
    let directory = tempfile::tempdir().unwrap();
    let checked = run(directory.path(), &["check", "--help"]);
    assert_eq!(checked.status.code(), Some(0));
    let help = String::from_utf8(checked.stdout).unwrap();
    assert!(
        help.contains("`-` reads standard input"),
        "`check --help` does not document `-`:\n{help}"
    );
    let fixed = run(directory.path(), &["fix", "--help"]);
    assert_eq!(fixed.status.code(), Some(0));
    let help = String::from_utf8(fixed.stdout).unwrap();
    assert!(
        help.contains("--dry-run"),
        "`fix --help` does not document --dry-run:\n{help}"
    );
}

/// Standard input is one target among others, not a mode: a piped file and a
/// named one are reported by the same run.
#[test]
fn a_dash_can_be_mixed_with_named_paths() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("sample.rs"), b"let y = 2; // named\n").unwrap();
    let output = run_stdin(
        directory.path(),
        &["check", "--language", "rust", "sample.rs", "-"],
        b"let x = 1; // piped\n",
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "<stdin>:1:12: removable line comment: // piped\n\
         sample.rs:1:12: removable line comment: // named\n"
    );
}

/// The default command takes the same PATH list, so `-` works without naming
/// `check` at all.
#[test]
fn the_default_command_also_reads_a_dash() {
    let directory = tempfile::tempdir().unwrap();
    let output = run_stdin(
        directory.path(),
        &["--language", "rust", "-"],
        b"let x = 1; // note\n",
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "<stdin>:1:12: removable line comment: // note\n"
    );
}

/// `--dry-run` previews the staged run too: the patch is the one `--staged`
/// would apply, and the index keeps every byte.
#[test]
fn fix_dry_run_previews_the_staged_patch_without_writing_the_index() {
    let directory = repository();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    git(directory.path(), &["add", "sample.rs"]);
    let output = run(directory.path(), &["fix", "--dry-run", "--staged"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with("--- a/sample.rs\n"),
        "diff is:\n{stdout}"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Would remove 1 comment in 1 file. Rerun without --dry-run to apply.\n"
    );
    assert_eq!(
        git(directory.path(), &["show", ":sample.rs"]),
        b"let x = 1; // remove\n"
    );
}

/// A tree whose report is far larger than any pipe buffer, so a reader that
/// stops early is guaranteed to close the pipe while the run is still writing.
fn wide_tree(files: usize, comments: usize) -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    let mut source = String::new();
    for index in 0..comments {
        source.push_str(&format!("let value{index} = {index}; // remove {index}\n"));
    }
    for index in 0..files {
        fs::write(directory.path().join(format!("file{index}.rs")), &source).unwrap();
    }
    directory
}

/// Run the binary, take `head` bytes of its output, then close the pipe and
/// report how the run ended and what it said on standard error.
fn run_closed_pipe(directory: &Path, arguments: &[&str], head: usize) -> (ExitStatus, String) {
    let mut child = Command::new(binary())
        .current_dir(directory)
        .env("PATH", "/usr/bin:/bin")
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = child.stdout.take().expect("standard output was piped");
    let mut taken = vec![0u8; head];
    if head > 0 {
        output.read_exact(&mut taken).unwrap();
    }
    /* NOTE: The reader has what it wanted; from here every write the run attempts
     * fails with EPIPE. */
    drop(output);
    let mut message = String::new();
    child
        .stderr
        .take()
        .expect("standard error was piped")
        .read_to_string(&mut message)
        .unwrap();
    (child.wait().unwrap(), message)
}

/// `ocomment check --format json . | head` is a reader that stops early, not a
/// failure: the run ends quietly with status 0 and says nothing.
#[test]
fn a_closed_pipe_ends_the_json_report_quietly() {
    let directory = wide_tree(100, 50);
    let (status, stderr) =
        run_closed_pipe(directory.path(), &["check", "--format", "json", "."], 10);
    assert!(
        status.success(),
        "expected a quiet exit, got {status:?} with stderr:\n{stderr}"
    );
    assert_eq!(stderr, "");
}

/// The human report is written the same way, so it ends the same way.
#[test]
fn a_closed_pipe_ends_the_human_report_quietly() {
    let directory = wide_tree(100, 50);
    let (status, stderr) = run_closed_pipe(directory.path(), &["check", "."], 10);
    assert!(
        status.success(),
        "expected a quiet exit, got {status:?} with stderr:\n{stderr}"
    );
    assert_eq!(stderr, "");
}

/// So are the machine formats that serialize straight into standard output.
#[test]
fn a_closed_pipe_ends_the_sarif_report_quietly() {
    let directory = wide_tree(100, 50);
    let (status, stderr) =
        run_closed_pipe(directory.path(), &["check", "--format", "sarif", "."], 10);
    assert!(
        status.success(),
        "expected a quiet exit, got {status:?} with stderr:\n{stderr}"
    );
    assert_eq!(stderr, "");
}

/// A short report can lose its reader before it writes its first byte. The
/// listing commands must survive that too.
#[test]
fn a_pipe_closed_before_the_first_byte_ends_languages_quietly() {
    let directory = tempfile::tempdir().unwrap();
    let (status, stderr) = run_closed_pipe(directory.path(), &["languages"], 0);
    assert!(
        status.success(),
        "expected a quiet exit, got {status:?} with stderr:\n{stderr}"
    );
    assert_eq!(stderr, "");
}

/// `clap_complete` writes straight into the handle it is handed and panics if
/// that write fails, so the completion script is buffered before it is written.
#[test]
fn a_pipe_closed_before_the_first_byte_ends_completions_quietly() {
    let directory = tempfile::tempdir().unwrap();
    let (status, stderr) = run_closed_pipe(directory.path(), &["completions", "zsh"], 0);
    assert!(
        status.success(),
        "expected a quiet exit, got {status:?} with stderr:\n{stderr}"
    );
    assert_eq!(stderr, "");
}

/// Run the binary with its standard error piped to a reader that closes at
/// once, and report how it ended.
fn run_closed_error_pipe(directory: &Path, arguments: &[&str]) -> ExitStatus {
    let mut child = Command::new(binary())
        .current_dir(directory)
        .env("PATH", "/usr/bin:/bin")
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stderr.take().expect("standard error was piped"));
    child.wait().unwrap()
}

/// Standard error carries commentary, not the product of the run, so losing
/// its reader changes nothing: `-v` still reports its verdict through the exit
/// status instead of dying on the trace it could not write.
#[test]
fn a_closed_error_pipe_does_not_end_the_run() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let status = run_closed_error_pipe(directory.path(), &["check", "-v", "."]);
    assert_eq!(
        status.code(),
        Some(1),
        "a closed standard error changed the verdict: {status:?}"
    );
}

/// The real `git` on this machine, found on `PATH` the way a shell finds it.
///
/// A fake `git` planted ahead of it has to hand every other subcommand to the
/// genuine one by absolute path: the fake is first on `PATH` itself, so `exec
/// git` would only call it back. `/usr/bin/git` is the fallback for a `PATH`
/// that names none.
#[cfg(unix)]
fn real_git() -> std::path::PathBuf {
    std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .map(|directory| directory.join("git"))
                .find(|candidate| candidate.is_file())
        })
        .unwrap_or_else(|| std::path::PathBuf::from("/usr/bin/git"))
}

/// A closed pipe is benign only when it is *our* report that lost its reader.
/// `git hash-object` exiting before it reads the rewritten blob breaks a pipe
/// the run owns in the other direction: the index was never updated, so the
/// run must report the failure instead of ending quietly with success.
#[cfg(unix)]
#[test]
fn a_broken_pipe_from_git_hash_object_fails_the_staged_fix() {
    use std::os::unix::fs::PermissionsExt;

    let directory = repository();
    /* NOTE: What travels down the pipe is the blob with the comments already taken
     * out, so it is that which has to outgrow the pipe buffer — 64 KiB on
     * Linux — for the write to still be in flight when the fake
     * `git hash-object` drops the reading end. Half again as much is margin
     * enough. The file is therefore sized by the bytes that survive the fix
     * rather than by its own length, and it carries them on a few long lines
     * instead of many short ones: the run costs time per comment, and this
     * test needs bytes. */
    let padding = "x".repeat(200);
    let mut source = String::new();
    let mut stripped = 0;
    let mut index = 0;
    while stripped < 96 * 1024 {
        let code = format!("let value{index} = \"{padding}\";");
        stripped += code.len() + 1;
        source.push_str(&format!("{code} // remove {index}\n"));
        index += 1;
    }
    let path = directory.path().join("wide.rs");
    fs::write(&path, &source).unwrap();
    git(directory.path(), &["add", "wide.rs"]);
    let staged_before = git(directory.path(), &["show", ":wide.rs"]);

    /* NOTE: Every invocation reaches the real Git except `hash-object`, which closes
     * its standard input and fails without reading a byte. */
    let fake = tempfile::tempdir().unwrap();
    let script = fake.path().join("git");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\n\
             if [ \"$1\" = hash-object ]; then\n\
             exec 0<&-\n\
             exit 1\n\
             fi\n\
             exec {} \"$@\"\n",
            real_git().display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new(binary())
        .current_dir(directory.path())
        .env("PATH", format!("{}:/usr/bin:/bin", fake.path().display()))
        .args(["fix", "--staged"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "a failed blob write ended the run quietly; stderr was:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "the failed staged fix was never reported on standard error"
    );
    assert!(
        stderr.contains("git hash-object"),
        "the report does not say which write failed:\n{stderr}"
    );
    /* NOTE: `hash-object` also exits non-zero, and that failure carries the same
     * name. This is the test for the broken pipe, so the blob must have been
     * in flight when the reader went away, not sitting whole in the buffer. */
    assert!(
        stderr.contains("cannot write the rewritten blob"),
        "the run failed before the blob was ever written, so the broken pipe \
         went untested:\n{stderr}"
    );
    assert_eq!(
        git(directory.path(), &["show", ":wide.rs"]),
        staged_before,
        "the index changed although no blob was written"
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        source.as_bytes(),
        "the working tree changed although no blob was written"
    );
}

/// `fix` refuses standard input, so its `--help` must not offer it as a target.
#[test]
fn fix_help_does_not_advertise_the_standard_input_it_refuses() {
    let directory = tempfile::tempdir().unwrap();
    let fixed = run(directory.path(), &["fix", "--help"]);
    assert_eq!(fixed.status.code(), Some(0));
    let help = String::from_utf8(fixed.stdout).unwrap();
    assert!(
        !help.contains("reads standard input"),
        "`fix --help` advertises a target it refuses:\n{help}"
    );
    assert!(
        help.contains("Files or directories to rewrite"),
        "`fix --help` does not describe its PATH list:\n{help}"
    );
    let checked = run(directory.path(), &["check", "--help"]);
    assert_eq!(checked.status.code(), Some(0));
    let help = String::from_utf8(checked.stdout).unwrap();
    assert!(
        help.contains("reads standard input"),
        "`check --help` stopped documenting `-`:\n{help}"
    );
}

/// The counter line is erased only if one was ever drawn: a run that scans
/// nothing must not write an escape sequence to a terminal that saw no counter.
#[test]
fn progress_clears_the_counter_only_when_one_was_drawn() {
    let empty = tempfile::tempdir().unwrap();
    let output = run(empty.path(), &["check", "--progress", "always", "."]);
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stderr.contains("\r\x1b[2K"),
        "a counter that was never drawn was cleared anyway:\n{stderr:?}"
    );

    let directory = many_files(120);
    let output = run(directory.path(), &["check", "--progress", "always", "."]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("\r\x1b[2K"),
        "the counter line was never cleared:\n{stderr:?}"
    );
}

/// "Nothing to check" is the vocabulary of `check`. Every command has its own
/// verb for the run that found nothing to work on.
#[test]
fn an_empty_run_summarizes_itself_in_the_vocabulary_of_its_command() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();
    /* NOTE: `fix --dry-run` keeps its standard output for the patch, so the named
     * skip it met stands on standard error directly above the summary this
     * test is about; every other command reports the skip elsewhere. */
    let skip = format!("notes.md: skipped: {NO_LANGUAGE}\n");
    for (arguments, expected) in [
        (vec!["check", "notes.md"], "Nothing to check.\n".to_owned()),
        (vec!["fix", "notes.md"], "Nothing to fix.\n".to_owned()),
        (
            vec!["fix", "--dry-run", "notes.md"],
            format!("{skip}Nothing to fix.\n"),
        ),
        (vec!["diff", "notes.md"], "Nothing to diff.\n".to_owned()),
        (vec!["scan", "notes.md"], "Nothing to scan.\n".to_owned()),
    ] {
        let output = run(directory.path(), &arguments);
        assert_eq!(output.status.code(), Some(0), "`ocomment {arguments:?}`");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            expected,
            "`ocomment {arguments:?}`"
        );
    }
    let output = run(directory.path(), &["scan", "."]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Nothing to scan: 1 file skipped (unknown language: 1; use -v to list).\n"
    );
}

/// A file OComment has no scanner for is not "unknown": the skip line names
/// the list to consult and the flag that forces a language anyway. The folded
/// summary clause keeps the short key, so a walk over a hundred unreadable
/// extensions still reads as one clause instead of a hundred sentences.
#[test]
fn an_unknown_language_skip_says_how_to_force_one() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    fs::write(directory.path().join("notes.md"), b"# notes\n").unwrap();
    let output = run(directory.path(), &["check", "-v", "."]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stdout.contains(
            "notes.md: skipped: no built-in language for this file \
             (see `ocomment languages`; use --language to force)"
        ),
        "the skip line never said what to do about it:\n{stdout}"
    );
    assert!(
        stderr.contains("1 file skipped (unknown language: 1)."),
        "the folded clause stopped using the short key:\n{stderr}"
    );
}

/// A path that was named and is not there says where it was looked for, so a
/// typo, a wrong working directory, and a deleted file are told apart without
/// a second run.
#[test]
fn a_missing_path_says_where_it_was_looked_for() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = fs::canonicalize(directory.path()).unwrap();
    let output = run(&cwd, &["check", "missing.rs"]);
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!(
            "missing.rs: error: path does not exist (checked relative to {})",
            cwd.display()
        )),
        "check output is:\n{stdout}"
    );
}

/// A project configuration without the version key is refused; saying which
/// line to add, and to which file, is the whole fix.
#[test]
fn a_configuration_without_a_version_says_how_to_add_one() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join(".ocomment.toml");
    fs::write(&config, b"[policy]\nmode = \"all\"\n").unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "sample.rs"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains(&format!(
            "must contain `version = 1` (add `version = 1` at the top of {})",
            config.display()
        )),
        "the version error never said what to write:\n{stderr}"
    );
}

/// A misspelled `[languages.*]` key is refused by name; the fix is the list of
/// the languages that do exist.
#[test]
fn an_unknown_language_key_points_at_the_language_list() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n[languages.klingon]\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "sample.rs"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "ocomment: unknown language configuration key `klingon`; see `ocomment languages`\n"
    );
}

/// `--staged` reads the index, so outside a repository the flag is the thing
/// to drop. Git's own words are kept: they say which directory was searched.
#[test]
fn staged_outside_a_repository_names_the_flag_and_quotes_git() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["check", "--staged"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("--staged needs a Git repository:"),
        "the failure never named the flag that needed one:\n{stderr}"
    );
    assert!(
        stderr.contains("not a git repository"),
        "Git's own explanation was dropped:\n{stderr}"
    );
}

/// A lock file left behind by a crashed Git is indistinguishable from a Git
/// that is running right now, so the message offers both readings and the
/// path to delete.
#[test]
fn a_locked_git_index_says_what_to_do_about_the_lock() {
    let directory = repository();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    git(directory.path(), &["add", "sample.rs"]);
    fs::write(directory.path().join(".git/index.lock"), b"").unwrap();
    let output = run(directory.path(), &["fix", "--staged"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains(
            "Git index is locked; no files were modified; another Git process may be \
             running, or remove a stale .git/index.lock"
        ),
        "the lock failure never said what to do about it:\n{stderr}"
    );
}

/// The plugin commands shell out to four tools. A missing one must name the
/// binary, say what this run wanted it for, and point at the command that
/// reports the whole environment at once.
#[test]
fn a_missing_plugin_tool_names_it_its_purpose_and_doctor() {
    let directory = tempfile::tempdir().unwrap();
    /* NOTE: Pin the project root: without a configuration the walk upwards can find
     * a repository marker above the temporary directory and install there. */
    fs::write(directory.path().join(".ocomment.toml"), b"version = 1\n").unwrap();
    let empty = tempfile::tempdir().unwrap();
    for (source, expected) in [
        (
            "https://example.invalid/scanner.wasm",
            "cannot run `curl` (needed for https:// plugin sources); run `ocomment doctor`",
        ),
        (
            "gh:owner/repo@v1#scanner.wasm",
            "cannot run `gh` (needed for gh: plugin sources); run `ocomment doctor`",
        ),
        (
            "oci:example.invalid/scanner:1",
            "cannot run `oras` (needed for oci: plugin sources); run `ocomment doctor`",
        ),
    ] {
        let output = Command::new(binary())
            .current_dir(directory.path())
            .env("PATH", empty.path())
            .args([
                "plugin",
                "add",
                source,
                "--sha256",
                "0000000000000000000000000000000000000000000000000000000000000000",
                "--identity",
                "publisher@example.test",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "`plugin add {source}`");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains(expected),
            "`plugin add {source}` said:\n{stderr}"
        );
    }
}

/// Write an executable stand-in for one external tool, printing `lines` and
/// nothing else. `doctor` reports whatever a tool says about itself, so a fake
/// that says something recognizable is enough to pin the row it produces.
#[cfg(unix)]
fn fake_tool_lines(directory: &Path, name: &str, lines: &[&str]) {
    use std::os::unix::fs::PermissionsExt;
    let path = directory.join(name);
    let arguments: String = lines
        .iter()
        .map(|line| {
            assert!(
                !line.contains(['"', '\\', '$', '`']),
                "the fake tool writes a shell script, so `{line}` needs quoting it does not do"
            );
            format!(" \"{line}\"")
        })
        .collect();
    fs::write(&path, format!("#!/bin/sh\nprintf '%s\\n'{arguments}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// The common case: a tool that answers with one line.
#[cfg(unix)]
fn fake_tool(directory: &Path, name: &str, line: &str) {
    fake_tool_lines(directory, name, &[line]);
}

/// Run the binary with `PATH` pointing at `tools` and nothing else, so a probe
/// sees exactly the tools the test installed there.
#[cfg(unix)]
fn run_with_tools(directory: &Path, tools: &Path, arguments: &[&str]) -> Output {
    Command::new(binary())
        .current_dir(directory)
        .env("PATH", tools)
        .args(arguments)
        .output()
        .unwrap()
}

/// `doctor` is the command every missing-tool failure points at, so it has to
/// probe the tools the plugin commands and `--staged` shell out to instead of
/// assuming them. A tool that is not installed is reported with the very
/// purpose the failure would have named, and is not itself a failure: all five
/// are optional, and a run that never touches a plugin never needs one.
#[cfg(unix)]
#[test]
fn doctor_probes_the_optional_tools_it_shells_out_to() {
    let directory = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    fake_tool(tools.path(), "git", "git version 9.9.9");

    let output = run_with_tools(directory.path(), tools.path(), &["doctor"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "a missing optional tool failed the run: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(
        report.contains("git: git version 9.9.9"),
        "doctor did not report the one tool on PATH:\n{report}"
    );
    for missing in [
        "curl: not found (needed for https:// plugin sources)",
        "gh: not found (needed for gh: plugin sources)",
        "oras: not found (needed for oci: plugin sources)",
        "cosign: not found (needed for --identity verification)",
    ] {
        assert!(
            report.contains(missing),
            "doctor never reported `{missing}`:\n{report}"
        );
    }
}

/// The row carries the tool's own version line, whatever the tool chose to
/// say: `doctor` reports the environment rather than parsing it. `git` is
/// probed for the same reason as the rest — `--staged` is the part of the run
/// that stops working without it.
#[cfg(unix)]
#[test]
fn doctor_reports_a_probed_tools_own_version_line() {
    let directory = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    fake_tool(tools.path(), "cosign", "cosign v9.9.9");

    let output = run_with_tools(directory.path(), tools.path(), &["doctor"]);
    assert_eq!(output.status.code(), Some(0));
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(
        report.contains("cosign: cosign v9.9.9"),
        "doctor did not carry the tool's own version line:\n{report}"
    );
    assert!(
        report.contains("git: not found (needed for --staged)"),
        "a missing `git` never named the flag that needs it:\n{report}"
    );
}

/// `cosign version` draws several lines of ASCII art before it says anything
/// about itself, and a row carrying the top of that banner would tell a reader
/// nothing at all. A version has a number in it, so that is the line the row
/// carries — sanitised like every probed line, so the run of spaces the tool
/// aligned its banner with is collapsed to one.
#[cfg(unix)]
#[test]
fn doctor_looks_past_a_banner_for_the_version_line() {
    let directory = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    fake_tool_lines(
        tools.path(),
        "cosign",
        &[
            "  ______   ______",
            " |      | |  __  |",
            "cosign: A tool for Container Signing",
            "",
            "GitVersion:    v9.9.9",
        ],
    );

    let output = run_with_tools(directory.path(), tools.path(), &["doctor"]);
    assert_eq!(output.status.code(), Some(0));
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(
        report.contains("cosign: GitVersion: v9.9.9"),
        "doctor reported the banner instead of the version behind it:\n{report}"
    );
    assert!(
        !report.contains("GitVersion:    v9.9.9"),
        "the tool's own alignment survived into the row:\n{report}"
    );
    assert!(
        !report.contains("cosign:   ______"),
        "the top of the banner reached the report:\n{report}"
    );
}

/// A probed tool chooses the bytes `doctor` prints, so a version line is
/// untrusted input on its way to a terminal: a tool planted on `PATH` could
/// clear the screen or repaint the report from its own banner. The row carries
/// what the tool said with every control sequence replaced, and a tool that
/// answers at all is still a healthy row rather than a failing run.
#[cfg(unix)]
#[test]
fn doctor_strips_control_sequences_from_a_probed_tools_version_line() {
    let directory = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    fake_tool(
        tools.path(),
        "cosign",
        "\u{1b}[2J\u{1b}[1;31mv1.0 PWNED\u{1b}[0m",
    );

    let output = run_with_tools(directory.path(), tools.path(), &["doctor"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "a tool that answered with control sequences failed the run: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.contains(&0x1b),
        "an escape byte reached the report: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(
        report.contains("cosign: \u{fffd}[2J\u{fffd}[1;31mv1.0 PWNED\u{fffd}[0m\n"),
        "doctor did not report the version line with its controls replaced:\n{report}"
    );
}

/// The other half of "why did that run do that?" is the environment the run
/// resolved for itself: where it stood, what it took as the root, which
/// configuration files it merged, and whether its output is decorated.
#[test]
fn doctor_reports_the_environment_it_resolved() {
    let directory = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    let doctor = |no_color: Option<&str>| {
        let mut command = Command::new(binary());
        command
            .current_dir(directory.path())
            .env("PATH", "/usr/bin:/bin")
            /* NOTE: Pin the user layer away from whoever is running the tests: the
             * trace this reports has to be the one this run resolved. */
            .env("XDG_CONFIG_HOME", empty.path())
            .arg("doctor");
        match no_color {
            Some(value) => command.env("NO_COLOR", value),
            None => command.env_remove("NO_COLOR"),
        };
        let output = command.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };

    let report = doctor(None);
    let cwd = fs::canonicalize(directory.path()).unwrap();
    assert!(
        report.contains(&format!("cwd: {}", cwd.display())),
        "doctor never said where it was standing:\n{report}"
    );
    assert!(
        report.contains("root: "),
        "doctor never said what it took as the root:\n{report}"
    );
    assert!(
        report.contains("config: built-in defaults"),
        "doctor never traced the configuration it merged:\n{report}"
    );
    /* NOTE: The report is read through a pipe, so the decoration it describes is the
     * decoration this very run chose. */
    assert!(
        report.contains("stdout: not a terminal"),
        "doctor never said whether its output is a terminal:\n{report}"
    );
    assert!(
        report.contains("NO_COLOR: unset"),
        "doctor never said whether NO_COLOR is set:\n{report}"
    );
    assert!(
        doctor(Some("1")).contains("NO_COLOR: set"),
        "doctor ignored the NO_COLOR that silences its colour"
    );

    let created = run(directory.path(), &["init", "config"]);
    assert_eq!(created.status.code(), Some(0));
    let report = doctor(None);
    assert!(
        report.contains(&format!(
            "config: project {}",
            cwd.join(".ocomment.toml").display()
        )),
        "doctor did not name the project configuration it found:\n{report}"
    );
    assert!(
        report.contains(&format!("root: {}", cwd.display())),
        "the project file did not move the root with it:\n{report}"
    );
}

/// A directory name is chosen by whoever made the directory, not by OComment,
/// so the two rows that print one are untrusted input on their way to a
/// terminal exactly like a probed tool's version line. They are sanitised the
/// same way and cut nowhere: a path is the answer the reader came for, and one
/// ending in an ellipsis names no directory at all.
#[cfg(unix)]
#[test]
fn doctor_sanitises_the_directories_it_reports_without_cutting_them_short() {
    /* NOTE: Long enough that a preview-width cap would have to cut it, and carrying
     * the escape that would let a directory name repaint the report. */
    let name = format!("ocomment\u{1b}{}", "a".repeat(90));
    let directory = tempfile::Builder::new()
        .prefix(&name)
        .tempdir()
        .expect("a directory name may carry an escape on this platform");
    /* NOTE: A project file of its own makes this directory the root as well, so both
     * rows name it and both are pinned by one run. */
    fs::write(directory.path().join(".ocomment.toml"), b"version = 1\n").unwrap();
    let empty = tempfile::tempdir().unwrap();
    let output = Command::new(binary())
        .current_dir(directory.path())
        .env("PATH", "/usr/bin:/bin")
        .env("XDG_CONFIG_HOME", empty.path())
        .arg("doctor")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.contains(&0x1b),
        "an escape byte reached the report: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    let sanitised = name.replace('\u{1b}', "\u{fffd}");
    for row in ["cwd", "root"] {
        let prefix = format!("{row}: ");
        let named = report
            .lines()
            .find(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("doctor printed no `{row}` row:\n{report}"));
        assert!(
            named.contains(&sanitised),
            "the `{row}` row lost the directory it names:\n{report}"
        );
        // NOTE: A version line may be cut to the preview width; a path may not.
        assert!(
            !named.contains('\u{2026}'),
            "the `{row}` row was cut to the preview width:\n{report}"
        );
    }
}

/// The scaffold refuses to write into a directory that already exists, and a
/// refusal that only says "refusing" leaves the reader to guess. There are two
/// ways out — take the directory away, or take the plugin that owns it away —
/// and the message names both.
#[test]
fn plugin_new_refuses_an_existing_directory_and_says_what_to_do() {
    let directory = tempfile::tempdir().unwrap();
    let taken = directory.path().join("scanner");
    fs::create_dir(&taken).unwrap();

    let output = run(directory.path(), &["plugin", "new", "scanner"]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("refusing to overwrite plugin directory"),
        "{error}"
    );
    assert!(
        error.contains("remove it or run `ocomment plugin remove <name>` first"),
        "the refusal never said what to do about it:\n{error}"
    );
    assert_eq!(
        fs::read_dir(&taken).unwrap().count(),
        0,
        "the refusal wrote into the directory it refused"
    );
}

/// `--policy all` means "take everything out", so the one thing it deliberately
/// leaves behind has to explain itself: the summary counts the kept preambles
/// and names the flag that removes them too.
#[test]
fn policy_all_says_how_to_remove_a_kept_preamble() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("a.py"),
        b"#!/usr/bin/env python3\n# note\nx = 1\n",
    )
    .unwrap();
    let hint = "1 protected preamble comment kept; add --force-protected to remove it.";

    let all = run(directory.path(), &["check", "--policy", "all", "a.py"]);
    assert_eq!(all.status.code(), Some(1));
    let stderr = String::from_utf8(all.stderr).unwrap();
    assert!(
        stderr.contains(hint),
        "`--policy all` never explained the comment it kept:\n{stderr}"
    );

    // NOTE: Nothing is protected any more, so there is nothing to explain.
    let forced = run(
        directory.path(),
        &["check", "--policy", "all", "--force-protected", "a.py"],
    );
    let stderr = String::from_utf8(forced.stderr).unwrap();
    assert!(
        !stderr.contains("--force-protected"),
        "the hint outlived the flag that answers it:\n{stderr}"
    );

    /* NOTE: Under `safe` the preamble is one of many deliberate keeps; singling it
     * out would be noise on every run. */
    let safe = run(directory.path(), &["check", "a.py"]);
    let stderr = String::from_utf8(safe.stderr).unwrap();
    assert!(
        !stderr.contains("--force-protected"),
        "a policy that keeps much more than preambles advertised the flag:\n{stderr}"
    );

    // NOTE: A file with no preamble at all never mentions it.
    fs::write(directory.path().join("b.py"), b"# note\nx = 1\n").unwrap();
    let plain = run(directory.path(), &["check", "--policy", "all", "b.py"]);
    let stderr = String::from_utf8(plain.stderr).unwrap();
    assert!(
        !stderr.contains("--force-protected"),
        "a run that kept no preamble advertised the flag anyway:\n{stderr}"
    );

    /* NOTE: The hint counts what it kept, so its pronoun has to agree with the
     * count: one preamble is removed with "it", several with "them". */
    fs::write(
        directory.path().join("c.py"),
        b"#!/usr/bin/env python3\nx = 2\n",
    )
    .unwrap();
    let both = run(
        directory.path(),
        &["check", "--policy", "all", "a.py", "c.py"],
    );
    let stderr = String::from_utf8(both.stderr).unwrap();
    assert!(
        stderr
            .contains("2 protected preamble comments kept; add --force-protected to remove them."),
        "the plural hint does not agree with the two preambles it counted:\n{stderr}"
    );
}

/// A file that ships with the repository, resolved from the crate directory so
/// a test can read it from whatever temporary directory it runs in.
fn shipped(relative: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// How `clap_mangen` writes one option name: every `-` escaped, the name in
/// bold. Help text that merely mentions a flag is rendered in roman, so this
/// matches a real entry rather than a passing reference in someone else's
/// description.
fn roff_option(flag: &str) -> String {
    format!("\\fB{}\\fR", flag.replace('-', "\\-"))
}

/// Every long flag the CLI shows a user, gathered by walking `--help` down
/// every subcommand. Descriptions are scanned too: a flag a description names
/// is a flag the reader will look up.
fn long_flags_in_help(directory: &Path, path: &[&str], found: &mut BTreeSet<String>) {
    let mut arguments = path.to_vec();
    arguments.push("--help");
    let output = run(directory, &arguments);
    assert_eq!(
        output.status.code(),
        Some(0),
        "`ocomment {}` did not print help",
        arguments.join(" ")
    );
    let help = String::from_utf8(output.stdout).unwrap();
    let mut rest = help.as_str();
    while let Some(start) = rest.find("--") {
        let tail = &rest[start + 2..];
        let end = tail
            .find(|character: char| {
                !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
            })
            .unwrap_or(tail.len());
        let name = tail[..end].trim_end_matches('-');
        if name.starts_with(|character: char| character.is_ascii_lowercase()) {
            found.insert(format!("--{name}"));
        }
        rest = tail;
    }
    let children = subcommand_lines(&help);
    for (name, _) in &children {
        if name == "help" {
            continue;
        }
        let mut child = path.to_vec();
        child.push(name.as_str());
        long_flags_in_help(directory, &child, found);
    }
}

/// A flag nobody can look up is a flag nobody knows about. The manual page is
/// the reference the `man` subcommand and the release archives both hand out,
/// so every flag `--help` mentions anywhere in the command tree has to have an
/// entry there.
#[test]
fn the_manual_page_documents_every_long_flag() {
    let directory = tempfile::tempdir().unwrap();
    let mut flags = BTreeSet::new();
    long_flags_in_help(directory.path(), &[], &mut flags);
    assert!(
        flags.len() >= 20,
        "the help walk stopped finding flags, so this test proves nothing: {flags:?}"
    );
    /* NOTE: A walk that stopped at the root would still collect enough flags to look
     * healthy, so it is pinned to one flag from each depth it has to reach. */
    for reached in ["--dry-run", "--sha256"] {
        assert!(
            flags.contains(reached),
            "the help walk never reached `{reached}`, so it is not descending: {flags:?}"
        );
    }
    let page = fs::read_to_string(shipped("docs/ocomment.1")).unwrap();
    let missing: Vec<&String> = flags
        .iter()
        .filter(|flag| !page.contains(&roff_option(flag)))
        .collect();
    assert!(
        missing.is_empty(),
        "docs/ocomment.1 has no entry for {missing:?}; regenerate it with `ocomment man`"
    );
}

/// The manual page is generated from the parser, and the generated bytes are
/// checked in twice: once for `man -l docs/ocomment.1` and once for the release
/// archives. A page that drifted from the binary documents a tool nobody ships.
#[test]
fn the_checked_in_manual_page_is_the_one_the_binary_renders() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["man"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for path in ["docs/ocomment.1", "release-extras/ocomment.1"] {
        let checked_in = fs::read(shipped(path)).unwrap();
        assert!(
            checked_in == output.stdout,
            "{path} is stale; regenerate it with \
             `python3 tools/release_extras.py --binary rust/target/debug/ocomment` \
             and copy release-extras/ocomment.1 to docs/"
        );
    }
}

/// The completion scripts ship from the same generator and go stale the same
/// way, so they are pinned to the binary too.
#[test]
fn the_checked_in_completions_are_the_ones_the_binary_generates() {
    let directory = tempfile::tempdir().unwrap();
    for (shell, path) in [
        ("bash", "release-extras/ocomment.bash"),
        ("zsh", "release-extras/_ocomment"),
        ("fish", "release-extras/ocomment.fish"),
        ("powershell", "release-extras/_ocomment.ps1"),
        ("elvish", "release-extras/ocomment.elv"),
    ] {
        let output = run(directory.path(), &["completions", shell]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let checked_in = fs::read(shipped(path)).unwrap();
        assert!(
            checked_in == output.stdout,
            "{path} is stale; regenerate it with \
             `python3 tools/release_extras.py --binary rust/target/debug/ocomment`"
        );
    }
}

/// `--explain` answers "why was this comment kept?": it lists every comment,
/// kept ones included, and names both the rule that decided each one and the
/// table that rule was written in. A plain `check` still reports only what it
/// would remove.
#[test]
fn check_explain_names_the_override_and_the_pattern_that_kept_a_comment() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[policy]\nkeep_regex = [\"(?i)^// api\"]\n\n\
          [[overrides]]\npaths = [\"gen/**\"]\nkeep_regex = [\"(?i)generated\"]\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join("gen")).unwrap();
    fs::write(
        directory.path().join("gen/b.rs"),
        b"/* generated */\nlet x = 1; // TODO\n",
    )
    .unwrap();
    fs::write(directory.path().join("a.rs"), b"// API stays\n").unwrap();

    let plain = run(directory.path(), &["check"]);
    assert_eq!(plain.status.code(), Some(1));
    let plain = String::from_utf8(plain.stdout).unwrap();
    assert!(
        !plain.contains("kept"),
        "a plain `check` listed a kept comment:\n{plain}"
    );

    let output = run(directory.path(), &["check", "--explain"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    for needle in [
        "gen/b.rs:1:1: kept block comment: /* generated */",
        "kept: matched keep_regex #1 `(?i)generated` ([[overrides]] #0, paths = [\"gen/**\"])",
        "gen/b.rs:2:12: removable line comment: // TODO",
        /* NOTE: Nothing set `[policy] mode`, so the reader is told it is a default
         * rather than sent to a file that never mentions it. The pattern the
         * same file does set is named with the file, spelled the way the reader
         * typed their way into the directory. */
        "removed: policy `safe` removes ordinary comments (built-in defaults)",
        "a.rs:1:1: kept line comment: // API stays",
        "kept: matched keep_regex #0 `(?i)^// api` ([policy] in .ocomment.toml)",
    ] {
        assert!(
            report.contains(needle),
            "`check --explain` lacks {needle:?}:\n{report}"
        );
    }
}

/// A comment no setting decided is explained by the flag that would change its
/// fate, because there is no table to send the reader to.
#[test]
fn check_explain_says_what_would_remove_a_preamble_or_a_directive() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("script.py"),
        b"#!/usr/bin/env python3\n# note\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("app.js"),
        b"// eslint-disable-next-line\nlet x = 1;\n",
    )
    .unwrap();

    let output = run(directory.path(), &["check", "--explain"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    for needle in [
        "script.py:1:1: kept shebang comment: #!/usr/bin/env python3",
        "required source preamble",
        "add --force-protected to remove it",
        "app.js:1:1: kept directive comment: // eslint-disable-next-line",
        "kept: tool or language directive `eslint`; use --remove-kind directive \
         or --policy all to remove it",
    ] {
        assert!(
            report.contains(needle),
            "`check --explain` lacks {needle:?}:\n{report}"
        );
    }
}

/// The one keep no setting decided and no flag overrules: a YAML block scalar
/// ends at the comment above the directive it would otherwise swallow. The
/// explanation names the block scalar and the line that has to go first, and
/// the run reports nothing removable at all.
#[test]
fn check_explain_names_the_block_scalar_a_kept_comment_separates() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("values.yaml"),
        b"k: |\n  a\n# ends the block\n  # yamllint disable\nz: 1\n",
    )
    .unwrap();

    let output = run(directory.path(), &["check", "--explain"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    for needle in [
        "values.yaml:3:1: kept line comment: # ends the block",
        "kept: it separates a `yaml` block scalar from the kept comment below \
         it; the comment under it has to go first",
    ] {
        assert!(
            report.contains(needle),
            "`check --explain` lacks {needle:?}:\n{report}"
        );
    }
    /* NOTE: `all` takes the directive out, and with nothing left standing under
     * the body the comment above it is ordinary again. */
    let widened = run(directory.path(), &["check", "--explain", "--policy", "all"]);
    let widened = String::from_utf8(widened.stdout).unwrap();
    assert!(
        widened.contains("values.yaml:3:1: removable line comment"),
        "`--policy all` still kept it:\n{widened}"
    );
}

/// A setting the command line supplied is named as the command line, not as
/// the file it would otherwise have been written in.
#[test]
fn explain_names_the_command_line_when_a_flag_set_the_policy() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("notice.rs"),
        b"// Copyright 2026 Example\nlet x = 1; // TODO\n",
    )
    .unwrap();

    let output = run(
        directory.path(),
        &["check", "--explain", "--policy", "legal"],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    for needle in [
        "notice.rs:1:1: kept license comment: // Copyright 2026 Example",
        "kept: policy legal protects license comments, and this one says `copyright` \
         (--policy on the command line)",
        "removed: policy `legal` removes ordinary comments (--policy on the command line)",
    ] {
        assert!(
            report.contains(needle),
            "`check --explain --policy legal` lacks {needle:?}:\n{report}"
        );
    }
}

/// The machine formats are schemas, not prose, and none of them has a place to
/// put an explanation. Asking for one is a usage error rather than a flag that
/// quietly does nothing.
#[test]
fn explain_is_refused_by_every_machine_format() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("a.rs"), b"let x = 1; // TODO\n").unwrap();
    for format in ["json", "jsonl", "sarif", "github"] {
        let output = run(
            directory.path(),
            &["check", "--explain", "--format", format],
        );
        assert_eq!(
            output.status.code(),
            Some(2),
            "`--format {format} --explain` was accepted"
        );
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("--explain is only available with --format human"),
            "`--format {format} --explain` said:\n{error}"
        );
        assert!(
            output.stdout.is_empty(),
            "`--format {format} --explain` wrote a report anyway"
        );
    }
}

/// `--explain` annotates a report of comments, and only `check` and `scan`
/// write one: `fix` reports the files it rewrote, `diff` writes a patch,
/// `strip` writes the stripped source, and the rest of the commands are not
/// about comments at all. The flag is global, so asking for it anywhere else
/// is a usage error rather than a flag that quietly does nothing.
#[test]
fn explain_is_refused_by_the_commands_that_write_no_report() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("a.rs"), b"let x = 1; // TODO\n").unwrap();
    let refused: [&[&str]; 12] = [
        &["fix", "--explain", "--dry-run"],
        &["fix", "--explain"],
        &["diff", "--explain"],
        &["strip", "--explain", "--language", "rust"],
        &["lsp", "--explain"],
        &["init", "--explain"],
        &["config", "--explain"],
        &["languages", "--explain"],
        &["plugin", "--explain", "list"],
        &["completions", "--explain", "bash"],
        &["doctor", "--explain"],
        &["man", "--explain"],
    ];
    for arguments in refused {
        let output = run_stdin(directory.path(), arguments, b"let x = 1; // TODO\n");
        assert_eq!(
            output.status.code(),
            Some(2),
            "`ocomment {}` was accepted",
            arguments.join(" ")
        );
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("--explain is only available with `check` and `scan`"),
            "`ocomment {}` said:\n{error}",
            arguments.join(" ")
        );
        assert!(
            output.stdout.is_empty(),
            "`ocomment {}` wrote a report anyway",
            arguments.join(" ")
        );
    }
    assert_eq!(
        fs::read(directory.path().join("a.rs")).unwrap(),
        b"let x = 1; // TODO\n",
        "a refused run rewrote the file anyway"
    );
    assert!(
        !directory.path().join(".ocomment.toml").exists(),
        "a refused `init` wrote its starter file anyway"
    );
    // NOTE: The two commands the flag is for still take it.
    for command in ["check", "scan"] {
        let output = run(directory.path(), &[command, "--explain"]);
        assert!(
            output.status.code() != Some(2),
            "`ocomment {command} --explain` was refused:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("a.rs"),
            "`ocomment {command} --explain` wrote no report"
        );
    }
}

/// `scan` already lists every comment; `--explain` puts the reason under each
/// of its lines without disturbing the listing itself.
#[test]
fn scan_explain_annotates_every_listed_comment() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("a.py"),
        b"#!/usr/bin/env python3\nx = 1  # remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["scan", "a.py", "--explain"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("a.py:1:1: shebang keep (required source preamble) 0..22: #!/usr/bin/env python3"),
        "`scan --explain` changed the listing:\n{stdout}"
    );
    assert!(
        lines
            .get(1)
            .is_some_and(|line| line.starts_with("    kept: required source preamble")),
        "`scan --explain` did not explain the shebang:\n{stdout}"
    );
    assert_eq!(
        lines.get(2).copied(),
        Some("a.py:2:8: line remove 30..38: # remove"),
        "`scan --explain` changed the listing:\n{stdout}"
    );
    assert!(
        lines.get(3).is_some_and(
            |line| line.starts_with("    removed: policy `safe` removes ordinary comments")
        ),
        "`scan --explain` did not explain the removal:\n{stdout}"
    );
    assert_no_debug_leak("human scan --explain output", &stdout);
}

/// A staged run reads index blobs through a path that carries no policy trace,
/// so it says so rather than printing a listing with every explanation missing.
#[test]
fn explain_is_refused_by_a_staged_run() {
    let directory = repository();
    fs::write(directory.path().join("a.rs"), b"let x = 1; // TODO\n").unwrap();
    git(directory.path(), &["add", "a.rs"]);
    let output = run(directory.path(), &["check", "--staged", "--explain"]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("--explain is not available with --staged"),
        "`check --staged --explain` said:\n{error}"
    );
}

/// `fix -i` asks a question per comment, so it needs somebody there to answer
/// it. A piped or redirected run would otherwise read the prompt's answer out
/// of whatever the pipe carried — a script's own data — and start writing
/// files from it. The refusal names both ways out and touches nothing.
#[test]
fn fix_interactive_without_a_terminal_refuses_and_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sample.rs");
    let before = b"let x = 1; // remove\n";
    fs::write(&path, before).unwrap();

    let output = run_stdin(directory.path(), &["fix", "-i", "sample.rs"], b"y\n");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "ocomment: --interactive needs a terminal; run without -i or use `ocomment diff`\n"
    );
}

/// The long spelling refuses the same way, so a script that uses it is not
/// told something different from one that uses `-i`.
#[test]
fn fix_interactive_long_spelling_refuses_without_a_terminal() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(directory.path(), &["fix", "--interactive", "sample.rs"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "ocomment: --interactive needs a terminal; run without -i or use `ocomment diff`\n"
    );
}

/// A machine format has no prompt to put a question on and no place to put the
/// answer, so the combination is refused rather than quietly ignoring one of
/// the two flags. It is refused before the terminal is looked at, because the
/// flag combination is wrong however the run was started.
#[test]
fn fix_interactive_refuses_a_machine_format() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sample.rs");
    let before = b"let x = 1; // remove\n";
    fs::write(&path, before).unwrap();

    let output = run(
        directory.path(),
        &["fix", "-i", "--format", "json", "sample.rs"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "ocomment: --interactive is only available with --format human\n"
    );
}

/// Each of these describes a run that cannot also be interactive: the index
/// carries no working-tree file to show a hunk from, `--dry-run` writes
/// nothing whatever the answers were, and `-q` asks for a run with no
/// commentary at all. Clap refuses the pair at parse time, before any file is
/// read.
#[test]
fn fix_interactive_conflicts_with_the_flags_that_contradict_it() {
    let directory = repository();
    let path = directory.path().join("sample.rs");
    let before = b"let x = 1; // remove\n";
    fs::write(&path, before).unwrap();
    git(directory.path(), &["add", "sample.rs"]);

    for conflicting in ["--staged", "--dry-run", "--quiet", "-q"] {
        let output = run(directory.path(), &["fix", "-i", conflicting]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "`fix -i {conflicting}` was accepted"
        );
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("cannot be used with"),
            "`fix -i {conflicting}` did not report a conflict:\n{error}"
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            before,
            "`fix -i {conflicting}` reached the file"
        );
    }
}

/// The flag is discoverable where the reader looks for it.
#[test]
fn help_documents_the_interactive_fix() {
    let directory = tempfile::tempdir().unwrap();
    let fixed = run(directory.path(), &["fix", "--help"]);
    assert_eq!(fixed.status.code(), Some(0));
    let help = String::from_utf8(fixed.stdout).unwrap();
    assert!(
        help.contains("-i, --interactive"),
        "`fix --help` does not document --interactive:\n{help}"
    );
}

/// The listing is a table of languages, not a report of comments, so the
/// formats that describe a report have nowhere to put it. Each is refused with
/// the pair that does work rather than answered with the human table, which is
/// what `--format json` used to be given.
#[test]
fn languages_refuses_the_formats_that_carry_no_table() {
    let directory = tempfile::tempdir().unwrap();
    for format in ["jsonl", "sarif", "github"] {
        let output = run(directory.path(), &["languages", "--format", format]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "`languages --format {format}` was accepted"
        );
        assert!(
            output.stdout.is_empty(),
            "`languages --format {format}` wrote a listing anyway"
        );
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("only available with --format human or --format json"),
            "`languages --format {format}` said:\n{error}"
        );
    }
    for format in ["human", "json"] {
        let output = run(directory.path(), &["languages", "--format", format]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "`languages --format {format}` was refused: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let listing = String::from_utf8(output.stdout).unwrap();
        for language in ["rust", "objective-cpp", "xhtml", "kotlin", "toml"] {
            assert!(
                listing.contains(language),
                "`languages --format {format}` omits `{language}`:\n{listing}"
            );
        }
    }
}

/// A Scala file keeps its scala-cli directive and hides a `//` inside an XML
/// literal's text, while comments inside an interpolation and a nested block
/// comment are removed.
#[test]
fn a_scala_file_keeps_its_directive_and_hides_xml_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Main.scala");
    fs::write(
        &path,
        b"//> using scala \"3.3.0\"\nval a = <a>// text</a>\nval b = s\"${1 /* keep */}\" // remove\n/* outer /* inner */ */\n",
    )
    .unwrap();

    let scanned = run(
        directory.path(),
        &["scan", "Main.scala", "--format", "json"],
    );
    assert_eq!(
        scanned.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&scanned.stdout).unwrap();
    let report = &document["files"][0]["report"];
    assert_eq!(report["language"], "scala");
    assert_eq!(report["comments"].as_array().unwrap().len(), 4);
    assert_eq!(report["comments"][0]["kind"], "directive");
    assert_eq!(report["comments"][0]["disposition"]["action"], "keep");
    assert_eq!(report["comments"][0]["span"]["start"], 0);
    assert_eq!(report["comments"][0]["span"]["end"], 23);
    assert_eq!(report["comments"][1]["span"]["start"], 61);
    assert_eq!(report["comments"][1]["span"]["end"], 71);
    assert_eq!(report["comments"][2]["span"]["start"], 74);
    assert_eq!(report["comments"][2]["span"]["end"], 83);
    assert_eq!(report["comments"][3]["span"]["start"], 84);
    assert_eq!(report["comments"][3]["span"]["end"], 107);

    let fixed = run(directory.path(), &["fix", "Main.scala"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"//> using scala \"3.3.0\"\nval a = <a>// text</a>\nval b = s\"${1 }\" \n\n"
    );
}
