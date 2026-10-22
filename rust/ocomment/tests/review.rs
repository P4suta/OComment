//! The two formats a reader decides from, pinned byte for byte.
//!
//! A layout is a contract like any other.
//! These two are read by a person on a screen and by an agent through a pipe, and both of them build a habit out of the shape: where the count sits, what a marker means, which line is the edit.
//! A change to any of that is a change to the contract, and the way to make one deliberate is to make it show up in a diff.
//!
//! Both are taken from the same file, because the point of the pair is that they say the same thing.
//! If one of these ever has to change without the other, that is the finding.

mod common;

use std::{path::Path, process::Output};
use tempfile::TempDir;

/// The `PATH` a run under test is given.
///
/// Fixed on Unix, so the suite reads the system's own tools rather than whatever the machine it runs on puts in front of them -- the author of this one has a `git` shim earlier on PATH that refuses a force push, and a suite that inherited it would be testing that.
/// Inherited on Windows, which has no such pair of fixed directories: a process needs the system ones on PATH to start at all, and Git is found through PATH or not found.
fn test_path() -> std::ffi::OsString {
    if cfg!(unix) {
        std::ffi::OsString::from("/usr/bin:/bin")
    } else {
        std::env::var_os("PATH").unwrap_or_default()
    }
}

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

fn run(directory: &Path, arguments: &[&str]) -> Output {
    common::isolated(binary())
        .env("XDG_CONFIG_HOME", no_user_config())
        .current_dir(directory)
        .env("PATH", test_path())
        .env_remove("NO_COLOR")
        .args(arguments)
        .output()
        .expect("the binary runs")
}

/// One file holding one of every decision, so neither golden can lose a branch without the diff saying which.
fn project() -> TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(directory.path().join(".ocomment.toml"), b"version = 1\n")
        .expect("the fixture is writable");
    std::fs::create_dir(directory.path().join("src")).expect("the fixture is writable");
    std::fs::write(
        directory.path().join("src/budget.rs"),
        b"use std::time::Duration;\n\
          \n\
          // The retry budget is per connection, not per request, because the\n\
          // server counts attempts against the socket it sees.\n\
          pub struct Budget {\n\
          \x20   remaining: u32,\n\
          }\n\
          \n\
          // TODO: make this configurable\n\
          const DEFAULT: u32 = 3;\n\
          \n\
          impl Budget {\n\
          \x20   /// A fresh budget.\n\
          \x20   pub fn new() -> Self {\n\
          \x20       Self { remaining: DEFAULT } // start full\n\
          \x20   }\n\
          }\n\
          \n\
          fn backoff(attempt: u32) -> Duration {\n\
          \x20   // self.remaining = 0;\n\
          \x20   Duration::from_millis(100 << attempt)\n\
          }\n",
    )
    .expect("the fixture is writable");
    directory
}

const REVIEW: &str = r#"
  NO  5 comments in 1 file · 1 file scanned · policy conservative

  DECIDE  make it a documentation comment                 2 comments
    src/budget.rs:3-4
      - // The retry budget is per connection, not per request, because the
      - // server counts attempts against the socket it sees.
      + /// The retry budget is per connection, not per request, because the
      + /// server counts attempts against the socket it sees.
        pub struct Budget {
    or keep them  [policy.allow]
                  tags = ["NOTE"]

  DECIDE  do what it promises, or delete it                1 comment
    src/budget.rs:9
      - // TODO: make this configurable
    or keep them  [policy.allow]
                  tags = ["TODO"]

  DECIDE  move it above the code, or drop it               1 comment
    src/budget.rs:15
      -         Self { remaining: DEFAULT } // start full
    or keep them  [policy.allow]
                  trailing = true

  DECIDE  delete it — the history has the code             1 comment
    src/budget.rs:20
      -     // self.remaining = 0;

  ALLOWED 1 comment this run did not report; `--explain` names the rule that kept each

  ──────────────────────────────────────────────────────────────────────
   ocomment fix   removes all 5, including anything above you meant to keep

"#;

const AGENT: &str = r#"# ocomment: 5 comments to answer for in 1 of 1 file scanned, policy conservative.
# Every line starts with a marker. DECIDE opens one question, asked of each
# FINDING under it, and only you can answer it. TIDY opens one this tool has
# already answered and is offering to write; TIDY-ALL applies every one of
# them and removes nothing. A FINDING names a path and the first and last line
# of one comment, which may span several, and the column when the comment does
# not open its line. `-` is what is there now, `+` what would replace it, `=`
# the code the comment is about. KEEP names a file and `|` the setting that
# would stop the question being asked. BROKEN is a file that did not parse.
# The argv lines are commands, ready to run.

DECIDE make it a documentation comment | 2 comments
FINDING src/budget.rs:3-4
- // The retry budget is per connection, not per request, because the
- // server counts attempts against the socket it sees.
+ /// The retry budget is per connection, not per request, because the
+ /// server counts attempts against the socket it sees.
= pub struct Budget {
KEEP .ocomment.toml
| [policy.allow]
| tags = ["NOTE"]

DECIDE do what it promises, or delete it | 1 comment
FINDING src/budget.rs:9
- // TODO: make this configurable
KEEP .ocomment.toml
| [policy.allow]
| tags = ["TODO"]

DECIDE move it above the code, or drop it | 1 comment
FINDING src/budget.rs:15:37
-         Self { remaining: DEFAULT } // start full
KEEP .ocomment.toml
| [policy.allow]
| trailing = true

DECIDE delete it — the history has the code | 1 comment
FINDING src/budget.rs:20
-     // self.remaining = 0;

# rule: policy `conservative` removes line and block comments.
RECHECK ["ocomment","check"]
REMOVE-ALL ["ocomment","fix"] removes 5 comments, including any above that were worth keeping
"#;

#[test]
fn the_review_report_is_the_one_recorded_here() {
    let directory = project();
    let output = run(
        directory.path(),
        &[
            "check",
            "--color",
            "never",
            "--hyperlinks",
            "never",
            "src/budget.rs",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), REVIEW);
}

#[test]
fn the_agent_report_is_the_one_recorded_here() {
    let directory = project();
    let output = run(
        directory.path(),
        &["check", "--format", "agent", "src/budget.rs"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), AGENT);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "",
        "a machine format wrote to standard error"
    );
}

/// The shape does not follow the terminal.
///
/// Every neighbouring tool switches layout when standard output stops being a terminal, and here that would put a person and the agent working beside them in front of two different reports of the same run.
/// The test runs in a pipe,
/// which is the case that would differ if it ever did.
#[test]
fn the_default_format_is_the_one_a_reader_decides_from() {
    let directory = project();
    let piped = run(
        directory.path(),
        &[
            "check",
            "--color",
            "never",
            "--hyperlinks",
            "never",
            "src/budget.rs",
        ],
    );
    let named = run(
        directory.path(),
        &[
            "check",
            "--format",
            "review",
            "--color",
            "never",
            "--hyperlinks",
            "never",
            "src/budget.rs",
        ],
    );
    assert_eq!(
        String::from_utf8_lossy(&piped.stdout),
        String::from_utf8_lossy(&named.stdout),
        "the default through a pipe is not the format it names"
    );
}

/// The stream a pipeline greps is still there, under its own name.
#[test]
fn the_line_per_finding_stream_is_still_reachable() {
    let directory = project();
    let output = run(
        directory.path(),
        &["check", "--format", "human", "src/budget.rs"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("src/budget.rs:3:1: removable line comment:"),
        "the grep-able stream changed shape:\n{stdout}"
    );
    assert_eq!(stdout.lines().count(), 5, "one line per finding:\n{stdout}");
}

/// After a fix, the half worth reading is what is still there.
///
/// A run that says only what it removed is a run whose judgement nobody can audit: the reader is told five went and has no way to check that the sixth was right to stay.
/// Asking for the decisions again would be worse -- they are answered, and the comments are not in the file any more.
#[test]
fn a_fix_says_what_it_left_behind() {
    let directory = project();
    let output = run(
        directory.path(),
        &[
            "fix",
            "--color",
            "never",
            "--hyperlinks",
            "never",
            "src/budget.rs",
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "\n  OK  5 comments removed from 1 file · 1 file scanned\n\
         \n  KEPT    1 comment, still in the files\n    \
         src/budget.rs:13  /// A fresh budget.\n\n"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("DECIDE"),
        "a fix asked again for decisions it had already answered"
    );
}

/// The commentary on standard error does not depend on the layout above it.
///
/// It lived inside the one-line renderer, so a second format arrived without it: the summary a CI job greps for went missing, and the job that strips this repository's own OCaml and rebuilds it is what noticed.
#[test]
fn the_summary_is_written_whichever_format_wrote_the_report() {
    let directory = project();
    for format in ["human", "review", "agent"] {
        let output = run(
            directory.path(),
            &["check", "--format", format, "src/budget.rs"],
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let expected = format != "agent";
        assert_eq!(
            stderr.contains("Found 5 removable comments in 1 file"),
            expected,
            "`--format {format}` wrote the wrong commentary:\n{stderr}"
        );
    }
}

/// A large report is a map, not a list.
///
/// This repository under `--policy all` finds over nine thousand comments.
/// Printed one finding at a time that is nearly eighteen thousand lines, and nobody reads the ten thousandth.
/// What a reader needs at that size is which decision, how many, where they are, and somewhere to start.
#[test]
fn a_report_too_large_to_read_becomes_one_to_navigate() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(directory.path().join(".ocomment.toml"), b"version = 1\n")
        .expect("the fixture is writable");
    let mut crowded = String::new();
    for index in 0..40 {
        crowded.push_str(&format!("let value{index} = {index}; // a note\n"));
    }
    std::fs::write(directory.path().join("crowded.rs"), crowded.as_bytes())
        .expect("the fixture is writable");
    std::fs::write(directory.path().join("quiet.rs"), b"let a = 1; // one\n")
        .expect("the fixture is writable");

    let output = run(
        directory.path(),
        &["check", "--color", "never", "--hyperlinks", "never", "."],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().count() < 30,
        "41 findings printed in full:\n{stdout}"
    );
    assert!(
        stdout.contains("crowded.rs") && stdout.contains("40"),
        "the report does not say where they are:\n{stdout}"
    );
    assert!(
        stdout.contains("ocomment check crowded.rs"),
        "the report does not say where to start:\n{stdout}"
    );
    assert!(
        stdout.contains("41 comments"),
        "the headline lost the total:\n{stdout}"
    );
}

/// A small report is still a list, because at that size the list is the answer.
#[test]
fn a_report_small_enough_to_read_stays_one_to_read() {
    let directory = project();
    let output = run(
        directory.path(),
        &[
            "check",
            "--color",
            "never",
            "--hyperlinks",
            "never",
            "src/budget.rs",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("more file") && !stdout.contains("in full"),
        "a five-finding report was summarised:\n{stdout}"
    );
}

/// `--explain` answers the question the report does not.
///
/// The report says what to do, which is read from where a comment sits.
/// `--explain` says why it is being asked, which is the rule the engine applied and the setting behind it.
/// They are different questions, and the flag was accepted and silently ignored -- the one shape this project refuses everywhere else.
#[test]
fn explain_names_the_rule_under_each_finding() {
    let directory = project();
    let output = run(
        directory.path(),
        &[
            "check",
            "--explain",
            "--color",
            "never",
            "--hyperlinks",
            "never",
            "src/budget.rs",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("removed: policy `conservative` removes ordinary comments"),
        "the rule that decided a finding is not under it:\n{stdout}"
    );
    let plain = run(
        directory.path(),
        &[
            "check",
            "--color",
            "never",
            "--hyperlinks",
            "never",
            "src/budget.rs",
        ],
    );
    assert_ne!(
        stdout,
        String::from_utf8_lossy(&plain.stdout),
        "`--explain` was accepted and changed nothing"
    );
}

/// The count of what was kept becomes the list of it.
///
/// `ALLOWED 1 comment this run did not report` is a promise that somebody checked.
/// The list is what lets a reader check the checker, and a gate nobody can audit when it is green is a gate whose green means nothing.
#[test]
fn explain_turns_the_allowed_count_into_the_list() {
    let directory = project();
    let output = run(
        directory.path(),
        &[
            "check",
            "--explain",
            "--color",
            "never",
            "--hyperlinks",
            "never",
            "src/budget.rs",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ALLOWED 1 comment this run did not report"),
        "{stdout}"
    );
    assert!(
        stdout.contains("src/budget.rs:13  /// A fresh budget."),
        "the kept comment is not named:\n{stdout}"
    );
    assert!(
        stdout.contains("kept: policy conservative protects documentation comments"),
        "the rule that kept it is not given:\n{stdout}"
    );
}

/// When the policy is stricter than the kind, the decision is about the policy.
///
/// A `///` taken out by `--policy all` is not a comment in the wrong place.
/// The advice read from the lines around it said "let the code say it, or tag it" and offered `[policy.allow] tags = ["NOTE"]`, and both halves were wrong: a doc comment carries no tag to allow, and allowing one would not reach a rule that is about kinds.
/// Under the default policy this never fires, because no policy keeps an ordinary comment.
#[test]
fn a_policy_stricter_than_the_kind_is_a_decision_about_the_policy() {
    let directory = project();
    let output = run(
        directory.path(),
        &[
            "check",
            "--policy",
            "all",
            "--color",
            "never",
            "--hyperlinks",
            "never",
            "src/budget.rs",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("keep `doc-line` comments with `conservative`"),
        "a doc comment removed by `all` got advice about where it sits:\n{stdout}"
    );
    assert!(
        stdout.contains("[policy]") && stdout.contains("mode = \"conservative\""),
        "the way to keep it is not the policy that keeps it:\n{stdout}"
    );

    let default = run(
        directory.path(),
        &[
            "check",
            "--color",
            "never",
            "--hyperlinks",
            "never",
            "src/budget.rs",
        ],
    );
    assert!(
        !String::from_utf8_lossy(&default.stdout).contains("mean to remove them"),
        "the default policy reached a decision that is only about a stricter one"
    );
}

/// The one number a reader takes from the headline is how much of the repository the run actually read, and it was the sum of what was read and what was passed over.
///
/// Every format that prints a coverage figure is checked here at once, because the three of them drifted apart the first time: the headline said seven, the end-of-run summary said two, and `ocomment coverage` said 28.5%.
/// Whichever one a reader believed, two of the three were wrong.
#[test]
fn a_headline_counts_what_was_read_and_not_what_was_passed_over() {
    let directory = project();
    for index in 0..5 {
        std::fs::write(
            directory.path().join(format!("data{index}.parquet")),
            b"not source\n",
        )
        .expect("the fixture is writable");
    }

    let review = run(directory.path(), &["check", "."]);
    let headline = String::from_utf8_lossy(&review.stdout)
        .lines()
        .find(|line| line.contains("scanned"))
        .expect("the headline says what was scanned")
        .to_owned();
    assert!(
        headline.contains("2 of 7 files scanned"),
        "the headline counted the files it skipped as files it scanned:\n{headline}"
    );

    /* NOTE: The machine format carries the same two numbers, because its reader is the one that cannot re-run the scan to check them. */
    let agent = run(directory.path(), &["check", ".", "--format", "agent"]);
    let first = String::from_utf8_lossy(&agent.stdout)
        .lines()
        .next()
        .expect("the agent report opens with its counts")
        .to_owned();
    assert!(
        first.contains("of 2 files scanned") && first.contains("5 files reached and not read"),
        "the agent headline does not separate what was read from what was not:\n{first}"
    );

    let coverage = run(directory.path(), &["coverage", "."]);
    assert!(
        String::from_utf8_lossy(&coverage.stdout).contains("2 of 7 files scanned"),
        "`coverage` and the headline disagree about the same run"
    );
}

/// A file no built-in language claims is read by a profile, in full, and the reports said `unknown` about it.
#[test]
fn every_report_says_which_reader_answered() {
    let directory = project();
    std::fs::write(
        directory.path().join(".gitignore"),
        b"# The second pattern is not redundant: the first has an inner slash.\n/target\n",
    )
    .expect("the fixture is writable");

    let scan = run(directory.path(), &["scan", ".", "--format", "json"]);
    let document: serde_json::Value =
        serde_json::from_slice(&scan.stdout).expect("the report parses as JSON");
    let ignored = document["files"]
        .as_array()
        .expect("a file array")
        .iter()
        .find(|file| file["path"] == ".gitignore")
        .expect("the profile read the file");
    assert_eq!(
        ignored["language"], "unknown",
        "no built-in language claims this file, and saying otherwise would be the lie the other way"
    );
    assert_eq!(
        ignored["read_by"],
        serde_json::json!({ "kind": "profile", "name": "hash-line" }),
        "the report does not say what read a file it read in full:\n{ignored}"
    );

    let coverage =
        String::from_utf8_lossy(&run(directory.path(), &["coverage", "."]).stdout).into_owned();
    assert!(
        coverage.contains("read by the `hash-line` profile"),
        "`coverage` counts a profile-read file as scanned without saying so:\n{coverage}"
    );
    assert!(
        coverage.contains("read by a built-in language"),
        "the readers are only legible beside each other:\n{coverage}"
    );

    /* NOTE: A listing of its own.
     * `ocomment languages` is spec/languages.toml rendered, and a profile name is not something `--language` takes. */
    let profiles =
        String::from_utf8_lossy(&run(directory.path(), &["profiles"]).stdout).into_owned();
    assert!(
        profiles.contains("hash-line\tbundled\t") && profiles.contains(".gitignore"),
        "`ocomment profiles` does not say this build can read the file:\n{profiles}"
    );
    /* NOTE: This project declares no profiles of its own, so every row is a shipped one.
     * The comparison is by value, and a shipped profile that left a field implicit would differ from its resolved copy and be reported here as the project's. */
    assert!(
        !profiles.contains("\tconfigured\t"),
        "a shipped profile is reported as one this project declared:\n{profiles}"
    );
}

/// A finding names the comment it was built from, not the line that comment sits on.
///
/// Two removable comments share a line whenever one of them sits beside code,
/// and the lookup that fetches a verdict for `--explain` matched on the line —
/// so it returned the first of the two for both findings, and a plain comment beside a directive was explained as `this one a `directive``.
/// Everything around that line was right: the decision, the settings that would keep it,
/// the code shown above.
/// Only the reason was another comment's.
#[test]
fn explain_asks_about_the_comment_the_finding_was_built_from() {
    let directory = project();
    std::fs::write(
        directory.path().join("a.js"),
        b"let x = 1; /* eslint-disable-line no-x */ /* plain prose */\n",
    )
    .expect("the fixture is writable");
    let output = run(
        directory.path(),
        &["check", "--policy", "all", "--explain", "a.js"],
    );
    let report = String::from_utf8_lossy(&output.stdout).into_owned();
    let reasons: Vec<&str> = report
        .lines()
        .filter(|line| line.trim_start().starts_with("removed:"))
        .collect();
    assert_eq!(
        reasons.len(),
        2,
        "the fixture is meant to produce one finding per comment:\n{report}"
    );
    assert!(
        reasons[0].contains("`directive`"),
        "the directive was not explained as one:\n{report}"
    );
    assert!(
        reasons[1].contains("`block`"),
        "the plain comment was explained as the directive beside it:\n{report}"
    );
}
