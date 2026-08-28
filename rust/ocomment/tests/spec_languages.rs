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

/// The prose that states, in words or in figures, how many languages OComment
/// scans or how many editor language identifiers the extension attaches to.
/// Nothing derives these sentences, so nothing but a test stops the next
/// language from leaving them behind — and a description that undercounts is
/// read by everyone who installs the extension.
const COMPARISON: &str = include_str!("../../../docs/comparison.md");
const EDITORS: &str = include_str!("../../../docs/editors.md");
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");
const VSCODE_PACKAGE: &str = include_str!("../../../editors/vscode/package.json");
const VSCODE_README: &str = include_str!("../../../editors/vscode/README.md");
const VSCODE_CHANGELOG: &str = include_str!("../../../editors/vscode/CHANGELOG.md");

/// The extensions `detect_language` knows that `spec/languages.toml` does not
/// publish. There are none: an extension the detector answers to is one the
/// hooks match, the documentation lists and `ocomment languages` prints.
///
/// The list stays here rather than the assertion being narrowed to "empty",
/// because it is what an extension added to the detector alone lands in, and
/// what it costs to put one here is the point: publishing one means
/// regenerating the `files:` pattern of `.pre-commit-hooks.yaml` from the spec
/// — `python3 tools/check_hooks.py --print-pattern` — and the languages page
/// with it, so an extension left out is left out on purpose and in writing.
const UNPUBLISHED_EXTENSIONS: [&str; 0] = [];

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

/// The same, for the interpreter names a `#!` line is read for. Unlike the
/// extensions and the reserved names, this set is not read out of the
/// detector's source: `ocomment_core::shebang_interpreters` publishes it, so
/// what is compared here is the table the detector actually searches rather
/// than a reading of the file it is written in.
///
/// `every_listed_shebang_detects_its_language` runs the detector over every
/// name the spec claims; this is the other direction, and it is the one that
/// catches an interpreter taught to the detector and never written down —
/// which would leave a piped script detected as a language `ocomment
/// languages` says nothing about.
#[test]
fn the_detector_knows_no_unrecorded_shebang() {
    let published: BTreeSet<String> = table()
        .languages
        .iter()
        .flat_map(|entry| entry.shebangs.iter().cloned())
        .collect();
    let known: BTreeSet<String> = ocomment_core::shebang_interpreters()
        .map(ToOwned::to_owned)
        .collect();
    assert_eq!(
        known, published,
        "the detector and spec/languages.toml disagree about which interpreters exist"
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

/// The English word for a small number, so a count written out in prose can be
/// checked against the number it means. The list stops where the prose does: a
/// repository with more than thirty-nine languages needs another entry here,
/// which is the same edit as the sentence it guards.
fn number_word(value: usize) -> String {
    const UNITS: [&str; 20] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    const TENS: [&str; 4] = ["twenty", "thirty", "forty", "fifty"];
    if value < UNITS.len() {
        return UNITS[value].to_owned();
    }
    let tens = TENS
        .get(value / 10 - 2)
        .unwrap_or_else(|| panic!("no English word for {value}"));
    match value % 10 {
        0 => (*tens).to_owned(),
        unit => format!("{tens}-{}", UNITS[unit]),
    }
}

/// One text with its runs of whitespace collapsed, so a claim can be searched
/// for without the line wrapping of the file it lives in being part of the
/// assertion.
fn unwrapped(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The VS Code language identifiers the extension attaches to, taken from the
/// default of its `ocomment.languages` setting.
fn vscode_language_identifiers() -> Vec<String> {
    let manifest: Value = serde_json::from_str(VSCODE_PACKAGE).expect("package.json parses");
    manifest["contributes"]["configuration"]["properties"]["ocomment.languages"]["default"]
        .as_array()
        .expect("`ocomment.languages` has an array default")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("a language identifier is a string")
                .to_owned()
        })
        .collect()
}

/// The extension activates on exactly the identifiers it attaches the server
/// to, in the same order. The two lists sit in one file and are read by
/// different parts of VS Code, so an identifier added to one alone is an
/// extension that either never wakes up for a language or wakes up for one it
/// then ignores.
#[test]
fn the_vscode_activation_events_are_the_languages_it_attaches_to() {
    let manifest: Value = serde_json::from_str(VSCODE_PACKAGE).expect("package.json parses");
    let activated: Vec<String> = manifest["activationEvents"]
        .as_array()
        .expect("`activationEvents` is an array")
        .iter()
        .filter_map(|value| value.as_str()?.strip_prefix("onLanguage:"))
        .map(ToOwned::to_owned)
        .collect();
    assert_eq!(
        activated,
        vscode_language_identifiers(),
        "editors/vscode/package.json activates on a different set of languages than it attaches to"
    );
}

/// Every written-out count of languages or of editor language identifiers is
/// the count it claims to be. `Language::ALL` and the extension's own selector
/// are the two things being counted, so adding a language cannot leave a
/// sentence, a Marketplace description, or a changelog entry quietly wrong.
///
/// The claims are searched for in the file with its line wrapping collapsed,
/// so re-flowing a paragraph is not a failure and changing what it says is.
#[test]
fn every_written_language_count_matches_what_it_counts() {
    let languages = Language::ALL.len();
    let dialects = Dialect::ALL.len();
    let identifiers = vscode_language_identifiers().len();
    let claims = [
        (
            "docs/comparison.md",
            COMPARISON,
            format!("[{languages} languages and {dialects} dialects](languages.md)"),
        ),
        (
            "editors/vscode/package.json",
            VSCODE_PACKAGE,
            format!("comment checker and remover for {languages} languages."),
        ),
        (
            "editors/vscode/README.md",
            VSCODE_README,
            format!("the {identifiers} identifiers above"),
        ),
        (
            "docs/editors.md",
            EDITORS,
            format!(
                "It attaches to {} language identifiers",
                number_word(identifiers)
            ),
        ),
        (
            "editors/vscode/CHANGELOG.md",
            VSCODE_CHANGELOG,
            format!(
                "attaches it to the {} language identifiers OComment scans",
                number_word(identifiers)
            ),
        ),
        (
            "CHANGELOG.md",
            CHANGELOG,
            format!(
                "attaches it to the {} language identifiers OComment scans",
                number_word(identifiers)
            ),
        ),
        (
            "CHANGELOG.md",
            CHANGELOG,
            format!("transformations for {languages} built-in languages"),
        ),
    ];
    for (name, text, claim) in claims {
        assert!(
            unwrapped(text).contains(&claim),
            "{name} does not say `{claim}`; \
             {languages} language(s) and {identifiers} editor language identifier(s) are what \
             `Language::ALL` and editors/vscode/package.json hold"
        );
    }
}
