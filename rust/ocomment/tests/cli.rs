use std::{
    fs,
    path::Path,
    process::{Command, Output},
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

#[test]
fn no_argument_scan_uses_repository_root_from_a_subdirectory() {
    let directory = repository();
    fs::write(directory.path().join("root.rs"), b"// root comment\n").unwrap();
    let nested = directory.path().join("nested/deeper");
    fs::create_dir_all(&nested).unwrap();
    let output = run(&nested, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("root.rs"));
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
    // roff requires the `\*(Aq` string definition before the title macro, so
    // `.TH` is the first macro that is not a string definition.
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
    // Only the policy line is pinned: the surrounding lines print filesystem
    // paths that may legitimately contain any spelling.
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
        "Found 1 removable comment in 1 file(s) (1 files scanned). \
         Run `ocomment fix` to remove them.\n"
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
        "No removable comments in 1 file(s).\n"
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
        "Found 1 removable comment in 1 file(s) (1 files scanned). \
         Run `ocomment fix` to apply.\n"
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
        "Removed 1 comment in 1 file(s) (1 files scanned).\n"
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
        "Nothing to fix in 1 file(s).\n"
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
        "Scanned 1 file(s): 2 comments (1 removable, 1 kept).\n"
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
        stderr.contains("1 file(s) skipped (unknown language: 1; use -v to list)."),
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
        stdout.contains("notes.md: skipped: unknown language"),
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
        stdout.contains("notes.md: skipped: unknown language"),
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
            &["check", "sample.rs", "--format", format],
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
        stderr.contains("1 file(s) have invalid syntax; nothing was written for them (use --force-invalid to apply known-safe edits)."),
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
        "Found 1 removable comment in 1 file(s) (1 files scanned). \
         Run `ocomment fix` to remove them.\n"
    );
}

#[test]
fn help_lists_the_verbosity_flags_and_retires_the_progress_indicator() {
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
        !help.contains("progress indicator"),
        "`--progress` still promises an indicator that is never drawn:\n{help}"
    );
}

/// `--progress` stays accepted, but the summary replaced the line it drew.
#[test]
fn the_progress_flag_no_longer_prints_a_progress_line() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("sample.rs"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    let output = run(
        directory.path(),
        &["check", "--progress", "always", "sample.rs"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Found 1 removable comment in 1 file(s) (1 files scanned). \
         Run `ocomment fix` to remove them.\n"
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
