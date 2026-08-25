use std::{
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
        !help.contains("progress indicator"),
        "`--progress` still promises an indicator that is never drawn:\n{help}"
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
        stdout.contains("notes.md: skipped: unknown language"),
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
            .contains("notes.md: skipped: unknown language")
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

/// `strip` and `check -` read the same standard input, so they must fail with
/// the same words when they cannot tell what it is.
#[test]
fn strip_and_check_agree_on_the_undetectable_input_message() {
    let directory = tempfile::tempdir().unwrap();
    let stripped = run_stdin(directory.path(), &["strip"], b"let x = 1; // note\n");
    let checked = run_stdin(directory.path(), &["check", "-"], b"let x = 1; // note\n");
    assert_eq!(stripped.status.code(), Some(2));
    assert_eq!(checked.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(stripped.stderr).unwrap(),
        String::from_utf8(checked.stderr).unwrap()
    );
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
    // The reader has what it wanted; from here every write the run attempts
    // fails with EPIPE.
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

/// A closed pipe is benign only when it is *our* report that lost its reader.
/// `git hash-object` exiting before it reads the rewritten blob breaks a pipe
/// the run owns in the other direction: the index was never updated, so the
/// run must report the failure instead of ending quietly with success.
#[cfg(unix)]
#[test]
fn a_broken_pipe_from_git_hash_object_fails_the_staged_fix() {
    use std::os::unix::fs::PermissionsExt;

    let directory = repository();
    // The blob has to outgrow any pipe buffer, so the write is still in flight
    // when the fake `git hash-object` drops the reading end.
    let mut source = String::new();
    for index in 0..8000 {
        source.push_str(&format!("let value{index} = {index}; // remove {index}\n"));
    }
    let path = directory.path().join("wide.rs");
    fs::write(&path, &source).unwrap();
    git(directory.path(), &["add", "wide.rs"]);
    let staged_before = git(directory.path(), &["show", ":wide.rs"]);

    // Every invocation reaches the real Git except `hash-object`, which closes
    // its standard input and fails without reading a byte.
    let fake = tempfile::tempdir().unwrap();
    let script = fake.path().join("git");
    fs::write(
        &script,
        "#!/bin/sh\n\
         if [ \"$1\" = hash-object ]; then\n\
         exec 0<&-\n\
         exit 1\n\
         fi\n\
         exec /usr/bin/git \"$@\"\n",
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
    for (arguments, expected) in [
        (vec!["check", "notes.md"], "Nothing to check.\n"),
        (vec!["fix", "notes.md"], "Nothing to fix.\n"),
        (vec!["fix", "--dry-run", "notes.md"], "Nothing to fix.\n"),
        (vec!["diff", "notes.md"], "Nothing to diff.\n"),
        (vec!["scan", "notes.md"], "Nothing to scan.\n"),
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
