//! What a run did not look at.
//!
//! `check` reports the comments it found and mentions the files it skipped in
//! a clause at the end of the summary. That clause is a note; the number in it
//! is not. A repository whose gate says "no removable comments in 143 files"
//! while 25 files were never opened has a gate over 85% of itself, and nothing
//! in that sentence says so.
//!
//! This is the other half of the report: every file the walk reached, grouped
//! by whether it was scanned and by why it was not, with the extensions that
//! account for the gaps named in the order they matter. It exists to be acted
//! on — the answer to "unknown language: 9 `.json`" is a decision about
//! `.json`, and the decision cannot be made by someone who does not know the
//! nine are there.

use crate::{
    files::{NotWalked, SkippedFile, SourceFile},
    output::{OutputFormat, ReadBy, skip_label, stdout, wrote},
};
use anyhow::Result;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

/// How many extensions a human listing names per reason before it stops.
///
/// The list is there to be acted on and a reader acts on the common ones
/// first, so the tail is summarised rather than printed. `--format json`
/// carries all of them, because a tool reading it is not the one getting
/// tired.
const TOP_EXTENSIONS: usize = 8;

/// The files of one run, split by what happened to them.
#[derive(Default)]
pub struct Coverage {
    /// What was scanned, split by which reader answered for it.
    ///
    /// A single count said how many files were read and could not say what
    /// read them. That is the one thing this report cannot leave out: when a
    /// release teaches the tool a format it used to pass over, the files it
    /// newly reads move from a skip reason into this total, and a reader
    /// looking at the number alone sees a repository that grew. Keyed by
    /// [`ReadBy`], whose ordering puts the built-in languages before the
    /// profiles and plugins that were added to reach past them.
    scanned: BTreeMap<ReadBy, Group>,
    /// Reason label to how many files it accounts for, and for which
    /// extensions.
    skipped: BTreeMap<String, Group>,
    /// The files the walk's own limits kept out, which nothing met and so
    /// nothing reported. Counted in the total, because a percentage of what a
    /// gate happened to walk is not a percentage of anything.
    not_walked: BTreeMap<String, Group>,
    io_errors: usize,
}

#[derive(Default)]
struct Group {
    files: usize,
    extensions: BTreeMap<String, usize>,
}

/// How a file is named in the listing: its extension, or its whole file name
/// when it has none.
///
/// A `dune` file and a `CODEOWNERS` file have no extension and are exactly the
/// kind of file this listing exists to surface, so falling back to the name is
/// what keeps them from collapsing into one anonymous bucket.
fn label_of(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .map_or_else(
            || {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("<unnamed>")
                    .to_owned()
            },
            |extension| format!(".{extension}"),
        )
}

impl Coverage {
    /// Split one run's discovery into what it covered and what it did not.
    pub fn compute(
        files: &[SourceFile],
        skipped: &[SkippedFile],
        not_walked: &[(PathBuf, NotWalked)],
    ) -> Self {
        let mut coverage = Self::default();
        for file in files {
            let group = coverage.scanned.entry(file.read_by()).or_default();
            group.files += 1;
            *group.extensions.entry(label_of(&file.path)).or_default() += 1;
        }
        for item in skipped {
            if item.error {
                coverage.io_errors += 1;
                continue;
            }
            let group = coverage
                .skipped
                .entry(skip_label(&item.reason).to_owned())
                .or_default();
            group.files += 1;
            *group.extensions.entry(label_of(&item.path)).or_default() += 1;
        }
        for (path, reason) in not_walked {
            let group = coverage
                .not_walked
                .entry(reason.reason().to_owned())
                .or_default();
            group.files += 1;
            *group.extensions.entry(label_of(path)).or_default() += 1;
        }
        coverage
    }

    /// Every file in the tree: the ones the walk reached, and the ones its
    /// own limits kept out.
    ///
    /// The second half is what makes the number mean something. Counting only
    /// what the walk reached gives a percentage of the walk rather than of the
    /// repository, and a run that walked three of seven files then reports
    /// `100.0%` -- which is true and is a false assurance.
    fn total(&self) -> usize {
        self.scanned()
            + self.io_errors
            + self
                .skipped
                .values()
                .chain(self.not_walked.values())
                .map(|group| group.files)
                .sum::<usize>()
    }

    /// The share that was scanned, in tenths of a percent, or 1000 when there
    /// was nothing to scan.
    ///
    /// An empty walk covers everything it was given, which is not a useful
    /// number but is the only honest one; reporting 0% for it would read as a
    /// failure that has not happened.
    ///
    /// Tenths rather than a float because the only thing this number is for is
    /// being printed to one decimal place, and integer arithmetic gets there
    /// without a lossy conversion to apologise for. The machine format leaves
    /// it out entirely: it carries `scanned` and `files`, and a caller that
    /// wants a ratio can divide two exact numbers rather than parse a rounded
    /// one.
    fn tenths_of_a_percent(&self) -> usize {
        let total = self.total();
        if total == 0 {
            return 1000;
        }
        self.scanned() * 1000 / total
    }

    /// How many files were read, whichever reader read them.
    fn scanned(&self) -> usize {
        self.scanned.values().map(|group| group.files).sum()
    }

    /// How a group of files is named by what read them.
    ///
    /// The built-in arm names no language, unlike the per-file label: this
    /// listing groups every built-in scanner together, because the question it
    /// answers is which files needed something beyond them. The extensions
    /// under each line say which files those were.
    fn reader_label(read_by: &ReadBy) -> String {
        match *read_by {
            ReadBy::Language => "read by a built-in language".to_owned(),
            ReadBy::Profile(ref name) => format!("read by the `{name}` profile"),
            ReadBy::Plugin(ref name) => format!("read by the `{name}` plugin"),
        }
    }
}

/// List the kinds of file one group accounts for, most first.
fn names_of(out: &mut impl Write, group: &Group) -> Result<()> {
    let mut names: Vec<_> = group.extensions.iter().collect();
    /* NOTE: Most files first, and the name as the tie-break so that two runs
     * over the same tree print the same listing. */
    names.sort_by(|left, right| right.1.cmp(left.1).then(left.0.cmp(right.0)));
    for (name, count) in names.iter().take(TOP_EXTENSIONS) {
        wrote(writeln!(out, "    {count:>4}  {name}"))?;
    }
    if names.len() > TOP_EXTENSIONS {
        wrote(writeln!(
            out,
            "    {:>4}  more kinds of file",
            names.len() - TOP_EXTENSIONS
        ))?;
    }
    Ok(())
}

/// Write the coverage of one run in the shape the format asks for.
pub fn render(coverage: &Coverage, format: OutputFormat) -> Result<()> {
    let mut out = stdout();
    match format {
        OutputFormat::Json | OutputFormat::Jsonl => {
            let document = json!({
                "version": 1,
                "files": coverage.total(),
                "scanned": coverage.scanned(),
                "read_by": coverage
                    .scanned
                    .iter()
                    .map(|(read_by, group)| {
                        /* NOTE: `kind` without a `name` is the built-in group,
                         * which spans every language the walk met and so has no
                         * single one to name. Naming one anyway -- `unknown`
                         * being the only candidate -- would be the same false
                         * answer this field exists to stop giving. */
                        let (kind, name) = match *read_by {
                            ReadBy::Language => ("language", None),
                            ReadBy::Profile(ref name) => ("profile", Some(name)),
                            ReadBy::Plugin(ref name) => ("plugin", Some(name)),
                        };
                        let mut entry = json!({
                            "kind": kind,
                            "files": group.files,
                            "names": group
                                .extensions
                                .iter()
                                .map(|(name, count)| json!({"name": name, "files": count}))
                                .collect::<Vec<_>>(),
                        });
                        if let Some(name) = name {
                            entry["name"] = json!(name);
                        }
                        entry
                    })
                    .collect::<Vec<_>>(),
                "io_errors": coverage.io_errors,
                "not_walked": coverage
                    .not_walked
                    .iter()
                    .map(|(reason, group)| {
                        json!({
                            "reason": reason,
                            "files": group.files,
                            "names": group
                                .extensions
                                .iter()
                                .map(|(name, count)| json!({"name": name, "files": count}))
                                .collect::<Vec<_>>(),
                        })
                    })
                    .collect::<Vec<_>>(),
                "skipped": coverage
                    .skipped
                    .iter()
                    .map(|(reason, group)| {
                        json!({
                            "reason": reason,
                            "files": group.files,
                            "names": group
                                .extensions
                                .iter()
                                .map(|(name, count)| json!({"name": name, "files": count}))
                                .collect::<Vec<_>>(),
                        })
                    })
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
            let tenths = coverage.tenths_of_a_percent();
            wrote(writeln!(
                out,
                "{} of {} files scanned ({}.{}%)",
                coverage.scanned(),
                coverage.total(),
                tenths / 10,
                tenths % 10
            ))?;
            /* NOTE: Only when more than one reader answered. A run every
             * built-in language read is a run where naming the reader adds a
             * line and no information; the listing is here for the run where a
             * profile or a plugin read files nothing else would have, which is
             * exactly the run whose coverage just changed under its reader. */
            if coverage.scanned.len() > 1 {
                for (read_by, group) in &coverage.scanned {
                    wrote(writeln!(
                        out,
                        "{}: {}",
                        group.files,
                        Coverage::reader_label(read_by)
                    ))?;
                    names_of(&mut out, group)?;
                }
            }
            for (reason, group) in coverage.skipped.iter().chain(&coverage.not_walked) {
                wrote(writeln!(out, "{}: {reason}", group.files))?;
                names_of(&mut out, group)?;
            }
            if coverage.io_errors > 0 {
                wrote(writeln!(out, "{}: could not be read", coverage.io_errors))?;
            }
        }
    }
    crate::output::finish(&mut out)
}

/// The skip reasons a run refuses to pass over, when it was told to refuse.
///
/// Not every skip is a hole. A binary file has no comments to miss and a file
/// the configuration excluded was excluded on purpose; both are decisions
/// already taken. An unreadable file and a file in a language nothing here
/// knows are different: those are files the gate was meant to cover and did
/// not, and a run that is supposed to be a gate should be able to say so with
/// its exit status rather than in a note.
pub fn denied(skipped: &[SkippedFile], reasons: &[String]) -> Vec<PathBuf> {
    skipped
        .iter()
        .filter(|item| {
            let label = if item.error {
                "unreadable"
            } else {
                skip_label(&item.reason)
            };
            reasons.iter().any(|wanted| wanted == label)
        })
        .map(|item| item.path.clone())
        .collect()
}
