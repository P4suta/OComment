//! The record of how a run reached its verdicts.
//!
//! `--explain` answers *why* for one comment: the rule that applied and the
//! setting behind it, printed beside the finding it is about. This answers a
//! different question — what the run did, in the order it did it — and it
//! answers it for the steps a finding never mentions: which evidence chose the
//! language, which files were never scanned and why, which edits were planned
//! from the comments that were found.
//!
//! It is diagnostic rather than product, so it goes to standard error and
//! leaves standard output to carry the findings, the patch, or the machine
//! report. That is what lets `--trace json` be combined with `--format json`
//! without either one having to know about the other.
//!
//! # What it does not see
//!
//! The scanner's recursion into an embedded language — an HTML `<script>` body
//! read as JavaScript, a Markdown fence read as the language its info string
//! names — happens inside `ocomment-core` and is not visible from the events a
//! scan hands back. A comment found inside one is reported at its byte span in
//! the outer file, as it is everywhere else. Surfacing the nesting would mean
//! threading a sink through the scanner, and the scanner is the half of this
//! repository that a second implementation is checked against; it is not worth
//! disturbing for a diagnostic that can be had from the outside.

use crate::{
    files::{SkippedFile, SourceFile},
    output::{Detail, Explanations, ProcessedFile, Verbosity, note},
};
use anyhow::Result;
use ocomment_core::{Comment, DispositionPatterns, explain_comment_with};
use serde::Serialize;
use std::{io::Write, path::Path};

/// When the trace is written.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TraceMode {
    /// Not written at all, and nothing is collected for it.
    #[default]
    Off,
    /// One line per event, for a person reading a terminal.
    Human,
    /// One JSON object per line, against `spec/trace.schema.json`.
    Json,
}

impl TraceMode {
    /// Whether this run records anything.
    pub const fn is_on(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// One step of a run, in the order the run took it.
///
/// Serialized as a tagged object so that a reader can switch on `event`
/// without positional knowledge, and so that adding a step cannot change the
/// shape of the steps already being read.
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum TraceEvent<'a> {
    /// Where the configuration came from, once per run.
    ConfigResolved {
        /// The directory the globs in the configuration are relative to.
        root: String,
        /// Each layer that contributed, weakest first.
        sources: &'a [String],
    },
    /// A file was read and a language was chosen for it.
    FileDetected {
        path: String,
        language: &'static str,
        dialect: &'static str,
        /// The evidence: `extension`, `reserved-filename`, `shebang`,
        /// `content`, `command-line`, or `configuration-routing`.
        how: &'static str,
        /// Bytes read, which is what a size-based skip would have been
        /// measured against.
        bytes: usize,
    },
    /// A file was not scanned.
    FileSkipped {
        path: String,
        reason: &'a str,
        /// Whether the skip is an error rather than a deliberate exclusion.
        error: bool,
    },
    /// A comment was found and the policy decided it.
    CommentDecided {
        path: String,
        /// One-based, matching the human report.
        line: usize,
        column: usize,
        kind: &'static str,
        action: &'static str,
        /// The rule that decided it, when the run has the material to say.
        #[serde(skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
    },
    /// An edit was planned for a removal.
    EditPlanned {
        path: String,
        start: usize,
        end: usize,
        /// How many bytes the removal leaves behind, which is what the layout
        /// decided.
        replacement_bytes: usize,
    },
    /// Everything the run did to one file.
    FileSummary {
        path: String,
        comments: usize,
        removable: usize,
        /// Whether the file's bytes would change.
        changed: bool,
        /// Whether the source lexed cleanly.
        valid: bool,
    },
}

impl TraceEvent<'_> {
    /// The `event` tag, which is also the name used in the human rendering.
    const fn name(&self) -> &'static str {
        match self {
            Self::ConfigResolved { .. } => "config-resolved",
            Self::FileDetected { .. } => "file-detected",
            Self::FileSkipped { .. } => "file-skipped",
            Self::CommentDecided { .. } => "comment-decided",
            Self::EditPlanned { .. } => "edit-planned",
            Self::FileSummary { .. } => "file-summary",
        }
    }

    /// Every tag, so a test can require each one to have been observed.
    ///
    /// A variant added without a fixture that reaches it is a step the trace
    /// claims to record and has never been seen recording. The end-to-end test
    /// in `tests/trace.rs` keeps its own copy of this list, because it runs the
    /// binary rather than linking it; the unit test below is what holds the two
    /// halves of that arrangement to the same enum.
    #[cfg(test)]
    const ALL_NAMES: [&'static str; 6] = [
        "config-resolved",
        "file-detected",
        "file-skipped",
        "comment-decided",
        "edit-planned",
        "file-summary",
    ];
}

/// Write one event in the mode the run asked for.
pub fn emit(writer: &mut impl Write, mode: TraceMode, event: &TraceEvent<'_>) -> Result<()> {
    match mode {
        TraceMode::Off => Ok(()),
        TraceMode::Json => {
            let line = serde_json::to_string(event).expect("a trace event serializes");
            // NOTE: Not the run's verbosity: `-q --trace` is how a caller gets only events.
            note(writer, Verbosity::default(), Detail::Normal, &line)
        }
        TraceMode::Human => note(writer, Verbosity::default(), Detail::Normal, &human(event)),
    }
}

/// One line of the human rendering.
///
/// Prefixed so that a reader who piped standard error somewhere can tell these
/// from the summary lines they arrive beside.
fn human(event: &TraceEvent<'_>) -> String {
    let body = match event {
        TraceEvent::ConfigResolved { root, sources } => {
            format!("root {root}; layers: {}", sources.join(" < "))
        }
        TraceEvent::FileDetected {
            path,
            language,
            dialect,
            how,
            bytes,
        } => format!("{path}: {language}/{dialect} by {how}, {bytes} bytes"),
        TraceEvent::FileSkipped { path, reason, .. } => format!("{path}: skipped, {reason}"),
        TraceEvent::CommentDecided {
            path,
            line,
            column,
            kind,
            action,
            rule,
        } => match rule {
            Some(rule) => format!("{path}:{line}:{column}: {kind} {action} — {rule}"),
            None => format!("{path}:{line}:{column}: {kind} {action}"),
        },
        TraceEvent::EditPlanned {
            path,
            start,
            end,
            replacement_bytes,
        } => format!("{path}: edit {start}..{end} leaves {replacement_bytes} bytes"),
        TraceEvent::FileSummary {
            path,
            comments,
            removable,
            changed,
            valid,
        } => format!(
            "{path}: {comments} comments, {removable} removable, changed={changed}, valid={valid}"
        ),
    };
    format!("trace {}: {body}", event.name())
}

/// The one-based line and column of a byte offset, counted as the reports do.
fn position(source: &[u8], offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let line = source[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1;
    let start = source[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    (line, offset - start + 1)
}

/// Record the discovery half of a run: what was read and what was not.
pub fn trace_discovery(
    writer: &mut impl Write,
    mode: TraceMode,
    files: &[SourceFile],
    skipped: &[SkippedFile],
) -> Result<()> {
    if !mode.is_on() {
        return Ok(());
    }
    for file in files {
        emit(
            writer,
            mode,
            &TraceEvent::FileDetected {
                path: display(&file.path),
                language: file.language.as_str(),
                dialect: file.dialect.as_str(),
                how: file.detection,
                bytes: file.source.len(),
            },
        )?;
    }
    for item in skipped {
        emit(
            writer,
            mode,
            &TraceEvent::FileSkipped {
                path: display(&item.path),
                reason: &item.reason,
                error: item.error,
            },
        )?;
    }
    Ok(())
}

/// Record what the policy decided, per comment, and what was planned from it.
pub fn trace_decisions(
    writer: &mut impl Write,
    mode: TraceMode,
    files: &[ProcessedFile],
    explanations: &Explanations,
) -> Result<()> {
    if !mode.is_on() {
        return Ok(());
    }
    for file in files {
        let path = display(&file.path);
        /* NOTE: Compiled once per file rather than once per comment, as the
         * reporting side does it, because the pattern sets are the same for
         * every comment in the file and compiling a regex set is the expensive
         * half of answering the question. */
        let material = explanations.get(&file.path);
        let patterns = material.map(|material| {
            DispositionPatterns::compile(&material.options)
                .unwrap_or_else(|_| DispositionPatterns::empty())
        });
        for comment in &file.result.report.comments {
            let (line, column) = position(&file.source, comment.span.start);
            emit(
                writer,
                mode,
                &TraceEvent::CommentDecided {
                    path: path.clone(),
                    line,
                    column,
                    kind: comment.kind.as_str(),
                    action: if comment.disposition.is_remove() {
                        "remove"
                    } else {
                        "keep"
                    },
                    rule: rule_for(file, comment, material, patterns.as_ref()),
                },
            )?;
        }
        for edit in &file.result.edits {
            emit(
                writer,
                mode,
                &TraceEvent::EditPlanned {
                    path: path.clone(),
                    start: edit.span.start,
                    end: edit.span.end,
                    replacement_bytes: edit.replacement.len(),
                },
            )?;
        }
        emit(
            writer,
            mode,
            &TraceEvent::FileSummary {
                path,
                comments: file.result.report.comments.len(),
                removable: file
                    .result
                    .report
                    .comments
                    .iter()
                    .filter(|comment| comment.disposition.is_remove())
                    .count(),
                changed: file.result.changed(),
                valid: file.result.report.valid,
            },
        )?;
    }
    Ok(())
}

/// The rule that decided one comment, and where that rule was written.
///
/// The verdict alone names the branch; the origin names the table the setting
/// came from. A trace is read by someone who is about to change a setting, so
/// it carries both, in the spelling `--explain` uses for the same pair.
fn rule_for(
    file: &ProcessedFile,
    comment: &Comment,
    material: Option<&crate::output::FileExplanation>,
    patterns: Option<&DispositionPatterns>,
) -> Option<String> {
    let (material, patterns) = (material?, patterns?);
    let start = comment.span.start.min(file.source.len());
    let end = comment.span.end.clamp(start, file.source.len());
    let verdict = explain_comment_with(
        patterns,
        comment,
        &file.source[start..end],
        file.language,
        &material.options,
    );
    Some(
        match material.trace.origin_of(&verdict, &material.options) {
            Some(origin) => format!("{verdict} ({origin})"),
            None => verdict.to_string(),
        },
    )
}

/// A path as one line, with control characters removed.
fn display(path: &Path) -> String {
    crate::output::sanitize_path(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ALL_NAMES` is the list a test can require every event to have been
    /// seen under, so it has to be the same list `name` can produce.
    ///
    /// Adding a variant without adding its tag here would leave the
    /// end-to-end test asserting over a list that no longer describes the
    /// enum, and it would pass. The `name` match is exhaustive, so the
    /// compiler catches the other direction.
    #[test]
    fn every_tag_is_listed() {
        let events = [
            TraceEvent::ConfigResolved {
                root: String::new(),
                sources: &[],
            },
            TraceEvent::FileDetected {
                path: String::new(),
                language: "rust",
                dialect: "standard",
                how: "extension",
                bytes: 0,
            },
            TraceEvent::FileSkipped {
                path: String::new(),
                reason: "",
                error: false,
            },
            TraceEvent::CommentDecided {
                path: String::new(),
                line: 1,
                column: 1,
                kind: "line",
                action: "remove",
                rule: None,
            },
            TraceEvent::EditPlanned {
                path: String::new(),
                start: 0,
                end: 0,
                replacement_bytes: 0,
            },
            TraceEvent::FileSummary {
                path: String::new(),
                comments: 0,
                removable: 0,
                changed: false,
                valid: true,
            },
        ];
        let names: Vec<&str> = events.iter().map(TraceEvent::name).collect();
        assert_eq!(
            names,
            TraceEvent::ALL_NAMES,
            "a trace event was added without its tag reaching ALL_NAMES"
        );
    }

    /// The tag a reader switches on is the tag serde writes.
    #[test]
    fn the_serialized_tag_is_the_name() {
        let event = TraceEvent::FileSummary {
            path: "a.rs".to_owned(),
            comments: 1,
            removable: 1,
            changed: true,
            valid: true,
        };
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).expect("serializes"))
                .expect("is JSON");
        assert_eq!(value["event"], event.name());
    }

    /// Positions are one-based and counted the way the reports count them.
    #[test]
    fn positions_are_one_based() {
        let source = b"one\ntwo\nthree";
        assert_eq!(position(source, 0), (1, 1));
        assert_eq!(position(source, 4), (2, 1));
        assert_eq!(position(source, 6), (2, 3));
        // NOTE: Past the end clamps rather than panicking, because a span from
        // NOTE: a plugin is not this crate's to trust.
        assert_eq!(position(source, 9_999), (3, 6));
    }
}
