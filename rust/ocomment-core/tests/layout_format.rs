//! What a removal leaves behind, measured against a real formatter.
//!
//! The claim a layout makes is about bytes, and every other test here checks
//! it against bytes this repository wrote down. That leaves one question open:
//! whether the bytes a removal leaves are bytes the language's own formatter
//! would accept. A `compact` run that left a stray blank line is correct by
//! this crate's rules and fails `gofmt -l` in the caller's pipeline, and no
//! fixture of ours would have said so.
//!
//! So these ask the formatter. A source that the formatter already accepts is
//! stripped, and what comes out has to be accepted too — the removal took a
//! comment out, and taking a comment out is not a reformatting.
//!
//! A formatter that is not installed makes the case skip rather than fail, so
//! the suite still runs on a machine with one toolchain. `OCOMMENT_REQUIRE_FORMATTERS`
//! turns every skip into a failure, and CI sets it: a skip that can become
//! permanent is a test that quietly stopped running.

use ocomment_core::{Language, Layout, Policy, ScanOptions, TransformOptions, transform};
use std::{
    io::Write,
    process::{Command, Stdio},
};

/// One formatter, and how to ask it whether bytes are already in its normal
/// form.
struct Formatter {
    /// What the case is reported as.
    name: &'static str,
    /// The program, and the arguments that make it read standard input and
    /// write the normalized form to standard output.
    program: &'static str,
    arguments: &'static [&'static str],
    language: Language,
    /// Sources the formatter already accepts, chosen so that a removal leaves
    /// a hole in every position one can be left in: above an item, beside
    /// code, between two items, inside a block, over a run, and in a block
    /// comment.
    ///
    /// Carried on the formatter rather than looked up by language, so that a
    /// formatter added later comes with its own and there is no arm for one to
    /// fall into without.
    fixtures: &'static [(&'static str, &'static str)],
}

const FORMATTERS: [Formatter; 2] = [
    Formatter {
        name: "gofmt",
        program: "gofmt",
        arguments: &[],
        language: Language::Go,
        fixtures: &[
            (
                "above an item",
                "package main\n\n// a remark\nfunc a() {}\n",
            ),
            (
                "beside code",
                "package main\n\nfunc a() int {\n\tx := 1 // a remark\n\treturn x\n}\n",
            ),
            (
                "between items",
                "package main\n\nfunc a() {}\n\n// a remark\n\nfunc b() {}\n",
            ),
            (
                "inside a block",
                "package main\n\nfunc a() {\n\t// a remark\n\tprintln(\"x\")\n}\n",
            ),
            (
                "a run of them",
                "package main\n\n// one\n// two\n// three\nfunc a() {}\n",
            ),
            (
                "a block comment",
                "package main\n\n/*\nA remark that runs on.\n*/\nfunc a() {}\n",
            ),
        ],
    },
    /* NOTE: `--emit stdout` writes the formatted source; the edition is named
     * because rustfmt's default differs between toolchains and a fixture that
     * lexes under one and not another would fail for the wrong reason. */
    Formatter {
        name: "rustfmt",
        program: "rustfmt",
        arguments: &["--emit", "stdout", "--edition", "2021", "--quiet"],
        language: Language::Rust,
        fixtures: &[
            ("above an item", "// a remark\nfn a() {}\n"),
            (
                "beside code",
                "fn a() -> i32 {\n    let x = 1; // a remark\n    x\n}\n",
            ),
            ("between items", "fn a() {}\n\n// a remark\n\nfn b() {}\n"),
            (
                "inside a block",
                "fn a() {\n    // a remark\n    println!(\"x\");\n}\n",
            ),
            ("a run of them", "// one\n// two\n// three\nfn a() {}\n"),
            (
                "a block comment",
                "/*\nA remark that runs on.\n*/\nfn a() {}\n",
            ),
        ],
    },
];

/// The formatter's normal form for `source`, or `None` when it refused the
/// bytes.
fn formatted(formatter: &Formatter, source: &str) -> Option<String> {
    let mut child = Command::new(formatter.program)
        .args(formatter.arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    child
        .stdin
        .as_mut()
        .expect("a piped standard input")
        .write_all(source.as_bytes())
        .ok()?;
    let output = child.wait_with_output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn available(formatter: &Formatter) -> bool {
    Command::new(formatter.program)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Which layouts leave bytes a formatter still calls normal, measured rather
/// than assumed.
///
/// The answer is a table and not a rule, because the three layouts promise
/// different things and only one of them is compatible with being normal:
/// `lines` keeps every line number, so a removed comment leaves an empty line
/// where a formatter wants none, and `columns` keeps every column, so it
/// leaves the spaces the comment occupied. Those are the promises, not
/// defects — a caller who is diffing against a formatted tree wants `compact`,
/// and this is what says so with evidence.
///
/// Written as an exact expectation in both directions. A layout that stopped
/// conforming fails, and so does one that started: the second is a layout that
/// has quietly changed what it promises.
#[test]
fn only_compact_leaves_a_formatter_nothing_to_do() {
    let required = std::env::var_os("OCOMMENT_REQUIRE_FORMATTERS").is_some();
    let mut measured = 0usize;
    let mut wrong = Vec::new();
    for formatter in &FORMATTERS {
        if !available(formatter) {
            assert!(
                !required,
                "{} is not installed and OCOMMENT_REQUIRE_FORMATTERS is set",
                formatter.name
            );
            continue;
        }
        for (position, source) in formatter.fixtures.iter().copied() {
            let normal = formatted(formatter, source).unwrap_or_else(|| {
                panic!(
                    "{}: the fixture `{position}` is not valid input",
                    formatter.name
                )
            });
            assert_eq!(
                normal, source,
                "{}: the fixture `{position}` is not already in normal form",
                formatter.name
            );
            for layout in Layout::ALL {
                let result = transform(
                    source.as_bytes(),
                    formatter.language,
                    TransformOptions {
                        scan: ScanOptions {
                            policy: Policy::All,
                            ..Default::default()
                        },
                        layout,
                    },
                );
                assert!(
                    result.report.valid,
                    "{}: {position} did not scan",
                    formatter.name
                );
                let stripped = String::from_utf8_lossy(&result.output).into_owned();
                measured += 1;
                let conformed =
                    formatted(formatter, &stripped).is_some_and(|normal| normal == stripped);
                let expected = layout == Layout::Compact;
                if conformed != expected {
                    wrong.push(format!(
                        "{} / {layout} / {position}: {}\n{}",
                        formatter.name,
                        if expected {
                            "a removal left bytes the formatter would rewrite, which is a \
                             caller's `gofmt -l` failing on a diff OComment produced"
                        } else {
                            "this layout is documented as keeping line numbers or columns \
                             and therefore as leaving a formatter something to do; it no \
                             longer does, so what it promises has changed"
                        },
                        indent(&stripped)
                    ));
                }
            }
        }
    }
    assert!(
        measured > 0 || !required,
        "no formatter was reachable, so nothing was measured"
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n\n"));
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("    |{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
