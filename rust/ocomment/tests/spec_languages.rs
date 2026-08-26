//! The shared language table, the detector, and the command that prints it.
//!
//! `spec/languages.toml` is what this repository publishes as the list of
//! languages OComment understands: the `files:` pattern of the pre-commit
//! hooks, the documentation page, and `ocomment languages` all come out of it.
//! A table like that is only worth publishing while it is true, so every claim
//! it makes is checked here against the code that would have to honour it —
//! `ocomment_core::detect_language` for the file names, the binary itself for
//! the dialects and for the listing — and the JSON listing is checked against
//! the table byte for byte, so the two cannot drift apart quietly.

use ocomment_core::{Dialect, Language, detect_language};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

/// The canonical table, as it sits in `spec/`.
const SPEC: &str = include_str!("../../../spec/languages.toml");

/// The copy the `ocomment` crate publishes and reads at run time.
/// `tools/check_embedded_specs.py` guards the same equality outside `cargo`.
const EMBEDDED: &str = include_str!("../assets/languages.toml");

/// The detector's own source, read as text so the table can be checked in the
/// other direction as well: what the spec claims is checked by running the
/// detector, and what the detector knows is read out of the file it is written
/// in, since nothing enumerates it at run time.
const DETECT: &str = include_str!("../../ocomment-core/src/detect.rs");

/// The extensions `detect_language` knows that `spec/languages.toml` does not
/// publish. Publishing one means regenerating the `files:` pattern of
/// `.pre-commit-hooks.yaml` from the spec — `python3 tools/check_hooks.py
/// --print-pattern` — so they are recorded here rather than quietly missing,
/// and this test fails the day the detector grows a sixth one.
const UNPUBLISHED_EXTENSIONS: [&str; 5] = ["cuh", "hh", "hxx", "mlt", "shtml"];

/// One language of the shared table.
///
/// `deny_unknown_fields` is what makes a typo in the spec a failing test rather
/// than a key that silently claims nothing.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    /// The canonical language name, which is also its serde spelling.
    name: String,
    /// Every file extension that selects this language, without the dot.
    extensions: Vec<String>,
    /// Every dialect the language accepts, in the order the binary lists them.
    dialects: Vec<String>,
    /// The extensions that select a dialect other than `standard`.
    #[serde(default)]
    extension_dialects: BTreeMap<String, String>,
    /// Whole file names that select the language when the extension does not.
    #[serde(default)]
    reserved_names: Vec<String>,
    /// Interpreter names that select the language from a `#!` line.
    #[serde(default)]
    shebangs: Vec<String>,
    /// A short remark for the human listing.
    #[serde(default)]
    notes: Option<String>,
}

/// The shared table as a whole.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    /// The schema version of the file; only `1` exists.
    version: u32,
    /// One entry per built-in language, in the order the binary lists them.
    languages: Vec<Entry>,
}

fn table() -> Table {
    let parsed: Table = toml::from_str(SPEC).expect("spec/languages.toml is valid TOML");
    assert_eq!(parsed.version, 1, "unknown spec/languages.toml version");
    parsed
}

/// The dialect the spec claims for an extension: the one it names, or
/// `standard` when it names none.
fn claimed_dialect(entry: &Entry, extension: &str) -> Dialect {
    let name = entry
        .extension_dialects
        .get(extension)
        .map_or("standard", String::as_str);
    name.parse()
        .unwrap_or_else(|error| panic!("`{name}` is not a dialect: {error}"))
}

fn language(name: &str) -> Language {
    name.parse()
        .unwrap_or_else(|error| panic!("`{name}` is not a language: {error}"))
}

/// Run the built binary somewhere no configuration file of this machine can
/// reach it, with `input` on its standard input.
fn run(arguments: &[&str], input: &[u8]) -> Output {
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ocomment"))
        .current_dir(home.path())
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("NO_COLOR", "1")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .expect("standard input was piped")
        .write_all(input)
        .unwrap();
    child.wait_with_output().unwrap()
}

/// The spec lists every language the binary has, under the name the binary
/// uses for it, in the same order. A language added to the core enum without a
/// row here would ship undocumented and unhooked.
#[test]
fn the_spec_lists_every_built_in_language() {
    let listed: Vec<String> = table()
        .languages
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    let built_in: Vec<String> = Language::ALL
        .iter()
        .map(|value| value.as_str().to_owned())
        .collect();
    assert_eq!(
        listed, built_in,
        "spec/languages.toml and `Language::ALL` disagree"
    );
}

/// Every extension in the spec really selects the language it is listed under,
/// and the dialect the spec claims for it. `.m` is Objective-C and `.cu` is
/// CUDA, and a table that says so has to be right about it.
#[test]
fn every_listed_extension_detects_its_language() {
    for entry in table().languages {
        let expected = language(&entry.name);
        for extension in &entry.extensions {
            let name = format!("sample.{extension}");
            let found = detect_language(Some(Path::new(&name)), b"")
                .unwrap_or_else(|| panic!("`{name}` is detected as nothing"));
            let dialect = claimed_dialect(&entry, extension);
            assert_eq!(
                (found.language, found.dialect, found.reason),
                (expected, dialect, "extension"),
                "`{name}` is not what spec/languages.toml says it is"
            );
            assert!(
                entry.dialects.contains(&dialect.as_str().to_owned()),
                "`.{extension}` selects `{dialect}`, which `{}` does not list",
                entry.name
            );
        }
        for extension in entry.extension_dialects.keys() {
            assert!(
                entry.extensions.contains(extension),
                "`{}` names a dialect for `.{extension}`, which it does not list",
                entry.name
            );
        }
    }
}

/// Every whole file name in the spec selects the language it is listed under.
/// `Dockerfile` has no extension to go on, so this is the only claim there is.
#[test]
fn every_listed_reserved_name_detects_its_language() {
    let mut checked = 0;
    for entry in table().languages {
        let expected = language(&entry.name);
        for reserved in &entry.reserved_names {
            let found = detect_language(Some(Path::new(reserved)), b"")
                .unwrap_or_else(|| panic!("`{reserved}` is detected as nothing"));
            assert_eq!(
                (found.language, found.reason),
                (expected, "reserved-filename"),
                "`{reserved}` is not what spec/languages.toml says it is"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "the spec claims no reserved file names");
}

/// Every interpreter name in the spec selects the language it is listed under
/// when it turns up in a `#!` line, which is all a piped script has to go on.
#[test]
fn every_listed_shebang_detects_its_language() {
    let mut checked = 0;
    for entry in table().languages {
        let expected = language(&entry.name);
        for interpreter in &entry.shebangs {
            let line = format!("#!/usr/bin/env {interpreter}\n");
            let found = detect_language(None, line.as_bytes())
                .unwrap_or_else(|| panic!("`{line:?}` is detected as nothing"));
            assert_eq!(
                (found.language, found.reason),
                (expected, "shebang"),
                "`{interpreter}` is not what spec/languages.toml says it is"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "the spec claims no shebangs");
}

/// The dialects the spec lists for a language are exactly the ones the binary
/// accepts for it, in the same order.
///
/// Both halves are read out of the binary rather than out of a second table in
/// this file: naming a dialect the language does not have is refused with the
/// list of the ones it does, which is `config::supported_dialects` verbatim, so
/// one refused run per language proves the whole row — and every dialect the
/// row lists is then run to prove the refusal was not lying about it.
#[test]
fn the_listed_dialects_are_the_dialects_the_binary_accepts() {
    for entry in table().languages {
        let listed: BTreeSet<&str> = entry.dialects.iter().map(String::as_str).collect();
        let absent = Dialect::ALL
            .iter()
            .find(|value| !listed.contains(value.as_str()))
            .expect("no language accepts every dialect");
        let refused = run(
            &[
                "strip",
                "--language",
                &entry.name,
                "--dialect",
                absent.as_str(),
            ],
            b"",
        );
        assert_eq!(
            refused.status.code(),
            Some(2),
            "`--dialect {absent}` was accepted for {}",
            entry.name
        );
        let error = String::from_utf8_lossy(&refused.stderr);
        let supported = error
            .split_once("supported: ")
            .unwrap_or_else(|| panic!("{} refused `{absent}` with: {error}", entry.name))
            .1
            .trim_end()
            .to_owned();
        assert_eq!(
            supported,
            entry.dialects.join(", "),
            "spec/languages.toml lists the wrong dialects for {}",
            entry.name
        );
        for dialect in &entry.dialects {
            let accepted = run(
                &["strip", "--language", &entry.name, "--dialect", dialect],
                b"",
            );
            assert_eq!(
                accepted.status.code(),
                Some(0),
                "`--language {} --dialect {dialect}` was refused: {}",
                entry.name,
                String::from_utf8_lossy(&accepted.stderr)
            );
        }
    }
}

/// The language and dialect enumerations of the published JSON schemas are the
/// same vocabulary as the spec table. `result.schema.json` describes what a run
/// reports and so also carries `unknown`, which is the only difference allowed.
#[test]
fn the_schemas_enumerate_the_same_vocabulary() {
    let config: Value = serde_json::from_str(include_str!("../../../spec/config.schema.json"))
        .expect("spec/config.schema.json is valid JSON");
    let result: Value = serde_json::from_str(include_str!("../../../spec/result.schema.json"))
        .expect("spec/result.schema.json is valid JSON");
    let names = |schema: &Value, definition: &str| -> Vec<String> {
        schema["$defs"][definition]["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("`{definition}` has no enum"))
            .iter()
            .map(|value| value.as_str().expect("enum values are strings").to_owned())
            .collect()
    };
    let spec = table();
    let languages: Vec<String> = spec
        .languages
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert_eq!(
        names(&config, "language"),
        languages,
        "spec/config.schema.json and spec/languages.toml disagree"
    );
    let mut reported = languages.clone();
    reported.push(Language::Unknown.as_str().to_owned());
    assert_eq!(
        names(&result, "language"),
        reported,
        "spec/result.schema.json and spec/languages.toml disagree"
    );
    let dialects: BTreeSet<String> = spec
        .languages
        .iter()
        .flat_map(|entry| entry.dialects.iter().cloned())
        .collect();
    assert_eq!(
        names(&config, "dialect")
            .into_iter()
            .collect::<BTreeSet<_>>(),
        dialects,
        "spec/config.schema.json and spec/languages.toml disagree about dialects"
    );
    assert_eq!(
        names(&config, "dialect"),
        Dialect::ALL
            .iter()
            .map(|value| value.as_str().to_owned())
            .collect::<Vec<_>>(),
        "spec/config.schema.json and `Dialect::ALL` disagree"
    );
}

/// The crate publishes and reads the spec table itself, so what a released
/// binary prints cannot be a copy that was edited on its own.
#[test]
fn the_embedded_table_is_the_spec_table() {
    assert_eq!(
        EMBEDDED, SPEC,
        "rust/ocomment/assets/languages.toml differs from spec/languages.toml"
    );
}

/// `ocomment languages --format json` is the shared table, rendered. Every key
/// the spec sets is in the JSON, and every key it leaves out is absent rather
/// than empty.
#[test]
fn the_json_listing_is_the_spec_table() {
    let listed = run(&["languages", "--format", "json"], b"");
    assert_eq!(
        listed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let printed: Value =
        serde_json::from_slice(&listed.stdout).expect("`languages --format json` writes JSON");
    let expected: Vec<Value> = table()
        .languages
        .iter()
        .map(|entry| {
            let mut object = Map::new();
            object.insert("name".to_owned(), json!(entry.name));
            object.insert("extensions".to_owned(), json!(entry.extensions));
            object.insert("dialects".to_owned(), json!(entry.dialects));
            if !entry.extension_dialects.is_empty() {
                object.insert(
                    "extension_dialects".to_owned(),
                    json!(entry.extension_dialects),
                );
            }
            if !entry.reserved_names.is_empty() {
                object.insert("reserved_names".to_owned(), json!(entry.reserved_names));
            }
            if !entry.shebangs.is_empty() {
                object.insert("shebangs".to_owned(), json!(entry.shebangs));
            }
            if let Some(notes) = &entry.notes {
                object.insert("notes".to_owned(), json!(notes));
            }
            Value::Object(object)
        })
        .collect();
    assert_eq!(
        printed,
        Value::Array(expected),
        "`ocomment languages --format json` is not spec/languages.toml"
    );
}

/// The human listing is the same table in columns: one row per language, in
/// spec order, carrying the extensions and dialects the spec gives it.
#[test]
fn the_human_listing_is_the_spec_table() {
    let listed = run(&["languages"], b"");
    assert_eq!(listed.status.code(), Some(0));
    let text = String::from_utf8(listed.stdout).expect("the listing is text");
    let mut rows = text.lines();
    assert_eq!(
        rows.next(),
        Some("language\textensions\tdialects\tnotes"),
        "the listing has no header"
    );
    for entry in table().languages {
        let row = rows
            .next()
            .unwrap_or_else(|| panic!("`{}` has no row", entry.name));
        let columns: Vec<&str> = row.split('\t').collect();
        assert_eq!(columns[0], entry.name, "rows are out of spec order: {row}");
        assert_eq!(
            columns[1],
            entry.extensions.join(","),
            "wrong extensions for `{}`",
            entry.name
        );
        assert_eq!(
            columns[2],
            entry.dialects.join(","),
            "wrong dialects for `{}`",
            entry.name
        );
        assert_eq!(
            columns.get(3).copied(),
            entry.notes.as_deref(),
            "wrong notes for `{}`",
            entry.name
        );
    }
    assert_eq!(rows.next(), None, "the listing has a row the spec does not");
}

/// Every string literal of one `match` block of the detector, which for the
/// two blocks read here is exactly the set of names that block answers to.
///
/// The block is found by the line that opens it and ends at the first line
/// that closes a `let` binding, so a rewrite of `detect.rs` that moves either
/// one fails this loudly rather than passing on an empty set.
fn match_keys(header: &str) -> BTreeSet<String> {
    let opened = DETECT
        .split_once(header)
        .unwrap_or_else(|| panic!("detect.rs no longer contains `{header}`"))
        .1;
    let body = opened
        .split_once("\n        };")
        .unwrap_or_else(|| panic!("the block opened by `{header}` is not closed as expected"))
        .0;
    let mut keys = BTreeSet::new();
    let mut rest = body;
    while let Some((_, after)) = rest.split_once('"') {
        let (key, tail) = after
            .split_once('"')
            .unwrap_or_else(|| panic!("an unterminated literal follows `{header}`"));
        assert!(
            !key.contains('\\'),
            "`{key}` is escaped, which this reader cannot undo"
        );
        keys.insert(key.to_owned());
        rest = tail;
    }
    assert!(!keys.is_empty(), "`{header}` matches on nothing");
    keys
}

/// The published table is checked against the detector above; this checks the
/// detector against the published table, which nothing else does. An extension
/// the detector answers to is either in `spec/languages.toml` — and so in the
/// hooks, the documentation, and the listing — or named as one that is not.
#[test]
fn the_detector_knows_no_unrecorded_extension() {
    let published: BTreeSet<String> = table()
        .languages
        .iter()
        .flat_map(|entry| entry.extensions.iter().cloned())
        .collect();
    let known = match_keys("let by_extension = match extension.as_str() {");
    let missing: BTreeSet<&str> = known
        .iter()
        .map(String::as_str)
        .filter(|extension| !published.contains(*extension))
        .collect();
    assert_eq!(
        missing,
        UNPUBLISHED_EXTENSIONS.into_iter().collect::<BTreeSet<_>>(),
        "the detector and spec/languages.toml disagree about which extensions exist"
    );
    let unknown: Vec<&String> = published.difference(&known).collect();
    assert!(
        unknown.is_empty(),
        "spec/languages.toml publishes extensions the detector does not know: {unknown:?}"
    );
}

/// The same, for the whole file names that carry no extension. Every one the
/// detector answers to is published, so this difference is empty in both
/// directions.
#[test]
fn the_detector_knows_no_unrecorded_file_name() {
    let published: BTreeSet<String> = table()
        .languages
        .iter()
        .flat_map(|entry| entry.reserved_names.iter())
        .map(|name| name.to_ascii_lowercase())
        .collect();
    let known = match_keys("let reserved = match lower.as_str() {");
    assert_eq!(
        known, published,
        "the detector and spec/languages.toml disagree about which file names are reserved"
    );
}
