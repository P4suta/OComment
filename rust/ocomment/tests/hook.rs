//! The agent-facing report, and the editing hook that carries it.
//!
//! Two promises are under test and neither is visible from any other test.
//! The report is an instruction rather than a listing: the verb on each line is the rule that decided the comment, and a comment that only had to move must not be reported as one to delete.
//! The hook is silent unless it has something to say — a hook that spoke on every event would stand between an agent and every file it touched.

use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ocomment")
}

/// A project with the three shape rules turned on, so a single fixture reaches all three verbs.
fn project() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n\n[policy]\nmode = \"conservative\"\n\n[policy.allow]\ntags = [\"NOTE\"]\nmax_lines = 1\ntrailing = false\n",
    )
    .expect("the fixture is writable");
    directory
}

fn run(directory: &Path, arguments: &[&str], stdin: &str) -> (String, String, i32) {
    use std::io::Write;
    let mut child = Command::new(binary())
        .current_dir(directory)
        .env("PATH", "/usr/bin:/bin")
        .args(arguments)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .as_mut()
        .expect("a piped standard input")
        .write_all(stdin.as_bytes())
        .expect("the child accepts its input");
    let output = child.wait_with_output().expect("the binary finishes");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn the_verb_on_each_line_is_the_rule_that_decided_the_comment() {
    let directory = project();
    std::fs::write(
        directory.path().join("sample.rs"),
        b"// a paragraph\n// over two lines\nfn a() {}\nfn b() {} // NOTE: beside code\nfn c() {}\n// plain\n",
    )
    .expect("the fixture is writable");
    let (stdout, stderr, code) = run(
        directory.path(),
        &["check", "--format", "agent", "sample.rs"],
        "",
    );
    assert_eq!(code, 1, "findings exit 1: {stdout}{stderr}");
    assert!(
        stdout.contains("FINDING sample.rs:1-2") && stdout.contains("shorten to 1 line"),
        "a comment that only had to be shorter was not reported that way:\n{stdout}"
    );
    /* NOTE: Tagged, so the tag rule kept it and the trailing rule took it back.
     * The verb is the one that decided it: an untagged comment beside code is removed by the policy and would still be removed a line higher up, so telling a reader to move that one would be wrong advice. */
    assert!(
        stdout.contains("move it above the code, or drop it")
            && stdout.contains("FINDING sample.rs:4"),
        "a comment that only had to move was not reported that way:\n{stdout}"
    );
    assert!(
        stdout.contains("FINDING sample.rs:6") && stdout.contains("- // plain"),
        "a comment the policy removed was not reported that way:\n{stdout}"
    );
    assert!(
        stdout.contains("rule: policy `conservative` removes line and block comments."),
        "the report never says what the project accepts:\n{stdout}"
    );
    assert!(
        stdout.contains("Allowed: a comment tagged NOTE, at most 1 line of adjacent comments, never beside code."),
        "the report never says what the project accepts:\n{stdout}"
    );
    assert!(
        stderr.is_empty(),
        "a machine format wrote to standard error:\n{stderr}"
    );
}

#[test]
fn a_clean_run_writes_nothing_at_all() {
    let directory = project();
    std::fs::write(
        directory.path().join("sample.rs"),
        b"// NOTE: one line.\nfn a() {}\n",
    )
    .expect("the fixture is writable");
    let (stdout, stderr, code) = run(
        directory.path(),
        &["check", "--format", "agent", "sample.rs"],
        "",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "", "a clean run wrote a report");
    assert_eq!(stderr, "", "a clean run wrote commentary");
}

fn hook_payload(event: &str, tool: &str, input: Value, directory: &Path) -> String {
    json!({
        "session_id": "test",
        "cwd": directory.to_string_lossy(),
        "permission_mode": "default",
        "hook_event_name": event,
        "tool_name": tool,
        "tool_use_id": "test",
        "tool_input": input,
    })
    .to_string()
}

#[test]
fn a_write_that_would_add_a_comment_is_refused_before_it_happens() {
    let directory = project();
    let path = directory.path().join("sample.rs");
    let payload = hook_payload(
        "PreToolUse",
        "Write",
        json!({
            "file_path": path.to_string_lossy(),
            "content": "fn a() {}\n// a paragraph\n// over two lines\n",
        }),
        directory.path(),
    );
    let (stdout, _, code) = run(directory.path(), &["hook", "claude-code"], &payload);
    assert_eq!(
        code, 0,
        "a decision is carried in the JSON, not in the status"
    );
    let decision: Value = serde_json::from_str(&stdout).expect("the decision parses as JSON");
    let output = &decision["hookSpecificOutput"];
    assert_eq!(output["hookEventName"], "PreToolUse");
    assert_eq!(output["permissionDecision"], "deny");
    let reason = output["permissionDecisionReason"]
        .as_str()
        .expect("a reason the model can read");
    assert!(
        reason.contains("shorten to 1 line"),
        "the refusal does not say what to do:\n{reason}"
    );
    /* NOTE: The file does not hold these bytes and may not exist, so `fix` would be sent at something that is not there. */
    assert!(
        reason.contains("not on disk yet: write it without them"),
        "the refusal sent the reader to a file that does not hold these bytes:\n{reason}"
    );
    assert!(
        !path.exists(),
        "judging a proposal must not create the file"
    );
}

#[test]
fn a_write_that_would_be_clean_is_waved_through_silently() {
    let directory = project();
    let payload = hook_payload(
        "PreToolUse",
        "Write",
        json!({
            "file_path": directory.path().join("sample.rs").to_string_lossy(),
            "content": "fn a() {}\n// NOTE: one line.\n",
        }),
        directory.path(),
    );
    let (stdout, stderr, code) = run(directory.path(), &["hook", "claude-code"], &payload);
    assert_eq!(code, 0);
    /* NOTE: Silence rather than `allow`: answering `allow` would wave the edit past the permission rules its user set, which is not this hook's business. */
    assert_eq!(stdout, "", "a clean edit got a decision it did not need");
    assert_eq!(stderr, "");
}

#[test]
fn an_edit_is_judged_by_what_the_file_would_become() {
    let directory = project();
    let path = directory.path().join("sample.rs");
    std::fs::write(&path, b"fn a() {}\nfn b() {}\n").expect("the fixture is writable");
    let payload = hook_payload(
        "PreToolUse",
        "Edit",
        json!({
            "file_path": path.to_string_lossy(),
            "old_string": "fn b() {}",
            "new_string": "// explains b\n// at length\nfn b() {}",
        }),
        directory.path(),
    );
    let (stdout, _, code) = run(directory.path(), &["hook", "claude-code"], &payload);
    assert_eq!(code, 0);
    let decision: Value = serde_json::from_str(&stdout).expect("the decision parses as JSON");
    let reason = decision["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .expect("a reason the model can read");
    /* NOTE: Line 2 of the file the edit would produce, not line 1 of the replacement: the hook judges the whole file, so the position it reports is the one the reader will find. */
    assert!(
        reason.contains("sample.rs:2-3"),
        "the report is not in the coordinates of the file:\n{reason}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file is still readable"),
        "fn a() {}\nfn b() {}\n",
        "judging a proposal must not apply it"
    );
}

#[test]
fn a_comment_that_lands_is_reported_back_after_the_fact() {
    let directory = project();
    let path = directory.path().join("sample.rs");
    std::fs::write(&path, b"// explains a\n// at length\nfn a() {}\n")
        .expect("the fixture is writable");
    let payload = hook_payload(
        "PostToolUse",
        "Write",
        json!({ "file_path": path.to_string_lossy() }),
        directory.path(),
    );
    let (stdout, stderr, code) = run(directory.path(), &["hook", "claude-code"], &payload);
    /* NOTE: The edit already happened, so there is nothing to refuse.
     * Exit 2 is how this host puts the text of standard error in front of the model. */
    assert_eq!(
        code, 2,
        "an unclean file after the fact did not report back"
    );
    assert_eq!(stdout, "");
    assert!(
        stderr.contains("shorten to 1 line"),
        "the correction does not say what to do:\n{stderr}"
    );
    assert!(
        stderr.contains("REMOVE-ALL [\"ocomment\",\"fix\"]"),
        "the file is on the disk, so `fix` is a way to do it:\n{stderr}"
    );
}

/// A hook that answered events it was not asked about would stand between an agent and every file it read, every command it ran, and every session it started.
#[test]
fn the_hook_is_silent_about_everything_that_is_not_an_edit() {
    let directory = project();
    let path = directory.path().join("sample.rs");
    std::fs::write(&path, b"// explains a\n// at length\nfn a() {}\n")
        .expect("the fixture is writable");
    let quiet = [
        // NOTE: Reading a file with comments in it is not writing one.
        hook_payload(
            "PostToolUse",
            "Read",
            json!({ "file_path": path.to_string_lossy() }),
            directory.path(),
        ),
        // NOTE: A command is not a file.
        hook_payload(
            "PreToolUse",
            "Bash",
            json!({ "command": "ls" }),
            directory.path(),
        ),
        // NOTE: An event about neither.
        json!({ "hook_event_name": "SessionStart", "cwd": directory.path().to_string_lossy() })
            .to_string(),
        /* NOTE: An edit whose replacement is not in the file is one the host will refuse on its own; judging the bytes it would have produced would be judging bytes that never exist. */
        hook_payload(
            "PreToolUse",
            "Edit",
            json!({
                "file_path": path.to_string_lossy(),
                "old_string": "nothing like this is in the file",
                "new_string": "// a comment",
            }),
            directory.path(),
        ),
    ];
    for payload in quiet {
        let (stdout, stderr, code) = run(directory.path(), &["hook", "claude-code"], &payload);
        assert_eq!(code, 0, "the hook took a position on `{payload}`");
        assert_eq!(stdout, "", "the hook answered `{payload}`");
        assert_eq!(stderr, "", "the hook answered `{payload}`");
    }
}

/// A payload this build cannot read is the host's business rather than the edit's: standing in the way of an edit nothing has judged would make every protocol change a broken editing session.
#[test]
fn an_unreadable_payload_fails_without_blocking_the_edit() {
    let directory = project();
    let (stdout, stderr, code) = run(directory.path(), &["hook", "claude-code"], "not json");
    assert_eq!(code, 2, "an error is an error");
    assert_eq!(stdout, "");
    assert!(
        stderr.contains("hook payload"),
        "the failure does not say what it could not read:\n{stderr}"
    );
}
