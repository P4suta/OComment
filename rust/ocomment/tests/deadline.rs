//! Deadlines on the tags that are promises.
//!
//! `[policy.allow] tags` and `[policy.allow.expiry]` differ in one thing and it is the thing that needs a repository: whether the tag runs out.
//! These tests build one, commit at a date of their choosing, and check that the run reaches the verdict the dates call for — and, just as importantly, that it leaves a comment alone when it cannot read them.

mod common;

use std::{fs, path::Path, process::Output};
use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ocomment")
}

/// A scratch directory with no `ocomment/config.toml` in it.
///
/// `ocomment` reads `$XDG_CONFIG_HOME/ocomment/config.toml`, which is a real setting on a real machine and is meant to reach every run.
/// A suite that let it through is a suite whose answers depend on whose machine it ran on.
fn no_user_config() -> &'static std::path::Path {
    static EMPTY: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    EMPTY
        .get_or_init(|| tempfile::tempdir().expect("a temporary directory"))
        .path()
}

fn git(directory: &Path, arguments: &[&str], date: Option<&str>) {
    let mut command = common::isolated("git");
    command.current_dir(directory).args(arguments);
    if let Some(date) = date {
        command
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date);
    }
    let output = command.output().expect("git runs");
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Run the binary, naming `--format human` unless the test names a format.
///
/// These assert on the one-line-per-finding stream, which is `human`; `review` became the default while they were written against the other one.
fn run(directory: &Path, arguments: &[&str]) -> Output {
    let mut arguments: Vec<&str> = arguments.to_vec();
    if !arguments.contains(&"--format") {
        arguments.push("--format");
        arguments.push("human");
    }
    common::isolated(binary())
        .env("XDG_CONFIG_HOME", no_user_config())
        .current_dir(directory)
        .args(&arguments)
        .output()
        .expect("the binary runs")
}

/// A repository whose configuration gives `TODO` a fortnight and lets `NOTE` stand indefinitely.
fn repository() -> TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path();
    git(path, &["init", "-q"], None);
    git(path, &["config", "user.email", "test@example.test"], None);
    git(path, &["config", "user.name", "OComment Test"], None);
    git(path, &["config", "commit.gpgsign", "false"], None);
    fs::write(
        path.join(".ocomment.toml"),
        b"version = 1\n\n[policy]\nmode = \"conservative\"\n\n[policy.allow]\ntags = [\"NOTE\"]\n\n[policy.allow.expiry]\nTODO = \"14d\"\n",
    )
    .expect("the fixture is writable");
    directory
}

#[test]
fn a_promise_is_fine_until_its_time_is_up() {
    let directory = repository();
    let path = directory.path();
    fs::write(
        path.join("sample.rs"),
        b"// TODO: an old promise\n\nfn a() {}\n\n// NOTE: a rationale\n",
    )
    .expect("the fixture is writable");
    git(path, &["add", "-A"], None);
    git(
        path,
        &["commit", "-qm", "old"],
        Some("2000-01-01T00:00:00 +0000"),
    );

    let output = run(path, &["check", "--explain", "sample.rs"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(
        stdout.contains("removable line comment: // TODO: an old promise"),
        "a promise long past its deadline was kept:\n{stdout}"
    );
    assert!(
        stdout.contains("`TODO` is a promise with 14d to keep it, and this line is"),
        "the reason does not name the rule that decided it:\n{stdout}"
    );
    assert!(
        stdout.contains("; do it, or delete the comment"),
        "the reason does not say what to do about it:\n{stdout}"
    );
    /* NOTE: The other tag has no deadline, so no amount of age reaches it. */
    assert!(
        stdout.contains("kept line comment: // NOTE: a rationale"),
        "a tag with no deadline was given one:\n{stdout}"
    );
    assert!(
        stderr.contains("1 comment past its deadline: 1 TODO. Do it or delete it."),
        "the run never complained about it:\n{stderr}"
    );
}

/// Writing one costs nothing.
/// The deadline starts at the commit that adds the line, so a `TODO` written a moment ago is attributed to no commit and has not started counting.
#[test]
fn a_promise_written_just_now_has_not_started_counting() {
    let directory = repository();
    let path = directory.path();
    fs::write(path.join("sample.rs"), b"fn a() {}\n").expect("the fixture is writable");
    git(path, &["add", "-A"], None);
    git(
        path,
        &["commit", "-qm", "old"],
        Some("2000-01-01T00:00:00 +0000"),
    );
    fs::write(
        path.join("sample.rs"),
        b"fn a() {}\n\n// TODO: written just now\n",
    )
    .expect("the fixture is writable");

    let output = run(path, &["check", "sample.rs"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    assert!(
        !stderr.contains("deadline"),
        "an uncommitted promise was called overdue:\n{stderr}"
    );
}

/// No repository, no clock.
/// A deadline nobody can measure has not passed, and the run says nothing rather than removing a comment on a guess.
#[test]
fn a_tree_that_is_not_a_repository_leaves_every_promise_alone() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path();
    fs::write(
        path.join(".ocomment.toml"),
        b"version = 1\n\n[policy]\nmode = \"conservative\"\n\n[policy.allow.expiry]\nTODO = \"0d\"\n",
    )
    .expect("the fixture is writable");
    fs::write(path.join("sample.rs"), b"// TODO: a promise\nfn a() {}\n")
        .expect("the fixture is writable");

    let output = run(path, &["check", "sample.rs"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
}

/// `fix` removes one, because the rule is the same rule and `fix` applies the rules.
/// That is the half of "do it or delete it" a machine can do.
#[test]
fn fix_deletes_a_promise_whose_time_is_up() {
    let directory = repository();
    let path = directory.path();
    fs::write(
        path.join("sample.rs"),
        b"// TODO: an old promise\n\nfn a() {}\n",
    )
    .expect("the fixture is writable");
    git(path, &["add", "-A"], None);
    git(
        path,
        &["commit", "-qm", "old"],
        Some("2000-01-01T00:00:00 +0000"),
    );

    let output = run(path, &["fix", "sample.rs"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(path.join("sample.rs")).expect("the file is readable"),
        "\n\nfn a() {}\n",
        "the promise is still in the file"
    );
}

/// The age is read from the bytes under judgement rather than from the file on disk, so an editing hook asked about an edit that has not happened yet gets the same answer a check would give once it had.
#[test]
fn a_proposed_edit_is_judged_against_the_history_of_the_file_it_would_change() {
    let directory = repository();
    let path = directory.path();
    let file = path.join("sample.rs");
    fs::write(&file, b"// TODO: an old promise\n\nfn a() {}\n").expect("the fixture is writable");
    git(path, &["add", "-A"], None);
    git(
        path,
        &["commit", "-qm", "old"],
        Some("2000-01-01T00:00:00 +0000"),
    );

    let payload = serde_json::json!({
        "cwd": path.to_string_lossy(),
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": file.to_string_lossy(),
            "old_string": "fn a() {}",
            "new_string": "fn a() {}\n\nfn b() {}",
        },
    })
    .to_string();
    let mut child = common::isolated(binary())
        .env("XDG_CONFIG_HOME", no_user_config())
        .current_dir(path)
        .args(["hook", "claude-code"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the binary runs");
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .expect("a piped standard input")
            .write_all(payload.as_bytes())
            .expect("the child accepts its input");
    }
    let output = child.wait_with_output().expect("the binary finishes");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let decision: serde_json::Value =
        serde_json::from_str(&stdout).expect("the decision parses as JSON");
    let reason = decision["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .expect("a reason the model can read");
    /* NOTE: The edit adds no comment.
     * The one already in the file is over its deadline, and the line it sits on is unchanged by the edit, so the history still answers for it. */
    assert!(
        reason.contains("do it or drop it"),
        "a promise the edit left in place stopped being overdue:\n{reason}"
    );
}
