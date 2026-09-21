//! The three things a gate needs besides a verdict: a way to narrow itself to what a branch changed, a refusal to be silently green, and numbers a later step can read.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
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

fn git(directory: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(directory)
        .args(arguments)
        .output()
        .expect("git runs");
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
    Command::new(binary())
        .env("XDG_CONFIG_HOME", no_user_config())
        .current_dir(directory)
        .args(&arguments)
        .output()
        .expect("the binary runs")
}

fn repository() -> TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path();
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "user.email", "test@example.test"]);
    git(path, &["config", "user.name", "OComment Test"]);
    git(path, &["config", "commit.gpgsign", "false"]);
    directory
}

/// A branch answers for what it changed and not for what it inherited.
#[test]
fn base_reports_the_files_this_branch_changed_and_no_others() {
    let directory = repository();
    let path = directory.path();
    fs::write(path.join("old.rs"), b"fn a() {} // an old comment\n").expect("writable");
    git(path, &["add", "-A"]);
    git(path, &["commit", "-qm", "first"]);
    git(path, &["checkout", "-q", "-b", "work"]);
    fs::write(path.join("new.rs"), b"fn b() {} // a new comment\n").expect("writable");
    git(path, &["add", "-A"]);
    git(path, &["commit", "-qm", "second"]);

    let whole = run(path, &["check", "--quiet"]);
    assert_eq!(whole.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&whole.stdout).lines().count(),
        2,
        "the whole tree holds two findings"
    );

    let narrowed = run(path, &["check", "--quiet", "--base", "main"]);
    let stdout = String::from_utf8_lossy(&narrowed.stdout);
    assert_eq!(narrowed.status.code(), Some(1));
    assert!(
        stdout.contains("new.rs") && !stdout.contains("old.rs"),
        "--base reported a file the branch never touched:\n{stdout}"
    );
}

/// A path named beside `--base` narrows it further rather than replacing it.
#[test]
fn base_and_a_path_are_an_intersection() {
    let directory = repository();
    let path = directory.path();
    fs::create_dir(path.join("src")).expect("writable");
    fs::write(path.join("root.rs"), b"fn a() {}\n").expect("writable");
    fs::write(path.join("src/inner.rs"), b"fn b() {}\n").expect("writable");
    git(path, &["add", "-A"]);
    git(path, &["commit", "-qm", "first"]);
    fs::write(path.join("root.rs"), b"fn a() {} // outside\n").expect("writable");
    fs::write(path.join("src/inner.rs"), b"fn b() {} // inside\n").expect("writable");

    let output = run(path, &["check", "--quiet", "--base", "HEAD", "src"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("inner.rs") && !stdout.contains("root.rs"),
        "the path did not narrow the diff:\n{stdout}"
    );
}

/// A branch with nothing to check exits 0, which reads exactly like a clean one.
/// The run is right and the silence is the trap, so the silence goes.
#[test]
fn a_gate_that_examined_nothing_says_so() {
    let directory = repository();
    let path = directory.path();
    fs::write(path.join("a.rs"), b"fn a() {} // a comment\n").expect("writable");
    git(path, &["add", "-A"]);
    git(path, &["commit", "-qm", "first"]);

    let base = run(path, &["check", "--base", "HEAD"]);
    assert_eq!(base.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&base.stderr).contains("no changed files to check"),
        "an empty --base run said nothing:\n{}",
        String::from_utf8_lossy(&base.stderr)
    );

    let staged = run(path, &["check", "--staged"]);
    assert_eq!(staged.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&staged.stderr);
    assert!(
        stderr.contains("nothing is staged") && stderr.contains("--all-files"),
        "an empty --staged run said nothing, which is how it becomes a gate that \
         is green forever:\n{stderr}"
    );
}

/// The counts are the run's own, so they are the same numbers whatever the product was written in.
#[test]
fn the_summary_is_the_same_whatever_format_the_product_took() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path();
    fs::write(path.join("a.rs"), b"fn a() {}\n// one\n// two\n").expect("writable");
    fs::write(path.join("b.rs"), b"fn b() {} // three\n").expect("writable");

    /* NOTE: Written outside the tree under test.
     * A summary left beside the sources would be the next run's third file. */
    let elsewhere = tempfile::tempdir().expect("a temporary directory");
    let mut written = Vec::new();
    for format in ["human", "json", "jsonl", "sarif", "github", "agent"] {
        let file = elsewhere.path().join(format!("{format}.json"));
        let output = run(
            path,
            &[
                "check",
                "--quiet",
                "--format",
                format,
                "--summary",
                file.to_str().expect("a UTF-8 temporary path"),
            ],
        );
        assert_eq!(output.status.code(), Some(1), "{format}");
        written.push(fs::read_to_string(&file).expect("the summary was written"));
    }
    for other in &written[1..] {
        assert_eq!(&written[0], other, "two formats reported different counts");
    }
    let summary: serde_json::Value =
        serde_json::from_str(&written[0]).expect("the summary parses as JSON");
    assert_eq!(summary["version"], 1);
    assert_eq!(summary["operation"], "check");
    assert_eq!(summary["files_scanned"], 2);
    assert_eq!(summary["files_with_findings"], 2);
    assert_eq!(summary["removable_comments"], 3);
    assert_eq!(summary["comments_removed"], 0);
    assert_eq!(summary["removable_by_kind"]["line"], 3);
    assert_eq!(summary["top_files"][0]["path"], "a.rs");
    assert_eq!(summary["top_files"][0]["removable"], 2);
}

/// Only a run that reached the disk may claim a removal.
#[test]
fn only_a_fix_reports_comments_removed() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path();
    fs::write(path.join("a.rs"), b"fn a() {}\n// one\n").expect("writable");
    let elsewhere = tempfile::tempdir().expect("a temporary directory");
    let file = elsewhere.path().join("summary.json");
    let argument = file.to_str().expect("a UTF-8 temporary path");

    run(path, &["check", "--quiet", "--summary", argument]);
    let checked: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).expect("written")).expect("parses");
    assert_eq!(checked["comments_removed"], 0);

    run(path, &["fix", "--quiet", "--summary", argument]);
    let fixed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).expect("written")).expect("parses");
    assert_eq!(fixed["operation"], "fix");
    assert_eq!(fixed["comments_removed"], 1);
    assert_eq!(fixed["files_changed"], 1);
}

/// A project's own tools are in nobody's catalogue, and `keep_regex` cannot stand in for one: a pattern leaves the comment ordinary, and `--policy all` is entitled to an ordinary comment.
#[test]
fn a_project_can_name_the_markers_its_own_tools_read() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path();
    fs::write(
        path.join(".ocomment.toml"),
        br#"version = 1

[policy]
mode = "all"
keep_regex = ['^// pattern-only']
protected = [
  { contains = "rust-mutants:", reason = "read by the mutation tester", tier = "load-bearing" },
  { contains = "my-linter:", reason = "read by our linter" },
]
"#,
    )
    .expect("writable");
    fs::write(
        path.join("a.rs"),
        b"// rust-mutants: skip\nfn a() {}\n// my-linter: allow\nfn b() {}\n// pattern-only\nfn c() {}\n",
    )
    .expect("writable");

    let output = run(path, &["scan", "a.rs"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("load-bearing keep (read by the mutation tester)"),
        "the stronger tier did not survive `--policy all`, and the project's own \
         words are not on the line:\n{stdout}"
    );
    /* NOTE: The weaker tier is a directive, and `all` -- having said it would take every comment -- takes it.
     * That is the difference the two tiers are for, and it is why declaring the stronger one has to be an act. */
    assert!(
        stdout.contains("directive remove"),
        "the weaker tier was not reached:\n{stdout}"
    );
    /* NOTE: What a `keep_regex` can do, for contrast: it holds the comment back but leaves it an ordinary line comment. */
    assert!(
        stdout.contains("line keep"),
        "a keep_regex no longer holds a comment back:\n{stdout}"
    );
}

/// A convention drifts in two directions and the report looks both ways.
#[test]
fn the_tag_inventory_reports_both_kinds_of_drift() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path();
    fs::write(
        path.join(".ocomment.toml"),
        b"version = 1\n\n[policy.allow]\ntags = [\"NOTE\", \"SAFETY\"]\n",
    )
    .expect("writable");
    fs::write(
        path.join("a.rs"),
        b"// NOTE: a reason\n// XXX: undone\n// XXX: again\nfn a() {}\n",
    )
    .expect("writable");

    let output = run(path, &["tags"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "an inventory is not a gate");
    assert!(stdout.contains("  NOTE\t1"), "{stdout}");
    /* NOTE: Marked on its listing line as well as in the sentence: the listing is what a reader scans. */
    assert!(stdout.contains("! XXX\t2"), "{stdout}");
    assert!(
        stdout.contains("Allowed and never written: SAFETY."),
        "a tag the configuration protects and nothing writes went unreported:\n{stdout}"
    );
    assert!(
        stdout.contains("Written and not allowed: XXX (2)."),
        "a tag this tree writes and nothing allows went unreported, which is \
         usually the first anybody hears of it:\n{stdout}"
    );

    let json = run(path, &["tags", "--format", "json"]);
    let document: serde_json::Value =
        serde_json::from_slice(&json.stdout).expect("the inventory parses as JSON");
    assert_eq!(document["counts"]["XXX"], 2);
    assert_eq!(document["unused"][0], "SAFETY");
    assert_eq!(document["unconfigured"][0]["tag"], "XXX");
}

/// A percentage of what a walk happened to reach is a percentage of nothing.
///
/// `hidden = false` is the default, so a repository's `.github/` is not walked — and the walk never met those files, so nothing reported them and `coverage` said `100.0%`.
/// Every workflow a project has is under there.
#[test]
fn coverage_counts_the_files_the_walk_never_reached() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path();
    fs::create_dir_all(path.join(".github/workflows")).expect("writable");
    fs::write(path.join("a.rs"), b"fn a() {}\n").expect("writable");
    fs::write(
        path.join(".github/workflows/ci.yml"),
        b"on: [push]  # here\n",
    )
    .expect("writable");
    fs::write(path.join(".hidden.toml"), b"# and here\n").expect("writable");

    let output = run(path, &["coverage"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("1 of 3 files scanned (33.3%)"),
        "the percentage is of the walk rather than of the tree:\n{stdout}"
    );
    assert!(
        stdout.contains("2: hidden file or directory ([files] hidden = false)"),
        "the reason does not name the setting a reader would change:\n{stdout}"
    );

    /* NOTE: And the other direction: with the rule lifted they are walked, so the same tree reports full coverage and the line disappears.
     * A report that said `100.0%` either way would be saying nothing. */
    fs::write(
        path.join(".ocomment.toml"),
        b"version = 1\n\n[files]\nhidden = true\n",
    )
    .expect("writable");
    let walked = run(path, &["coverage"]);
    let stdout = String::from_utf8_lossy(&walked.stdout);
    assert!(
        stdout.starts_with("4 of 4 files scanned (100.0%)"),
        "lifting the rule did not walk them:\n{stdout}"
    );
    assert!(!stdout.contains("hidden file"), "{stdout}");
}
