//! What `--trace` records, and what it must not disturb.
//!
//! The trace is diagnostic: it goes to standard error so that the product on
//! standard output stays exactly what it was, and it is off unless asked for.
//! Both of those are properties a change could break without any other test
//! noticing, because every other test runs without the flag.

use std::{path::Path, process::Command};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ocomment")
}

/// A fixture reaching every kind of event the trace can record.
///
/// `diff` plans edits, so `edit-planned` is produced; the unreadable file
/// makes `file-skipped` happen; the source carries a kept comment and a
/// removed one so that `comment-decided` is seen deciding both ways.
fn fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        directory.path().join("sample.rs"),
        b"// SPDX-License-Identifier: MIT\nlet value = 1; // removable\n",
    )
    .expect("the fixture is writable");
    std::fs::write(
        directory.path().join("opaque.unknownext"),
        b"not a language\n",
    )
    .expect("the fixture is writable");
    directory
}

fn run(directory: &Path, arguments: &[&str]) -> (String, String) {
    let output = Command::new(binary())
        .current_dir(directory)
        .env("PATH", "/usr/bin:/bin")
        .args(arguments)
        .output()
        .expect("the binary runs");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn the_trace_is_off_until_it_is_asked_for() {
    let directory = fixture();
    let (_, stderr) = run(directory.path(), &["check", "."]);
    assert!(
        !stderr.contains("trace "),
        "a run that asked for no trace wrote one:\n{stderr}"
    );
    let (_, explicit) = run(directory.path(), &["check", ".", "--trace", "off"]);
    assert!(
        !explicit.contains("trace "),
        "`--trace off` wrote a trace:\n{explicit}"
    );
}

/// The trace must not reach standard output, whatever the format is.
///
/// This is the property that lets `--trace json` be combined with `--format
/// json`: if either one moved, the combination would stop producing a document
/// a caller can parse, and the caller would find out at run time.
#[test]
fn the_trace_leaves_the_product_alone() {
    let directory = fixture();
    let (plain, _) = run(directory.path(), &["scan", ".", "--format", "json"]);
    let (traced, stderr) = run(
        directory.path(),
        &["scan", ".", "--format", "json", "--trace", "json"],
    );
    assert_eq!(
        plain, traced,
        "asking for a trace changed the document on standard output"
    );
    serde_json::from_str::<serde_json::Value>(&traced).expect("the product is still one document");
    assert!(
        stderr.contains("\"event\""),
        "the trace did not reach standard error:\n{stderr}"
    );
}

/// Every event the trace can record is reached by one fixture.
///
/// A variant added without a fixture that produces it is a step the trace
/// claims to record and has never been observed recording, which is the same
/// gap as a gate that has only ever been seen passing.
#[test]
fn every_recorded_event_is_reached_by_a_fixture() {
    let directory = fixture();
    let (_, stderr) = run(
        directory.path(),
        &["diff", ".", "--quiet", "--trace", "json"],
    );
    let mut seen: Vec<String> = stderr
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value: serde_json::Value =
                serde_json::from_str(line).unwrap_or_else(|_| panic!("not a JSON line: {line}"));
            value["event"]
                .as_str()
                .expect("every event is tagged")
                .to_owned()
        })
        .collect();
    seen.sort_unstable();
    seen.dedup();
    let mut expected: Vec<String> = [
        "config-resolved",
        "file-detected",
        "file-skipped",
        "comment-decided",
        "edit-planned",
        "file-summary",
    ]
    .iter()
    .map(|name| (*name).to_owned())
    .collect();
    expected.sort_unstable();
    assert_eq!(
        seen, expected,
        "the fixture no longer reaches every kind of trace event"
    );
}

/// `--quiet` is what makes the stream parseable line by line.
///
/// Standard error carries the run summary too, so a caller that wants every
/// line to be an event has to say so. Documenting it is not enough: the
/// combination is pinned here.
#[test]
fn quiet_makes_every_error_line_an_event() {
    let directory = fixture();
    let (_, stderr) = run(
        directory.path(),
        &["check", ".", "--quiet", "--trace", "json"],
    );
    for line in stderr.lines().filter(|line| !line.trim().is_empty()) {
        serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|_| panic!("`--quiet --trace json` wrote a non-event line: {line}"));
    }
}

/// The human rendering names the evidence that chose the language.
///
/// It is the first thing a run that scanned a file as the wrong language
/// needs, and detection already knew it — it was being dropped.
#[test]
fn the_human_trace_names_the_evidence_for_a_language() {
    let directory = fixture();
    let (_, stderr) = run(directory.path(), &["check", ".", "--trace", "human"]);
    assert!(
        stderr.contains("sample.rs: rust/standard by extension"),
        "the trace did not say how the language was chosen:\n{stderr}"
    );
}

/// `selftest` re-runs the embedded corpus and says what it could not reach.
///
/// The count matters as much as the verdict: "agrees with everything" is a
/// weaker claim when the corpus it agreed with has quietly shrunk, which is
/// what the floors recorded beside the corpus are for.
#[test]
fn selftest_checks_the_embedded_corpus_and_accounts_for_what_it_skips() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let output = Command::new(binary())
        .current_dir(directory.path())
        .env("PATH", "/usr/bin:/bin")
        .args(["selftest", "--format", "json"])
        .output()
        .expect("the binary runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "selftest failed:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the report is one JSON document");
    assert_eq!(report["failures"].as_array().expect("failures").len(), 0);
    let cases = report["cases"].as_u64().expect("a case count");
    let checked = report["checked"].as_u64().expect("a checked count");
    let out_of_reach = report["out_of_reach"].as_u64().expect("a skipped count");
    assert_eq!(
        checked + out_of_reach,
        cases,
        "every case is either checked or accounted for as out of reach"
    );
    /* NOTE: A floor of its own, so that a corpus emptied by accident cannot
     * make this test pass by having nothing to disagree with. It is well under
     * the recorded floor, which is what actually guards the size; this only
     * guards against the corpus vanishing entirely. */
    assert!(
        checked > 100,
        "selftest checked only {checked} cases, so the corpus did not reach the binary"
    );
}

/// A wall of findings gets two lines saying what to do about it.
///
/// The count alone is what the run found; it says nothing about what to do.
/// Where the findings are, and whether one flag would answer all of them, are
/// both things the run already knows.
#[test]
fn a_concentrated_report_says_where_the_findings_are_and_what_would_answer_them() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    for (name, count) in [("many.rs", 12), ("few.rs", 4)] {
        let mut source = String::new();
        for index in 0..count {
            source.push_str(&format!("/// doc {index}\npub fn f{index}() {{}}\n"));
        }
        std::fs::write(directory.path().join(name), source).expect("the fixture is writable");
    }
    let (_, stderr) = run(directory.path(), &["check", ".", "--policy", "standard"]);
    assert!(
        stderr.contains("where they are: many.rs 12, few.rs 4"),
        "the summary did not rank the files:\n{stderr}"
    );
    /* NOTE: A policy, not a flag. `conservative` keeps documentation, so it
     * answers every finding on its own -- shorter than a list of kinds, and it
     * is the configuration the project should be keeping anyway. */
    assert!(
        stderr.contains("`--policy conservative` would make this run clean"),
        "the summary did not name the policy that answers every finding:\n{stderr}"
    );
}

/// Two kinds is still one flag, and that was the bug.
///
/// The rule was written as "only when one *kind* accounts for everything",
/// which withheld the advice from the only case that occurs: a Rust crate with
/// both documentation and a licence header produces exactly two kinds and
/// never one. `--keep-kind` is variadic, so two kinds is one flag — and here a
/// policy answers both, which is better still.
#[test]
fn two_kinds_are_still_one_change() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    for index in 0..6 {
        std::fs::write(
            directory.path().join(format!("f{index}.rs")),
            format!("// SPDX-License-Identifier: MIT\n/// doc {index}\npub fn f{index}() {{}}\n"),
        )
        .expect("the fixture is writable");
    }
    let (_, stderr) = run(directory.path(), &["check", ".", "--policy", "standard"]);
    assert!(
        stderr.contains("`--policy conservative` would make this run clean"),
        "the summary withheld advice from a two-kind run:\n{stderr}"
    );
}

/// When no policy answers, the advice is the one flag naming every kind.
#[test]
fn a_run_no_policy_answers_gets_the_flag_that_names_every_kind() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    for index in 0..6 {
        std::fs::write(
            directory.path().join(format!("f{index}.rs")),
            format!("// ordinary {index}\n/* block {index} */\npub fn f{index}() {{}}\n"),
        )
        .expect("the fixture is writable");
    }
    let (_, stderr) = run(directory.path(), &["check", "."]);
    assert!(
        stderr.contains("`--keep-kind line,block` would make this run clean"),
        "the summary did not name the one flag that answers every finding:\n{stderr}"
    );
}

/// Mixed kinds still get advice, and it is the flag rather than a policy.
///
/// Documentation and ordinary comments together: no policy keeps both, so
/// there is no policy to name, and the answer is the single `--keep-kind` that
/// names every kind present. The old rule refused to say anything here, on the
/// reading that two kinds need two flags. They do not.
#[test]
fn mixed_kinds_get_the_flag_when_no_policy_answers() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let mut source = String::new();
    for index in 0..8 {
        source.push_str(&format!("/// doc {index}\npub fn f{index}() {{}}\n"));
    }
    for index in 0..8 {
        source.push_str(&format!("// ordinary {index}\n"));
    }
    std::fs::write(directory.path().join("mixed.rs"), source).expect("the fixture is writable");
    let (_, stderr) = run(directory.path(), &["check", ".", "--policy", "standard"]);
    assert!(
        stderr.contains("where they are: mixed.rs 16"),
        "the summary did not rank the files:\n{stderr}"
    );
    assert!(
        stderr.contains("`--keep-kind line,doc-line` would make this run clean"),
        "the summary did not name the flag that answers every finding:\n{stderr}"
    );
}

/// A short report is left alone.
///
/// Under the threshold a reader has already read every line by the time they
/// reach the summary, and telling them where the findings are would be telling
/// them what they just saw.
#[test]
fn a_short_report_gets_no_summary_of_itself() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        directory.path().join("small.rs"),
        b"// one\n// two\nfn main() {}\n",
    )
    .expect("the fixture is writable");
    let (_, stderr) = run(directory.path(), &["check", "."]);
    assert!(
        !stderr.contains("where they are"),
        "a two-finding run summarised itself:\n{stderr}"
    );
}

/// `fix` checks what it is about to write before it writes it.
///
/// The check is that the result still lexes and holds nothing removable. A
/// scanner that produced a span opening a string would fail the first; one
/// that made a new comment token out of the bytes around a hole would fail the
/// second. Neither is reachable today, which is the point — this pins that the
/// check runs and passes on every rewrite, so a regression that made one
/// reachable stops at the gate instead of reaching a file.
#[test]
fn fix_verifies_the_bytes_it_is_about_to_write() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    /* NOTE: A file whose comments sit in the places a removal is most likely to
     * get wrong: beside a string holding a comment token, between two operators
     * that must not join, and at the end of a line. */
    std::fs::write(
        directory.path().join("edge.rs"),
        br#"fn main() {
    let s = "// not a comment";
    let joined = 7/*x*/+ 8;
    let negate = -/*x*/-9_i32;
    let _ = (s, joined, negate); // trailing
}
"#,
    )
    .expect("the fixture is writable");
    let (stdout, stderr) = run(directory.path(), &["fix", "."]);
    assert!(
        !stderr.contains("defect in OComment"),
        "the rewrite failed its own check:\n{stderr}"
    );
    assert!(stdout.contains("fixed edge.rs"), "{stdout}");
    let rewritten =
        std::fs::read_to_string(directory.path().join("edge.rs")).expect("the file was written");
    assert!(
        rewritten.contains(r#""// not a comment""#),
        "the string lost its contents:\n{rewritten}"
    );
    assert!(
        rewritten.contains("- -9_i32"),
        "the two minus signs joined:\n{rewritten}"
    );
    /* NOTE: And the check's own claim, made again from outside: a second run
     * finds nothing, which is what "idempotent" means and what the verifier
     * asserted before writing. */
    let (_, second) = run(directory.path(), &["check", "."]);
    assert!(
        second.contains("No removable comments"),
        "the rewrite was not a fixed point:\n{second}"
    );
}
