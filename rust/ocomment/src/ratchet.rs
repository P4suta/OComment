//! A count that may only fall.
//!
//! A project with eleven thousand comments and a rule it wants to reach has two
//! bad options: turn the rule on and fail every commit, or leave it off and
//! never arrive. A ledger is the third. It records what each file holds today,
//! fails when a file holds more than that, and fails again when a file holds
//! fewer — because a ledger that only notices one direction eventually
//! describes a repository that no longer exists.
//!
//! That second failure is the one that makes this different from a baseline
//! file. A baseline forgives what it recorded and says nothing when the work is
//! done; a ledger asks to be updated, so the number in the file is always the
//! number in the tree, and the distance left to go is readable at a glance.
//!
//! It is deliberately not a suppression mechanism. The entries carry no
//! reasons, no expiry dates and no per-comment granularity: a ledger is a
//! measurement, and the moment it starts explaining itself it has become a
//! second configuration file arguing with the first.

use crate::output::{Detail, OutputFormat, ProcessedFile, Verbosity, note, stdout, wrote};
use anyhow::{Context, Result};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    io::Write,
    path::{Path, PathBuf},
};

/// What a run found, per file, in the order a ledger stores it.
pub type Counts = BTreeMap<String, usize>;

/// How a tree differs from the ledger recorded beside it.
#[derive(Debug, Default)]
pub struct Drift {
    /// Files holding more than the ledger allows, with both numbers.
    grew: Vec<(String, usize, usize)>,
    /// Files holding fewer, which is progress the ledger has not been told
    /// about.
    shrank: Vec<(String, usize, usize)>,
    /// Entries naming a file the walk did not reach.
    absent: Vec<String>,
}

impl Drift {
    /// Whether the tree and the ledger agree.
    pub fn is_empty(&self) -> bool {
        self.grew.is_empty() && self.shrank.is_empty() && self.absent.is_empty()
    }
}

/// Count the removable comments of a run, per file, under the path the report
/// uses.
///
/// Files with none are absent rather than zero: a ledger of zeroes would grow
/// with every file added to a clean repository and say nothing.
pub fn count(files: &[ProcessedFile], root: &Path) -> Counts {
    let mut counts = Counts::new();
    for file in files {
        let removable = file
            .result
            .report
            .comments
            .iter()
            .filter(|comment| comment.disposition.is_remove())
            .count();
        if removable == 0 {
            continue;
        }
        let key = file
            .path
            .strip_prefix(root)
            .unwrap_or(&file.path)
            .to_string_lossy()
            .replace('\\', "/");
        counts.insert(key, removable);
    }
    counts
}

/// Read a ledger, or an empty one when the file does not exist.
///
/// A missing ledger is not an error: `ocomment ratchet update` is how the first
/// one is written, and a run before that has nothing to be held to.
pub fn read(path: &Path) -> Result<Counts> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Counts::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", path.display()));
        }
    };
    let mut counts = Counts::new();
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // NOTE: The count first, so that a path may hold spaces.
        let (count, file) = trimmed.split_once(' ').with_context(|| {
            format!(
                "{}:{}: expected `<count> <path>`, got {trimmed:?}",
                path.display(),
                number + 1
            )
        })?;
        let count: usize = count.parse().with_context(|| {
            format!(
                "{}:{}: {count:?} is not a count",
                path.display(),
                number + 1
            )
        })?;
        counts.insert(file.trim().to_owned(), count);
    }
    Ok(counts)
}

/// The bytes a ledger holds for these counts.
pub fn render(counts: &Counts) -> String {
    let total: usize = counts.values().sum();
    let mut text = String::new();
    let _ = writeln!(
        text,
        "# Written by `ocomment ratchet update`. Each line is a count and a path,\n\
         # and a count may only fall: a file holding more fails the gate, and a\n\
         # file holding fewer fails it too, asking for this file to be updated.\n\
         #\n\
         # {total} comment(s) in {} file(s) left to remove.",
        counts.len()
    );
    for (file, count) in counts {
        let _ = writeln!(text, "{count} {file}");
    }
    text
}

/// How a tree differs from its ledger.
pub fn compare(recorded: &Counts, found: &Counts) -> Drift {
    let mut drift = Drift::default();
    for (file, allowed) in recorded {
        match found.get(file) {
            Some(count) if count > allowed => {
                drift.grew.push((file.clone(), *count, *allowed));
            }
            Some(count) if count < allowed => {
                drift.shrank.push((file.clone(), *count, *allowed));
            }
            Some(_) => {}
            None => drift.absent.push(file.clone()),
        }
    }
    for (file, count) in found {
        if !recorded.contains_key(file) {
            drift.grew.push((file.clone(), *count, 0));
        }
    }
    drift
}

/// Report a drift and say what to do about it.
pub fn report(
    drift: &Drift,
    ledger: &Path,
    format: OutputFormat,
    verbosity: Verbosity,
) -> Result<()> {
    let mut out = stdout();
    if matches!(format, OutputFormat::Json | OutputFormat::Jsonl) {
        let document = json!({
            "version": 1,
            "ledger": ledger.to_string_lossy(),
            "grew": drift.grew.iter()
                .map(|(file, found, allowed)| json!({"path": file, "found": found, "allowed": allowed}))
                .collect::<Vec<_>>(),
            "shrank": drift.shrank.iter()
                .map(|(file, found, allowed)| json!({"path": file, "found": found, "allowed": allowed}))
                .collect::<Vec<_>>(),
            "absent": drift.absent,
        });
        wrote(writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&document).expect("the report serializes")
        ))?;
        return crate::output::finish(&mut out);
    }

    for (file, found, allowed) in &drift.grew {
        wrote(writeln!(
            out,
            "{file}: {found} removable, and the ledger allows {allowed}"
        ))?;
    }
    for (file, found, allowed) in &drift.shrank {
        wrote(writeln!(
            out,
            "{file}: {found} removable, and the ledger still says {allowed}"
        ))?;
    }
    for file in &drift.absent {
        wrote(writeln!(out, "{file}: in the ledger, and not in the tree"))?;
    }
    crate::output::finish(&mut out)?;

    let stderr = std::io::stderr();
    let mut summary = stderr.lock();
    if drift.is_empty() {
        note(
            &mut summary,
            verbosity,
            Detail::Normal,
            "The tree matches its ledger.",
        )?;
        return Ok(());
    }
    // NOTE: Two sentences: growth is a gate failing, shrinkage is work to record.
    if !drift.grew.is_empty() {
        note(
            &mut summary,
            verbosity,
            Detail::Normal,
            &format!(
                "{} file(s) hold more than the ledger allows.",
                drift.grew.len()
            ),
        )?;
    }
    if !drift.shrank.is_empty() || !drift.absent.is_empty() {
        note(
            &mut summary,
            verbosity,
            Detail::Normal,
            &format!(
                "{} entr(ies) are out of date; run `ocomment ratchet update` to record the progress.",
                drift.shrank.len() + drift.absent.len()
            ),
        )?;
    }
    Ok(())
}

/// Write a ledger, replacing whatever was there.
pub fn write(path: &Path, counts: &Counts, verbosity: Verbosity) -> Result<()> {
    std::fs::write(path, render(counts))
        .with_context(|| format!("cannot write {}", path.display()))?;
    let stderr = std::io::stderr();
    let mut summary = stderr.lock();
    let total: usize = counts.values().sum();
    note(
        &mut summary,
        verbosity,
        Detail::Normal,
        &format!(
            "Recorded {total} comment(s) in {} file(s) in {}.",
            counts.len(),
            crate::output::sanitize_path(&path.to_string_lossy())
        ),
    )
}

/// Where a ledger lives, relative to the project root.
pub fn path(root: &Path, configured: &str) -> PathBuf {
    root.join(configured)
}
