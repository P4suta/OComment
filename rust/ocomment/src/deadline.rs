//! Deadlines on the tags that are promises.
//!
//! `[policy.allow] tags` keeps a comment for the tag it opens with, and that
//! is the right rule for a `SAFETY` — it records why the code is the way it
//! is, and it is true for as long as the code is. It is the wrong rule for a
//! `TODO`, which says somebody will do something. Keeping one forever is how a
//! repository ends up with a `TODO` from four years ago that everybody has
//! learned to read past; forbidding one outright loses the note along with the
//! nagging, and nobody obeys it anyway.
//!
//! `[policy.allow.expiry]` is the third answer. Write the `TODO`, commit it,
//! and it is fine — for a fortnight. After that it is a finding, with the
//! reason spelled out and the age counted, every run, until somebody either
//! does it or deletes it.
//!
//! The clock is the repository's: the age of a line is the age of the commit
//! that introduced it, read from `git blame`. That is why this lives here and
//! not in `ocomment-core`, which performs no I/O. The core owns the vocabulary
//! — [`ShapeRule::Expired`] — so a verdict reached here is reported through
//! the same channel every other verdict is.

use anyhow::Result;
use ocomment_core::{Age, AllowRules, ScanReport, ShapeRule};
use std::{
    collections::HashMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

/// What a run found overdue, counted for the note it owes the reader.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Overdue {
    /// How many comments were taken back, per tag.
    pub by_tag: std::collections::BTreeMap<String, usize>,
}

impl Overdue {
    pub fn total(&self) -> usize {
        self.by_tag.values().sum()
    }

    pub fn absorb(&mut self, other: &Self) {
        for (tag, count) in &other.by_tag {
            *self.by_tag.entry(tag.clone()).or_default() += count;
        }
    }

    /// The sentence a run writes about them, or `None` when there were none.
    ///
    /// Deliberately not phrased as a summary. A deadline that passed is not a
    /// statistic about the run; it is a thing somebody said they would do.
    pub fn note(&self) -> Option<String> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        let tags: Vec<String> = self
            .by_tag
            .iter()
            .map(|(tag, count)| format!("{count} {tag}"))
            .collect();
        Some(format!(
            "{total} comment{} past {} deadline: {}. Do {} or delete {}.",
            if total == 1 { "" } else { "s" },
            if total == 1 { "its" } else { "their" },
            tags.join(", "),
            if total == 1 { "it" } else { "them" },
            if total == 1 { "it" } else { "them" },
        ))
    }
}

/// Take back the keeps whose deadline has passed.
///
/// `source` is the exact content `report` describes, which need not be what is
/// on the disk: a staged run judges an index blob and an editing hook judges
/// an edit that has not happened yet, and `git blame --contents` answers for
/// either. A line those bytes introduced is attributed to no commit and has
/// therefore not started its deadline — which is the whole of "writing one
/// costs nothing".
///
/// Nothing is measured, and no process is started, unless a comment this file
/// actually holds carries a tag the configuration gave a deadline to.
pub fn apply(
    root: &Path,
    path: &Path,
    source: &[u8],
    report: &mut ScanReport,
    rules: &AllowRules,
    now: SystemTime,
) -> Result<Overdue> {
    let mut overdue = Overdue::default();
    if rules.expiry.is_empty() {
        return Ok(overdue);
    }
    let candidates: Vec<usize> = report
        .comments
        .iter()
        .enumerate()
        .filter(|(_, comment)| match &comment.shape {
            Some(ShapeRule::Tagged { tag }) => rules.expiry.contains_key(tag),
            _ => false,
        })
        .map(|(index, _)| index)
        .collect();
    if candidates.is_empty() {
        return Ok(overdue);
    }
    /* NOTE: No repository, no git, an untracked file: all of them mean the age
     * cannot be read, and a deadline nobody can measure has not passed. The
     * comment keeps the benefit of the doubt. */
    let Some(ages) = line_ages(root, path, source, now) else {
        return Ok(overdue);
    };
    let lines = LineIndex::new(source);
    for index in candidates {
        let comment = &mut report.comments[index];
        let Some(ShapeRule::Tagged { tag }) = &comment.shape else {
            continue;
        };
        let limit = rules.expiry[tag];
        let Some(age) = ages.get(&lines.line_of(comment.span.start)).copied() else {
            continue;
        };
        if age <= limit {
            continue;
        }
        let tag = tag.clone();
        *overdue.by_tag.entry(tag.clone()).or_default() += 1;
        let rule = ShapeRule::Expired { tag, age, limit };
        comment.disposition = rule.disposition();
        comment.shape = Some(rule);
    }
    Ok(overdue)
}

/// The age of every line of `source`, by 1-based line number.
///
/// `None` means the question could not be asked. A line attributed to no
/// commit — one these bytes introduce — is absent from the map rather than
/// recorded as new, so a caller that finds nothing leaves the comment alone.
fn line_ages(
    root: &Path,
    path: &Path,
    source: &[u8],
    now: SystemTime,
) -> Option<HashMap<usize, Age>> {
    let blame = blame(root, path, source)?;
    let now = i64::try_from(now.duration_since(UNIX_EPOCH).ok()?.as_secs()).ok()?;
    /* NOTE: Read in one pass and joined afterwards, because the porcelain form
     * announces a commit's date only the first time that commit is seen and
     * repeats the bare header for every group after it. Neither half can wait
     * for the other in a single sweep. */
    let mut times: HashMap<&str, i64> = HashMap::new();
    let mut groups: Vec<(&str, usize, usize)> = Vec::new();
    for line in blame.lines() {
        if let Some(header) = blame_header(line) {
            groups.push(header);
        } else if let Some(rest) = line.strip_prefix("committer-time ")
            && let Some((sha, _, _)) = groups.last()
            && let Ok(seconds) = rest.trim().parse::<i64>()
        {
            times.insert(sha, seconds);
        }
    }
    let mut ages = HashMap::new();
    for (sha, first, count) in groups {
        if let Some(seconds) = times.get(sha).copied() {
            record(&mut ages, first, count, seconds, now);
        }
    }
    Some(ages)
}

/// A porcelain group header: `<sha> <original line> <final line> [<count>]`.
///
/// Every other line of the form is a key and a value, so a first field of
/// exactly forty hex digits is what separates the two. The count is written
/// only the first time a group is announced, and one line is the default.
fn blame_header(line: &str) -> Option<(&str, usize, usize)> {
    let mut fields = line.split(' ');
    let sha = fields.next()?;
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let first = fields.nth(1)?.parse().ok()?;
    let count = fields
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    Some((sha, first, count))
}

/// Record one blame group's age against each line it covers.
///
/// A commit dated in the future — a clock that disagrees, a rebase — is
/// recorded as no age at all rather than as a negative one, so it cannot make
/// a deadline pass early or, worse, never.
fn record(ages: &mut HashMap<usize, Age>, first: usize, count: usize, seconds: i64, now: i64) {
    let days = u32::try_from((now - seconds).max(0) / 86_400).unwrap_or(u32::MAX);
    for line in first..first.saturating_add(count) {
        ages.insert(line, Age::from_days(days));
    }
}

/// `git blame --porcelain` over `source`, judged against the history of
/// `path`.
fn blame(root: &Path, path: &Path, source: &[u8]) -> Option<String> {
    let mut child = Command::new("git")
        .current_dir(root)
        .args(["blame", "--porcelain", "--contents", "-", "--"])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(source).ok()?;
    let output = child.wait_with_output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Line numbers for byte offsets, 1-based.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &[u8]) -> Self {
        Self {
            starts: source
                .iter()
                .enumerate()
                .filter(|(_, byte)| **byte == b'\n')
                .map(|(index, _)| index)
                .collect(),
        }
    }

    fn line_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|start| *start < offset) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_index_counts_from_one() {
        let index = LineIndex::new(b"a\nbb\nccc\n");
        assert_eq!(index.line_of(0), 1);
        assert_eq!(index.line_of(2), 2);
        assert_eq!(index.line_of(5), 3);
    }

    #[test]
    fn a_commit_dated_in_the_future_is_no_age_rather_than_a_negative_one() {
        let mut ages = HashMap::new();
        record(&mut ages, 1, 2, 2_000, 1_000);
        assert_eq!(ages[&1], Age::ZERO);
        assert_eq!(ages[&2], Age::ZERO);
    }

    #[test]
    fn a_note_names_the_tags_rather_than_counting_findings() {
        let mut overdue = Overdue::default();
        overdue.by_tag.insert("TODO".into(), 2);
        overdue.by_tag.insert("FIXME".into(), 1);
        assert_eq!(
            overdue.note().as_deref(),
            Some("3 comments past their deadline: 1 FIXME, 2 TODO. Do them or delete them.")
        );
        assert_eq!(Overdue::default().note(), None);
    }
}
