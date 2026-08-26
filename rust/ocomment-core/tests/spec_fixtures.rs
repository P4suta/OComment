//! The shared fixture corpus in `spec/fixtures/v1`, run against this crate.
//!
//! `tools/differential.py` feeds the same corpus to this implementation and to
//! the OCaml reference and compares the two. That says the pair agree; it does
//! not say what they agree on, and it needs a built OCaml tree to say anything
//! at all. This test is the other half: it runs every case here, checks the
//! ones carrying a recorded `expect` block against it, and holds the whole
//! corpus to the engine's structural promises — a scan never panics, and the
//! edits of a transformation are sorted, non-overlapping, and reproduce the
//! output when applied in one pass.
//!
//! `spec/fixtures/README.md` documents the case schema and how an `expect`
//! block is recorded.

use ocomment_core::{
    ByteSpan, CommentKind, DeclarativeProfile, Disposition, Edit, Language, Layout, ScanReport,
    TransformOptions, TransformResult, apply_edits, scan, scan_profile, transform,
    transform_profile, transform_spans,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::PathBuf, str::FromStr};

// INVARIANT: The floors live in `spec/fixtures/v1/floor.txt`, which
// INVARIANT: `tools/differential.py` reads too, so a case deleted from the
// INVARIANT: corpus fails the Rust test suite as well — on a machine with no
// INVARIANT: OCaml toolchain — and neither runner can be raised or lowered on
// INVARIANT: its own. `cases` is the least number of cases the corpus may hold;
// INVARIANT: `expectations` is the least number of those that must carry a
// INVARIANT: recorded `expect` block. A case with none is still held to the
// INVARIANT: structural promises below, so the second floor is what stops the
// INVARIANT: corpus from quietly degrading into that weaker check. Deleting a
// INVARIANT: block to re-record it is the documented way to change a recorded
// INVARIANT: behaviour, and `differential.py --record` puts it back before this
// INVARIANT: test is meant to run again.
const FLOOR_FILE: &str = "floor.txt";

/// One floor from `floor.txt`, which holds a `#` comment or a name and a
/// decimal count per line. A missing name is a failure rather than a zero: the
/// file is the only place either runner reads these numbers from.
fn floor(name: &str) -> usize {
    let path = corpus_directory().join(FLOOR_FILE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut fields = trimmed.split_whitespace();
        let (Some(key), Some(count), None) = (fields.next(), fields.next(), fields.next()) else {
            panic!(
                "{FLOOR_FILE}:{}: expected `name count`, got {line:?}",
                number + 1
            );
        };
        let count: usize = count.parse().unwrap_or_else(|error| {
            panic!(
                "{FLOOR_FILE}:{}: {count:?} is not a count: {error}",
                number + 1
            )
        });
        if key == name {
            return count;
        }
    }
    panic!("{FLOOR_FILE}: no `{name}` floor");
}

/// One comment as an `expect` block records it, and as the checks compare it.
#[derive(Debug, Eq, PartialEq)]
struct ExpectedComment {
    start: usize,
    end: usize,
    kind: String,
    action: String,
}

/// One diagnostic as an `expect` block records it.
#[derive(Debug, Eq, PartialEq)]
struct ExpectedDiagnostic {
    code: String,
    start: usize,
    end: usize,
}

/// Where the corpus and its `floor.txt` live, relative to this crate.
fn corpus_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/fixtures/v1")
}

/// Every corpus document, in file-name order, with the file each case came from.
fn corpus() -> Vec<(String, Value)> {
    let directory = corpus_directory();
    let mut paths: Vec<_> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("corpus directory entry").path())
        .filter(|path| path.extension().is_some_and(|value| value == "json"))
        .collect();
    paths.sort();
    assert!(
        !paths.is_empty(),
        "no corpus documents in {}",
        directory.display()
    );
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .expect("corpus file name")
                .to_string_lossy()
                .into_owned();
            let text =
                fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {name}: {error}"));
            let document: Value =
                serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {name}: {error}"));
            assert_eq!(
                document["version"],
                json!(1),
                "{name}: unsupported corpus version"
            );
            (name, document)
        })
        .collect()
}

/// Every case in the corpus, paired with the document it came from.
fn cases() -> Vec<(String, Value)> {
    corpus()
        .into_iter()
        .flat_map(|(name, document)| {
            let list = document["cases"]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: `cases` is not an array"));
            list.clone()
                .into_iter()
                .map(move |case| (name.clone(), case))
        })
        .collect()
}

/// The `id` of a case, which every message names.
fn id(case: &Value) -> &str {
    case["id"].as_str().expect("case `id` is a string")
}

/// The case source, from whichever of the two encodings it carries.
fn source_bytes(case: &Value) -> Vec<u8> {
    match (case.get("source_utf8"), case.get("source_base64")) {
        (Some(text), None) => text
            .as_str()
            .unwrap_or_else(|| panic!("{}: `source_utf8` is not a string", id(case)))
            .as_bytes()
            .to_vec(),
        (None, Some(encoded)) => decode_base64(
            encoded
                .as_str()
                .unwrap_or_else(|| panic!("{}: `source_base64` is not a string", id(case))),
        )
        .unwrap_or_else(|error| panic!("{}: `source_base64` {error}", id(case))),
        _ => panic!(
            "{}: exactly one of `source_utf8` and `source_base64`",
            id(case)
        ),
    }
}

/// The scan options a case asks for; `dialect` sits beside `language`, not in
/// `options`, and `layout` belongs to the transformation rather than the scan.
fn options(case: &Value) -> TransformOptions {
    let mut value = case.get("options").cloned().unwrap_or_else(|| json!({}));
    let object = value
        .as_object_mut()
        .unwrap_or_else(|| panic!("{}: `options` is not an object", id(case)));
    let layout = object
        .remove("layout")
        .map_or(Ok(Layout::Lines), serde_json::from_value);
    if let Some(dialect) = case.get("dialect") {
        object.insert("dialect".into(), dialect.clone());
    }
    TransformOptions {
        scan: serde_json::from_value(value)
            .unwrap_or_else(|error| panic!("{}: `options` {error}", id(case))),
        layout: layout.unwrap_or_else(|error| panic!("{}: `layout` {error}", id(case))),
    }
}

/// The language a case names.
fn language(case: &Value) -> Language {
    Language::from_str(
        case["language"]
            .as_str()
            .unwrap_or_else(|| panic!("{}: `language` is not a string", id(case))),
    )
    .unwrap_or_else(|error| panic!("{}: {error}", id(case)))
}

/// The external comment spans of a `transform-spans` case.
fn spans(case: &Value) -> Vec<(ByteSpan, CommentKind)> {
    case["spans"]
        .as_array()
        .unwrap_or_else(|| panic!("{}: `spans` is not an array", id(case)))
        .iter()
        .map(|span| {
            let start = usize::try_from(span["start"].as_u64().expect("span start"))
                .expect("span start fits");
            let end =
                usize::try_from(span["end"].as_u64().expect("span end")).expect("span end fits");
            let kind: CommentKind =
                serde_json::from_value(span["kind"].clone()).expect("span kind");
            (ByteSpan::new(start, end), kind)
        })
        .collect()
}

/// The caller-supplied edits of an `apply_edits` case.
fn edits(case: &Value) -> Vec<Edit> {
    case["edits"]
        .as_array()
        .unwrap_or_else(|| panic!("{}: `edits` is not an array", id(case)))
        .iter()
        .map(|edit| {
            let span = &edit["span"];
            let start = usize::try_from(span["start"].as_u64().expect("edit start"))
                .expect("edit start fits");
            let end =
                usize::try_from(span["end"].as_u64().expect("edit end")).expect("edit end fits");
            let replacement = decode_base64(
                edit["replacement_base64"]
                    .as_str()
                    .expect("edit replacement_base64"),
            )
            .expect("edit replacement_base64");
            Edit {
                span: ByteSpan::new(start, end),
                replacement,
            }
        })
        .collect()
}

/// The declarative profile of a `*-profile` case.
fn profile(case: &Value) -> DeclarativeProfile {
    serde_json::from_value(case["profile"].clone())
        .unwrap_or_else(|error| panic!("{}: `profile` {error}", id(case)))
}

/// What running one case produced: a report, and the bytes when it made any.
struct Outcome {
    report: Option<ScanReport>,
    output: Option<Vec<u8>>,
}

impl Outcome {
    /// A transformation, whose edits must reproduce its own output.
    fn transformed(case: &Value, source: &[u8], result: TransformResult) -> Self {
        assert!(
            result
                .edits
                .windows(2)
                .all(|pair| pair[0].span.end <= pair[1].span.start),
            "{}: edits are not sorted and non-overlapping: {:?}",
            id(case),
            result.edits
        );
        assert!(
            result
                .edits
                .iter()
                .all(|edit| edit.span.start <= edit.span.end && edit.span.end <= source.len()),
            "{}: an edit falls outside the {}-byte source: {:?}",
            id(case),
            source.len(),
            result.edits
        );
        assert_eq!(
            apply_edits(source, &result.edits),
            result.output,
            "{}: applying the edits does not reproduce the output",
            id(case)
        );
        Self {
            report: Some(result.report),
            output: Some(result.output),
        }
    }
}

/// Run one case through the operation it names.
fn run(case: &Value, source: &[u8]) -> Outcome {
    let operation = case
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or("transform");
    let options = options(case);
    match operation {
        "scan" => Outcome {
            report: Some(scan(source, language(case), options.scan)),
            output: None,
        },
        "transform" => {
            Outcome::transformed(case, source, transform(source, language(case), options))
        }
        "transform-spans" => Outcome::transformed(
            case,
            source,
            transform_spans(source, language(case), &spans(case), options)
                .unwrap_or_else(|error| panic!("{}: {error}", id(case))),
        ),
        "scan-profile" => Outcome {
            report: Some(
                scan_profile(source, &profile(case), options.scan)
                    .unwrap_or_else(|error| panic!("{}: {error}", id(case))),
            ),
            output: None,
        },
        "transform-profile" => Outcome::transformed(
            case,
            source,
            transform_profile(source, &profile(case), options)
                .unwrap_or_else(|error| panic!("{}: {error}", id(case))),
        ),
        "apply_edits" => Outcome {
            report: None,
            output: Some(apply_edits(source, &edits(case))),
        },
        other => panic!("{}: unsupported operation `{other}`", id(case)),
    }
}

/// Check one case against its recorded `expect` block.
fn check_expectation(case: &Value, expect: &Value, outcome: &Outcome) {
    let case_id = id(case);
    if let Some(valid) = expect.get("valid") {
        let report = outcome
            .report
            .as_ref()
            .unwrap_or_else(|| panic!("{case_id}: `expect.valid` needs an operation that reports"));
        assert_eq!(json!(report.valid), *valid, "{case_id}: `valid`");
    }
    if let Some(comments) = expect.get("comments") {
        let report = outcome.report.as_ref().unwrap_or_else(|| {
            panic!("{case_id}: `expect.comments` needs an operation that reports")
        });
        let observed: Vec<_> = report
            .comments
            .iter()
            .map(|comment| ExpectedComment {
                start: comment.span.start,
                end: comment.span.end,
                kind: comment.kind.as_str().to_owned(),
                action: match comment.disposition {
                    Disposition::Remove => "remove".to_owned(),
                    Disposition::Keep { .. } => "keep".to_owned(),
                },
            })
            .collect();
        let recorded: Vec<_> = comments
            .as_array()
            .unwrap_or_else(|| panic!("{case_id}: `expect.comments` is not an array"))
            .iter()
            .map(|comment| ExpectedComment {
                start: usize::try_from(comment["start"].as_u64().expect("comment start"))
                    .expect("fits"),
                end: usize::try_from(comment["end"].as_u64().expect("comment end")).expect("fits"),
                kind: comment["kind"].as_str().expect("comment kind").to_owned(),
                action: comment["action"]
                    .as_str()
                    .expect("comment action")
                    .to_owned(),
            })
            .collect();
        assert_eq!(observed, recorded, "{case_id}: `comments`");
    }
    if let Some(diagnostics) = expect.get("diagnostics") {
        let report = outcome.report.as_ref().unwrap_or_else(|| {
            panic!("{case_id}: `expect.diagnostics` needs an operation that reports")
        });
        let observed: Vec<_> = report
            .diagnostics
            .iter()
            .map(|item| ExpectedDiagnostic {
                code: item.code.clone(),
                start: item.span.start,
                end: item.span.end,
            })
            .collect();
        let recorded: Vec<_> = diagnostics
            .as_array()
            .unwrap_or_else(|| panic!("{case_id}: `expect.diagnostics` is not an array"))
            .iter()
            .map(|item| ExpectedDiagnostic {
                code: item["code"].as_str().expect("diagnostic code").to_owned(),
                start: usize::try_from(item["start"].as_u64().expect("diagnostic start"))
                    .expect("fits"),
                end: usize::try_from(item["end"].as_u64().expect("diagnostic end")).expect("fits"),
            })
            .collect();
        assert_eq!(observed, recorded, "{case_id}: `diagnostics`");
    }
    if let Some(wanted) = expected_output(case_id, expect) {
        let output = outcome.output.as_ref().unwrap_or_else(|| {
            panic!("{case_id}: `expect.output_*` needs an operation that writes bytes")
        });
        assert_eq!(
            String::from_utf8_lossy(output),
            String::from_utf8_lossy(&wanted),
            "{case_id}: `output` (lossy rendering; the bytes are what is compared)"
        );
        assert_eq!(*output, wanted, "{case_id}: `output` bytes");
    }
}

/// The output bytes an `expect` block pins, if it pins any.
fn expected_output(case_id: &str, expect: &Value) -> Option<Vec<u8>> {
    match (expect.get("output_utf8"), expect.get("output_base64")) {
        (Some(text), None) => Some(
            text.as_str()
                .unwrap_or_else(|| panic!("{case_id}: `output_utf8` is not a string"))
                .as_bytes()
                .to_vec(),
        ),
        (None, Some(encoded)) => Some(
            decode_base64(
                encoded
                    .as_str()
                    .unwrap_or_else(|| panic!("{case_id}: `output_base64` is not a string")),
            )
            .unwrap_or_else(|error| panic!("{case_id}: `output_base64` {error}")),
        ),
        (None, None) => None,
        (Some(_), Some(_)) => panic!("{case_id}: at most one of `output_utf8` and `output_base64`"),
    }
}

/// Standard base64 with padding; the corpus carries binary sources this way.
fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let cleaned: Vec<_> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if cleaned.len() % 4 != 0 {
        return Err("has a length that is not a multiple of four".into());
    }
    let value = |byte: u8| {
        TABLE
            .iter()
            .position(|candidate| *candidate == byte)
            .map(|index| u8::try_from(index).expect("base64 index fits"))
            .ok_or_else(|| format!("has the invalid byte {byte:#04x}"))
    };
    let mut output = Vec::with_capacity(cleaned.len() / 4 * 3);
    for chunk in cleaned.chunks(4) {
        let a = value(chunk[0])?;
        let b = value(chunk[1])?;
        output.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            let c = value(chunk[2])?;
            output.push((b << 4) | (c >> 2));
            if chunk[3] != b'=' {
                output.push((c << 6) | value(chunk[3])?);
            }
        }
    }
    Ok(output)
}

#[test]
fn the_corpus_is_well_formed() {
    let cases = cases();
    let minimum = floor("cases");
    assert!(
        cases.len() >= minimum,
        "the corpus holds {} case(s), fewer than the {minimum} required by \
         spec/fixtures/v1/{FLOOR_FILE}",
        cases.len()
    );
    let mut seen = BTreeSet::new();
    for (file, case) in &cases {
        let case_id = id(case);
        assert!(
            seen.insert(case_id.to_owned()),
            "duplicate fixture id `{case_id}` in {file}"
        );
        assert!(!case_id.is_empty(), "{file}: a case has an empty id");
        assert!(
            case.get("note")
                .and_then(Value::as_str)
                .is_some_and(|note| !note.is_empty()),
            "{case_id}: every case carries a `note` naming the specification it comes from"
        );
        let _ = source_bytes(case);
        let _ = language(case);
        let _ = options(case);
    }
}

#[test]
fn every_case_runs_and_keeps_the_engine_promises() {
    for (_, case) in cases() {
        let source = source_bytes(&case);
        let _ = run(&case, &source);
    }
}

#[test]
fn every_recorded_expectation_still_holds() {
    let mut recorded = 0;
    for (_, case) in cases() {
        let Some(expect) = case.get("expect") else {
            continue;
        };
        recorded += 1;
        let source = source_bytes(&case);
        let outcome = run(&case, &source);
        check_expectation(&case, expect, &outcome);
    }
    let minimum = floor("expectations");
    assert!(
        recorded >= minimum,
        "{recorded} case(s) carry a recorded `expect` block, fewer than the \
         {minimum} required by spec/fixtures/v1/{FLOOR_FILE}; record the missing \
         ones with `python3 tools/differential.py --record`"
    );
}
