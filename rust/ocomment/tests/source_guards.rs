//! Guards that read this crate's own source text.
//!
//! A test that runs the binary can only catch a bypass on the paths it
//! happens to exercise. These read the sources instead, so an invariant that
//! holds today cannot be broken quietly by a line added tomorrow.

use std::{collections::BTreeSet, fs, path::PathBuf};

/// Every source file of the crate, embedded at compile time so the scan does
/// not depend on the directory the test runs in. `the_guard_reads_every_source`
/// keeps this list equal to what is on disk.
const SOURCES: [(&str, &str); 11] = [
    ("atomic.rs", include_str!("../src/atomic.rs")),
    ("cli.rs", include_str!("../src/cli.rs")),
    ("config.rs", include_str!("../src/config.rs")),
    ("files.rs", include_str!("../src/files.rs")),
    ("git.rs", include_str!("../src/git.rs")),
    ("interactive.rs", include_str!("../src/interactive.rs")),
    ("lsp.rs", include_str!("../src/lsp.rs")),
    ("main.rs", include_str!("../src/main.rs")),
    ("output.rs", include_str!("../src/output.rs")),
    ("plugin.rs", include_str!("../src/plugin.rs")),
    ("values.rs", include_str!("../src/values.rs")),
];

/// The names this crate gives a handle on the program's standard output: the
/// locked writer `output::stdout()` returns is bound as `stdout`, and every
/// function that is handed it takes it as `output`. Nothing else in the crate
/// is written to under either name.
const STDOUT_HANDLES: [&str; 2] = ["stdout", "output"];

/// The write macros, matched with their opening parenthesis so the target is
/// the text that follows.
const MACROS: [&str; 2] = ["write!(", "writeln!("];

/// The method form of the same write.
const METHOD: &str = ".write_all(";

/// The call every write to standard output is raised through.
const WRAPPER: &str = "wrote(";

/// One write whose target is a standard-output handle.
struct StdoutWrite {
    line: usize,
    /// The call sits directly inside `wrote(` — or `output::wrote(`.
    wrapped: bool,
}

/// Every write to the program's own standard output is raised through
/// [`output::wrote`], which tags a lost reader as `OutputPipeClosed` so `main`
/// can end quietly for that case and only that case. A raw `writeln!` would
/// return a bare `BrokenPipe` that the chain cannot tell apart from a real
/// failure — `git hash-object` dropping the blob it was being handed, say —
/// and `ocomment … | head` would start failing runs, or a failed staged fix
/// would start passing.
///
/// The invariant checked here is textual: a `write!`, `writeln!`, or
/// `write_all` whose target names a standard-output handle must have `wrote(`
/// immediately in front of it. It is deliberately syntactic rather than
/// semantic — it cannot know what a handle is, only what it is called — so it
/// leans on the naming convention above and on
/// `standard_output_is_locked_in_exactly_one_place`, which keeps a writer from
/// being conjured anonymously under some other name.
#[test]
fn every_write_to_standard_output_goes_through_wrote() {
    let mut wrapped = 0;
    let mut bare = Vec::new();
    for (name, source) in SOURCES {
        for call in stdout_writes(source) {
            if call.wrapped {
                wrapped += 1;
            } else {
                bare.push(format!("src/{name}:{}", call.line));
            }
        }
    }
    assert!(
        bare.is_empty(),
        "these writes to standard output bypass `output::wrote`, so a reader \
         that closed the pipe would surface as an unrecognizable I/O failure: \
         {bare:?}"
    );
    // NOTE: A scan that matches nothing would pass this test forever.
    assert!(
        wrapped >= 30,
        "the guard recognized only {wrapped} writes to standard output, far \
         fewer than the crate makes; the naming convention it reads must have \
         changed, and the guard with it"
    );
}

/// The handle can only be watched by name if it is only ever made in one
/// place. `output::stdout()` locks standard output for the whole run; nowhere
/// else may turn it into a writer, whether by locking it, writing to it, or
/// flushing it. (Naming it to ask whether it is a terminal is not writing to
/// it, and neither is handing the LSP server its own protocol channel.)
#[test]
fn standard_output_is_locked_in_exactly_one_place() {
    let mut offenders = Vec::new();
    for (name, source) in SOURCES {
        if name == "output.rs" {
            continue;
        }
        for (at, _) in source.match_indices("stdout()") {
            let rest = &source[at + "stdout()".len()..];
            if [".lock()", ".write", ".flush"]
                .iter()
                .any(|call| rest.starts_with(call))
            {
                offenders.push(format!("src/{name}:{}", line_of(source, at)));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "standard output is written through a handle made outside \
         `output::stdout`, where the pipe guard cannot see it: {offenders:?}"
    );
}

/// The embedded list is the whole crate, so a module added later is scanned
/// rather than silently exempt.
#[test]
fn the_guard_reads_every_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut on_disk = BTreeSet::new();
    let mut pending = vec![(String::new(), root)];
    while let Some((prefix, directory)) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_str().unwrap().to_owned();
            let name = format!("{prefix}{name}");
            if entry.file_type().unwrap().is_dir() {
                pending.push((format!("{name}/"), entry.path()));
            } else if name.ends_with(".rs") {
                on_disk.insert(name);
            }
        }
    }
    let scanned: BTreeSet<String> = SOURCES.iter().map(|(name, _)| (*name).to_owned()).collect();
    assert_eq!(
        scanned, on_disk,
        "the source list this file scans is not the source list on disk"
    );
}

/// Every write in `source` whose target names a standard-output handle.
fn stdout_writes(source: &str) -> Vec<StdoutWrite> {
    let mut writes = Vec::new();
    for marker in MACROS {
        for (at, _) in source.match_indices(marker) {
            let target = handle(first_argument(&source[at + marker.len()..]));
            if STDOUT_HANDLES.contains(&target) {
                writes.push(StdoutWrite {
                    line: line_of(source, at),
                    wrapped: wraps(&source[..at]),
                });
            }
        }
    }
    for (at, _) in source.match_indices(METHOD) {
        let receiver = identifier_before(source, at);
        if STDOUT_HANDLES.contains(&receiver) {
            writes.push(StdoutWrite {
                line: line_of(source, at),
                wrapped: wraps(&source[..at - receiver.len()]),
            });
        }
    }
    writes
}

/// Whether the call that follows `prefix` sits directly inside `wrote(`.
fn wraps(prefix: &str) -> bool {
    prefix.trim_end().ends_with(WRAPPER)
}

/// The 1-based line byte `at` falls on.
fn line_of(source: &str, at: usize) -> usize {
    source[..at].matches('\n').count() + 1
}

/// The first argument of a call, given everything after its opening
/// parenthesis. A target too involved to end at the first comma — a call of
/// its own, say — comes back as something no handle is named, and the write is
/// left to `standard_output_is_locked_in_exactly_one_place`.
fn first_argument(rest: &str) -> &str {
    let end = rest.find([',', ')']).unwrap_or(rest.len());
    rest[..end].trim()
}

/// A target expression reduced to the name it writes through, so that
/// `&mut output` and `output` are the same handle.
fn handle(target: &str) -> &str {
    let target = target.trim_start_matches('&').trim_start();
    target.strip_prefix("mut ").unwrap_or(target).trim()
}

/// The identifier ending at byte `at`, empty when the byte before it is not
/// part of one.
fn identifier_before(source: &str, at: usize) -> &str {
    let start = source[..at]
        .char_indices()
        .rev()
        .take_while(|(_, character)| character.is_ascii_alphanumeric() || *character == '_')
        .last()
        .map_or(at, |(index, _)| index);
    &source[start..at]
}
