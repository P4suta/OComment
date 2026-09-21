//! What tags this tree actually uses, against the ones it says it allows.
//!
//! `[policy.allow] tags` is a convention, and a convention drifts in two directions at once.
//! A tag nobody writes any more is a line of configuration that protects nothing and reads like a rule; a tag people write that nobody configured is a comment the run is removing today, which is usually the first anybody hears of it.
//!
//! Both are reported, because a report that only looked one way would be the same half-a-gate this repository keeps meeting.

use crate::{
    files::SourceFile,
    output::{self, OutputFormat, wrote},
};
use anyhow::Result;
use ocomment_core::{AllowRules, CommentKind, ScanReport};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Write;

/// How many comments open with each tag, and which of those a configuration names.
#[derive(Debug, Default)]
pub struct Inventory {
    /// Every tag met, with how many comments opened with it.
    pub counts: BTreeMap<String, usize>,
    /// The tags the configuration allows, whether or not anything uses them.
    pub configured: Vec<String>,
}

impl Inventory {
    pub fn new(rules: &AllowRules) -> Self {
        Self {
            counts: BTreeMap::new(),
            configured: rules
                .every_tag()
                .into_iter()
                .map(str::to_ascii_uppercase)
                .collect(),
        }
    }

    /// Count the tags one file's comments open with.
    pub fn absorb(&mut self, file: &SourceFile, report: &ScanReport) {
        for comment in &report.comments {
            if !is_commentary(comment.kind) {
                continue;
            }
            let start = comment.span.start.min(file.source.len());
            let end = comment.span.end.clamp(start, file.source.len());
            if let Some(tag) = opening_tag(&file.source[start..end]) {
                *self.counts.entry(tag).or_default() += 1;
            }
        }
    }

    /// Tags the configuration allows that nothing here writes.
    pub fn unused(&self) -> Vec<&str> {
        self.configured
            .iter()
            .filter(|tag| !self.counts.contains_key(*tag))
            .map(String::as_str)
            .collect()
    }

    /// Tags this tree writes that the configuration does not allow.
    pub fn unconfigured(&self) -> Vec<(&str, usize)> {
        self.counts
            .iter()
            .filter(|(tag, _)| !self.configured.contains(tag))
            .map(|(tag, count)| (tag.as_str(), *count))
            .collect()
    }
}

/// Whether a comment of this kind is the sort a tag convention is about.
///
/// Commentary, and nothing else.
/// A documentation comment opens with the sentence the API documentation begins with and a licence notice opens with `SPDX`; reading either as a tag would report a convention nobody wrote.
const fn is_commentary(kind: CommentKind) -> bool {
    matches!(
        kind,
        CommentKind::Line | CommentKind::Block | CommentKind::HtmlComment
    )
}

/// The tag a comment opens with, if it opens with one.
///
/// A tag is an upper-case word of at least two characters at the start of the comment's text, followed by punctuation, space, or nothing — the same shape `[policy.allow] tags` matches, asked without a list to match against.
fn opening_tag(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(ocomment_core::comment_text(raw));
    let body = text.trim_start_matches(|character: char| {
        character.is_whitespace() || matches!(character, '*' | '!' | '-' | '/' | '#')
    });
    let word: String = body
        .chars()
        .take_while(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || *character == '_'
        })
        .collect();
    if word.len() < 2 || !word.starts_with(|character: char| character.is_ascii_uppercase()) {
        return None;
    }
    let rest = &body[word.len()..];
    rest.chars()
        .next()
        .is_none_or(|next| !next.is_alphanumeric())
        .then_some(word)
}

pub fn render(inventory: &Inventory, format: OutputFormat) -> Result<()> {
    let mut stdout = output::stdout();
    let unused = inventory.unused();
    let unconfigured = inventory.unconfigured();
    if matches!(format, OutputFormat::Json | OutputFormat::Jsonl) {
        let document = json!({
            "version": 1,
            "counts": inventory.counts,
            "configured": inventory.configured,
            "unused": unused,
            "unconfigured": unconfigured
                .iter()
                .map(|(tag, count)| json!({ "tag": tag, "count": count }))
                .collect::<Vec<_>>(),
        });
        let rendered = serde_json::to_string_pretty(&document)?;
        wrote(writeln!(stdout, "{rendered}"))?;
        return output::finish(&mut stdout);
    }
    for (tag, count) in &inventory.counts {
        let mark = if inventory.configured.contains(tag) {
            " "
        } else {
            /* NOTE: A tag nothing allows is a comment this run removes today,
             * so it is marked on its own line rather than only in the sentence underneath: the listing is what a reader scans. */
            "!"
        };
        wrote(writeln!(stdout, "{mark} {tag}\t{count}"))?;
    }
    if inventory.counts.is_empty() {
        wrote(writeln!(stdout, "No tagged comments."))?;
    }
    if !unused.is_empty() {
        wrote(writeln!(
            stdout,
            "\nAllowed and never written: {}.",
            unused.join(", ")
        ))?;
    }
    if !unconfigured.is_empty() {
        wrote(writeln!(
            stdout,
            "\nWritten and not allowed: {}. These are comments this run removes \
             today; add one to `[policy.allow] tags` to keep it, or to \
             `[policy.allow.expiry]` to keep it for a while.",
            unconfigured
                .iter()
                .map(|(tag, count)| format!("{tag} ({count})"))
                .collect::<Vec<_>>()
                .join(", ")
        ))?;
    }
    output::finish(&mut stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_an_upper_case_word_at_the_start() {
        assert_eq!(opening_tag(b"// NOTE: a reason").as_deref(), Some("NOTE"));
        assert_eq!(opening_tag(b"/* TODO(alice) */").as_deref(), Some("TODO"));
        assert_eq!(opening_tag(b"# XXX").as_deref(), Some("XXX"));
        assert_eq!(opening_tag(b"-- FIXME - later").as_deref(), Some("FIXME"));
    }

    #[test]
    fn prose_is_not_a_tag() {
        assert_eq!(opening_tag(b"// A remark"), None);
        assert_eq!(opening_tag(b"// nothing here"), None);
        assert_eq!(opening_tag(b"//"), None);
    }

    /// A different question from the one `[policy.allow] tags` asks, and the difference matters.
    ///
    /// The rule asks *does this carry the tag `NOTE`?*, and `NOTEBOOK` does not.
    /// This asks *what word does this open with?*, and the answer is `NOTEBOOK` — which is what the reader needs, because that comment is one the run removes today and the listing is where they would find out.
    #[test]
    fn the_inventory_reads_the_word_rather_than_matching_a_list() {
        assert_eq!(
            opening_tag(b"// NOTEBOOK entry").as_deref(),
            Some("NOTEBOOK")
        );
    }
}
