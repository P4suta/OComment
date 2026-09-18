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
