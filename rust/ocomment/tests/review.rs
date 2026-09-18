//! The two formats a reader decides from, pinned byte for byte.
//!
//! A layout is a contract like any other. These two are read by a person on a
//! screen and by an agent through a pipe, and both of them build a habit out of
//! the shape: where the count sits, what a marker means, which line is the edit.
//! A change to any of that is a change to the contract, and the way to make one
//! deliberate is to make it show up in a diff.
//!
//! Both are taken from the same file, because the point of the pair is that
//! they say the same thing. If one of these ever has to change without the
//! other, that is the finding.

use std::{
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
        .env_remove("NO_COLOR")
        .args(arguments)
        .output()
        .expect("the binary runs")
}

/// One file holding one of every decision, so neither golden can lose a branch
/// without the diff saying which.
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
  NO  5 comments in 1 file · 1 scanned · policy conservative

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
# FINDING under it. A FINDING names a path and the first and last line of one
# comment, which may span several. `-` is what is there now, `+` what would
# replace it, `=` the code the comment is about. KEEP names a file and `|` the
# setting that would stop the question being asked. BROKEN is a file that did
# not parse. The argv lines are commands, ready to run.

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
FINDING src/budget.rs:15
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
/// Every neighbouring tool switches layout when standard output stops being a
/// terminal, and here that would put a person and the agent working beside them
/// in front of two different reports of the same run. The test runs in a pipe,
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
/// A run that says only what it removed is a run whose judgement nobody can
/// audit: the reader is told five went and has no way to check that the sixth
/// was right to stay. Asking for the decisions again would be worse -- they are
/// answered, and the comments are not in the file any more.
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
/// It lived inside the one-line renderer, so a second format arrived without
/// it: the summary a CI job greps for went missing, and the job that strips
/// this repository's own OCaml and rebuilds it is what noticed.
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
