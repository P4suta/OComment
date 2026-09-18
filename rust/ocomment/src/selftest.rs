//! What this binary can show about itself, on the machine it was installed on.
//!
//! The test suite proves the source is correct on the machine that ran it. It
//! says nothing about the artefact somebody downloaded: an archive that lost
//! bytes, a build for an architecture the project has never run a test on, a
//! package a distributor patched. Those produce a binary that starts, answers
//! `--version`, and is wrong.
//!
//! So the shared corpus travels inside the binary and can be re-run on demand.
//! `ocomment selftest` scans every case and compares the result against the
//! expectation recorded with it — the same cases `tools/differential.py` gives
//! to the OCaml reference and `spec_fixtures.rs` gives to the library, asked of
//! the executable in the reader's hands.
//!
//! The corpus earns its place here because of what it is. `hazards.json` is not
//! a set of examples: every case in it is a form that was got wrong once — a
//! `#` inside a Perl regex, a Swift regex literal that looks like division, a
//! Rust lifetime that looks like a character. A binary that still gets all of
//! those right is a binary whose lexer arrived intact.

use crate::output::{Detail, OutputFormat, Verbosity, note, stdout, wrote};
use anyhow::{Context, Result};
use ocomment_core::{
    DeclarativeProfile, Disposition, Language, Layout, ScanReport, TransformOptions, scan,
    scan_profile, transform, transform_profile,
};
use serde_json::{Value, json};
use std::{io::Write, str::FromStr};

/// The corpus, embedded so that it is present wherever the binary is.
///
/// Derived from `spec/fixtures/v1` by `tools/gen_selftest_corpus.py`, which
/// keeps the input, the options and the recorded result and drops what the
/// check has no use for -- the prose explaining each case, the diagnostics and
/// edits of the differential protocol, and the indentation. That is 239 KB
/// rather than 533 KB, for the same 486 cases.
///
/// It is a derivation rather than a second source. `--check` on that script
/// fails when it no longer matches what `spec/fixtures/v1` would produce,
/// which is what stops the binary from certifying itself against cases the
/// project has moved on from.
const CORPUS: &str = include_str!("../assets/selftest-corpus.json");

/// One case that did not do what was recorded for it.
struct Failure {
    id: String,
    detail: String,
}

/// What a case produced.
struct Outcome {
    report: Option<ScanReport>,
    output: Option<Vec<u8>>,
}

/// Run every embedded case and report whether this binary still agrees.
pub fn run(format: OutputFormat, verbosity: Verbosity) -> Result<u8> {
    let document = parse_corpus()?;
    let case_floor = floor(&document, "cases")?;
    let expectation_floor = floor(&document, "expectations")?;
    let cases = document
        .get("cases")
        .and_then(Value::as_array)
        .context("the embedded self-test corpus has no `cases` array")?;

    let mut failures = Vec::new();
    let mut checked = 0usize;
    let mut out_of_reach = 0usize;
    for case in cases {
        let id = case
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("<unnamed>")
            .to_owned();
        let Some(expect) = case.get("expect") else {
            continue;
        };
        match execute(case) {
            Ok(Some(outcome)) => {
                checked += 1;
                if let Some(detail) = compare(&outcome, expect) {
                    failures.push(Failure {
                        id: id.clone(),
                        detail,
                    });
                }
            }
            Ok(None) => out_of_reach += 1,
            Err(error) => {
                checked += 1;
                failures.push(Failure {
                    id: id.clone(),
                    detail: format!("did not run: {error}"),
                });
            }
        }
    }

    /* NOTE: A corpus that shrank is a corpus that stopped asking something, and
     * a self-test happily reporting "all 3 cases passed" is the failure this
     * guards against. The floors are the same two `tools/differential.py` and
     * the library test read, so none of the three can be lowered alone. */
    if cases.len() < case_floor {
        failures.push(Failure {
            id: "<corpus>".to_owned(),
            detail: format!(
                "holds {} cases, and the floor recorded with it is {case_floor}",
                cases.len()
            ),
        });
    }
    /* NOTE: Compared against every case that carries an expectation, not just
     * the ones this binary can reach, because the floor counts what the corpus
     * records rather than what any one runner asks. */
    let recorded = checked + out_of_reach;
    if recorded < expectation_floor {
        failures.push(Failure {
            id: "<corpus>".to_owned(),
            detail: format!(
                "{recorded} cases carry a recorded expectation, and the floor is {expectation_floor}"
            ),
        });
    }

    report(
        format,
        verbosity,
        cases.len(),
        checked,
        out_of_reach,
        &failures,
    )?;
    Ok(u8::from(!failures.is_empty()) * 2)
}

/// The embedded corpus, parsed once.
fn parse_corpus() -> Result<Value> {
    serde_json::from_str(CORPUS).context("the embedded self-test corpus is not JSON")
}

/// One of the floors recorded beside the corpus and carried with it.
///
/// They are the same two `tools/differential.py` and the library test read, so
/// none of the three runners can be lowered on its own.
fn floor(document: &Value, name: &str) -> Result<usize> {
    document
        .get("floors")
        .and_then(|floors| floors.get(name))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .with_context(|| format!("the embedded corpus records no `{name}` floor"))
}

/// Run one case through the operation it names.
///
/// Only the operations a shipped binary can answer for are run. A case built
/// around a caller-supplied edit list or an externally supplied span is asking
/// about an API rather than about this executable, and is left to the library
/// test that can call it.
fn execute(case: &Value) -> Result<Option<Outcome>> {
    let source = source_bytes(case)?;
    let options = options(case)?;
    let operation = case
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or("transform");
    Ok(Some(match operation {
        "scan" => Outcome {
            report: Some(scan(&source, language(case)?, options.scan)),
            output: None,
        },
        "transform" => {
            let result = transform(&source, language(case)?, options);
            Outcome {
                report: Some(result.report),
                output: Some(result.output),
            }
        }
        "scan-profile" => {
            let profile = profile(case)?;
            Outcome {
                report: Some(
                    scan_profile(&source, &profile, options.scan)
                        .map_err(|error| anyhow::anyhow!("{error}"))?,
                ),
                output: None,
            }
        }
        "transform-profile" => {
            let profile = profile(case)?;
            let result = transform_profile(&source, &profile, options)
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            Outcome {
                report: Some(result.report),
                output: Some(result.output),
            }
        }
        /* NOTE: `transform-spans` and `apply_edits` take a caller-supplied span
         * list or edit list. They ask about the library's API rather than about
         * this executable, and there is no command that reaches them, so they
         * are not failures here -- but they are not silently dropped either.
         * The count is reported, because "484 checked" and "486 checked" are
         * different claims and only one of them is true. */
        _ => return Ok(None),
    }))
}

/// The bytes a case scans.
fn source_bytes(case: &Value) -> Result<Vec<u8>> {
    if let Some(text) = case.get("source_utf8").and_then(Value::as_str) {
        return Ok(text.as_bytes().to_vec());
    }
    let encoded = case
        .get("source_base64")
        .and_then(Value::as_str)
        .context("a case carries neither `source_utf8` nor `source_base64`")?;
    decode_base64(encoded)
}

/// The options a case asks for; `layout` belongs to the transformation and
/// `dialect` may sit beside `language` rather than inside `options`.
fn options(case: &Value) -> Result<TransformOptions> {
    let mut value = case.get("options").cloned().unwrap_or_else(|| json!({}));
    let object = value
        .as_object_mut()
        .context("`options` is not an object")?;
    let layout = match object.remove("layout") {
        Some(layout) => serde_json::from_value(layout).context("`layout`")?,
        None => Layout::Lines,
    };
    if let Some(dialect) = case.get("dialect") {
        object.insert("dialect".into(), dialect.clone());
    }
    Ok(TransformOptions {
        scan: serde_json::from_value(value).context("`options`")?,
        layout,
    })
}

fn language(case: &Value) -> Result<Language> {
    let name = case
        .get("language")
        .and_then(Value::as_str)
        .context("`language` is missing")?;
    Language::from_str(name).map_err(|error| anyhow::anyhow!("{error}"))
}

fn profile(case: &Value) -> Result<DeclarativeProfile> {
    serde_json::from_value(
        case.get("profile")
            .cloned()
            .context("a profile case carries no `profile`")?,
    )
    .context("`profile`")
}

/// What differs between an outcome and the expectation recorded for it.
fn compare(outcome: &Outcome, expect: &Value) -> Option<String> {
    if let Some(valid) = expect.get("valid").and_then(Value::as_bool) {
        let observed = outcome.report.as_ref()?.valid;
        if observed != valid {
            return Some(format!("`valid` is {observed}, expected {valid}"));
        }
    }
    if let Some(comments) = expect.get("comments").and_then(Value::as_array) {
        let report = outcome.report.as_ref()?;
        if report.comments.len() != comments.len() {
            return Some(format!(
                "found {} comments, expected {}",
                report.comments.len(),
                comments.len()
            ));
        }
        for (found, wanted) in report.comments.iter().zip(comments) {
            let action = match found.disposition {
                Disposition::Remove => "remove",
                Disposition::Keep { .. } => "keep",
            };
            let start = wanted["start"].as_u64().unwrap_or_default() as usize;
            let end = wanted["end"].as_u64().unwrap_or_default() as usize;
            let kind = wanted["kind"].as_str().unwrap_or_default();
            let expected_action = wanted["action"].as_str().unwrap_or_default();
            if found.span.start != start
                || found.span.end != end
                || found.kind.as_str() != kind
                || action != expected_action
            {
                return Some(format!(
                    "comment at {}..{} is {} {action}, expected {start}..{end} {kind} {expected_action}",
                    found.span.start,
                    found.span.end,
                    found.kind.as_str()
                ));
            }
        }
    }
    if let Some(wanted) = expected_output(expect)
        && let Some(output) = outcome.output.as_ref()
        && *output != wanted
    {
        return Some(format!(
            "output is {:?}, expected {:?}",
            String::from_utf8_lossy(output),
            String::from_utf8_lossy(&wanted)
        ));
    }
    None
}

/// The output bytes an expectation pins, if it pins any.
fn expected_output(expect: &Value) -> Option<Vec<u8>> {
    if let Some(text) = expect.get("output_utf8").and_then(Value::as_str) {
        return Some(text.as_bytes().to_vec());
    }
    expect
        .get("output_base64")
        .and_then(Value::as_str)
        .and_then(|text| decode_base64(text).ok())
}

/// Standard base64, which is how the corpus carries bytes that are not UTF-8.
fn decode_base64(text: &str) -> Result<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        if byte == b'=' {
            break;
        }
        let value = ALPHABET
            .iter()
            .position(|candidate| *candidate == byte)
            .with_context(|| format!("not base64: {:?}", byte as char))?;
        accumulator = (accumulator << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push(u8::try_from((accumulator >> bits) & 0xff).expect("one byte"));
        }
    }
    Ok(bytes)
}

/// Say what the run found, in the shape the format asks for.
fn report(
    format: OutputFormat,
    verbosity: Verbosity,
    total: usize,
    checked: usize,
    out_of_reach: usize,
    failures: &[Failure],
) -> Result<()> {
    let mut out = stdout();
    match format {
        OutputFormat::Json | OutputFormat::Jsonl => {
            let document = json!({
                "version": 1,
                "cases": total,
                "checked": checked,
                "out_of_reach": out_of_reach,
                "failures": failures
                    .iter()
                    .map(|failure| json!({"case": failure.id, "detail": failure.detail}))
                    .collect::<Vec<_>>(),
            });
            wrote(writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&document).expect("the report serializes")
            ))?;
        }
        OutputFormat::Human
        | OutputFormat::Review
        | OutputFormat::Sarif
        | OutputFormat::Github
        | OutputFormat::Agent => {
            for failure in failures {
                wrote(writeln!(out, "{}: {}", failure.id, failure.detail))?;
            }
        }
    }
    crate::output::finish(&mut out)?;

    if !matches!(format, OutputFormat::Json | OutputFormat::Jsonl) {
        let stderr = std::io::stderr();
        let mut summary = stderr.lock();
        let aside = if out_of_reach == 0 {
            String::new()
        } else {
            format!(
                " {out_of_reach} more ask about a library API no command reaches, and were not run."
            )
        };
        let line = if failures.is_empty() {
            format!(
                "This binary agrees with all {checked} expectations it can check, out of {total} shared cases.{aside}"
            )
        } else {
            format!(
                "{} of {checked} recorded expectations did not hold; this binary is not the one the corpus describes.",
                failures.len()
            )
        };
        note(&mut summary, verbosity, Detail::Normal, &line)?;
    }
    Ok(())
}
