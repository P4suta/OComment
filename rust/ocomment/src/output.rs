use crate::files::SkippedFile;
use anyhow::Result;
use clap::ValueEnum;
use ocomment_core::{ByteSpan, CommentKind, Language, TransformResult};
use serde::Serialize;
use serde_json::{Value, json};
use similar::{ChangeTag, TextDiff};
use std::{
    collections::BTreeMap,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
};
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
    Jsonl,
    Sarif,
    Github,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Check,
    Scan,
    Diff,
    Fix,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Presentation {
    pub color: bool,
    pub hyperlinks: bool,
}

/// How much of the human report a run is allowed to write.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Verbosity {
    /// Only errors and diagnostics.
    Quiet,
    #[default]
    Normal,
    /// Everything, including the per-kind breakdown and every skipped file.
    Verbose,
}

/// Everything the renderer needs besides the results themselves.
#[derive(Clone, Copy, Debug)]
pub struct RenderOptions {
    pub format: OutputFormat,
    pub operation: Operation,
    pub presentation: Presentation,
    pub verbosity: Verbosity,
    /// Human lines carry a one-line rendering of the comment text.
    pub preview: bool,
    // Plumbed for the presentation work that follows; nothing reads it yet.
    #[allow(dead_code)]
    pub explain: bool,
    /// The run is `fix --dry-run`: it produces the diff but speaks the
    /// vocabulary of the `fix` it is standing in for.
    pub dry_run: bool,
    /// `--force-invalid` was in effect, so a file that fails to scan still had
    /// its provably safe edits applied.
    pub force_invalid: bool,
    /// The run reached the disk. A `fix` blocked by invalid syntax or an I/O
    /// error leaves this false and must not claim any removal.
    pub applied: bool,
}

/// What one run found, counted once for the end-of-run summary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Summary {
    pub files_scanned: usize,
    pub files_with_removable: usize,
    pub removable_comments: usize,
    pub kept_comments: usize,
    pub files_changed: usize,
    pub comments_removed: usize,
    pub invalid_files: usize,
    /// Non-error skips met while walking, counted under a short stable label
    /// rather than the raw reason, which can carry a configured byte limit.
    /// A path named on the command line is deliberately absent: it already has
    /// its own line on standard output and must not be counted twice.
    pub skipped_by_reason: BTreeMap<String, usize>,
    /// Non-error skips whose path was named on the command line.
    pub named_skips: usize,
    pub io_errors: usize,
}

impl Summary {
    pub fn compute(files: &[ProcessedFile], skipped: &[SkippedFile], operation: Operation) -> Self {
        let mut summary = Self {
            files_scanned: files.len(),
            ..Self::default()
        };
        for file in files {
            let removable = removable_count(file);
            summary.removable_comments += removable;
            summary.kept_comments += file.result.report.comments.len() - removable;
            if removable > 0 {
                summary.files_with_removable += 1;
            }
            if !file.result.report.valid {
                summary.invalid_files += 1;
            }
            if file.source != file.result.output {
                summary.files_changed += 1;
                if operation == Operation::Fix {
                    summary.comments_removed += removable;
                }
            }
        }
        for item in skipped {
            if item.error {
                summary.io_errors += 1;
            } else if item.explicit {
                summary.named_skips += 1;
            } else {
                *summary
                    .skipped_by_reason
                    .entry(skip_label(&item.reason).to_owned())
                    .or_default() += 1;
            }
        }
        summary
    }

    fn skipped_files(&self) -> usize {
        self.skipped_by_reason.values().sum()
    }
}

fn removable_count(file: &ProcessedFile) -> usize {
    file.result
        .report
        .comments
        .iter()
        .filter(|comment| comment.disposition.is_remove())
        .count()
}

/// Fold a skip reason onto a short label the summary can group by.
fn skip_label(reason: &str) -> &str {
    if reason.starts_with("larger than ") {
        "too large"
    } else if reason.starts_with("binary file") {
        "binary"
    } else if reason.starts_with("language disabled") {
        "language disabled"
    } else {
        reason
    }
}

/// `1 file` / `2 files`: the count and its noun, pluralized by the regular
/// rule. Every noun the summary counts goes through this.
fn plural(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

/// `1 comment` / `2 removable comments`: the noun is pluralized and an
/// optional adjective is placed in front of it.
fn comments(count: usize, adjective: &str) -> String {
    let space = if adjective.is_empty() { "" } else { " " };
    plural(count, &format!("{adjective}{space}comment"))
}

#[derive(Clone, Debug)]
pub struct ProcessedFile {
    pub path: PathBuf,
    pub source: Vec<u8>,
    pub language: Language,
    pub result: TransformResult,
}

#[derive(Serialize)]
struct JsonFile<'a> {
    path: String,
    language: Language,
    changed: bool,
    report: &'a ocomment_core::ScanReport,
    edits: &'a [ocomment_core::Edit],
    source_map: &'a ocomment_core::SourceMap,
}

/// The one-line label for a comment OComment would delete.
pub fn removable_label(kind: CommentKind) -> String {
    format!("removable {kind} comment")
}

/// The one-line label for a comment OComment deliberately protects.
pub fn kept_label(kind: CommentKind, reason: &str) -> String {
    format!("kept {kind} comment: {reason}")
}

/// How many display columns a comment preview may occupy.
const PREVIEW_COLUMNS: usize = 72;

/// A one-line, terminal-safe rendering of the comment at `span`.
///
/// Comment text is untrusted input that is about to be written to a terminal,
/// so the whole comment is folded onto one line, every control character —
/// `ESC` above all — is replaced with U+FFFD instead of being forwarded, and
/// the result is cut to `max_columns` display columns.
fn preview(source: &[u8], span: ByteSpan, max_columns: usize) -> String {
    let start = span.start.min(source.len());
    let end = span.end.clamp(start, source.len());
    let text = String::from_utf8_lossy(&source[start..end]);
    let mut folded = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.chars() {
        if matches!(character, ' ' | '\t' | '\r' | '\n' | '\u{c}') {
            // Leading whitespace is dropped, and a run only becomes a space
            // once something else follows it, so the tail is trimmed too.
            pending_space = !folded.is_empty();
            continue;
        }
        if pending_space {
            folded.push(' ');
            pending_space = false;
        }
        folded.push(if is_control(character) {
            '\u{fffd}'
        } else {
            character
        });
    }
    truncate(folded, max_columns)
}

/// C0, DEL, C1, and the bidirectional and separator format controls. None of
/// these may reach the terminal verbatim: C0 drives it, the bidi overrides and
/// isolates can make a comment render as its own reverse, and U+2028/U+2029
/// break the promise that a preview is one line. U+061C joins the marks it
/// belongs with, and U+FEFF is invisible wherever it lands.
fn is_control(character: char) -> bool {
    matches!(
        character,
        '\u{0}'..='\u{1f}'
            | '\u{7f}'..='\u{9f}'
            | '\u{61c}'
            | '\u{200e}'..='\u{200f}'
            | '\u{2028}'..='\u{2029}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

fn columns(character: char) -> usize {
    UnicodeWidthChar::width(character).unwrap_or(0)
}

/// How many characters a preview may carry for each column it may occupy.
/// Zero-width and combining characters cost no columns, so the width budget on
/// its own cannot bound the line a terminal has to hold.
const PREVIEW_CHARS_PER_COLUMN: usize = 4;

/// Cut `text` to `max_columns` display columns and to a hard character cap,
/// never inside a wide character, leaving room for the ellipsis that marks the
/// cut.
fn truncate(text: String, max_columns: usize) -> String {
    let max_chars = max_columns.saturating_mul(PREVIEW_CHARS_PER_COLUMN);
    if text.chars().map(columns).sum::<usize>() <= max_columns && text.chars().count() <= max_chars
    {
        return text;
    }
    let column_budget = max_columns.saturating_sub(1);
    let char_budget = max_chars.saturating_sub(1);
    let mut cut = String::with_capacity(text.len());
    let mut width = 0usize;
    for (taken, character) in text.chars().enumerate() {
        if taken >= char_budget {
            break;
        }
        width += columns(character);
        if width > column_budget {
            break;
        }
        cut.push(character);
    }
    cut.push('\u{2026}');
    cut
}

/// The `: <text>` tail a human line carries, dimmed when colour is on.
fn preview_suffix(source: &[u8], span: ByteSpan, options: &RenderOptions) -> String {
    if !options.preview {
        return String::new();
    }
    let text = preview(source, span, PREVIEW_COLUMNS);
    if text.is_empty() {
        return String::new();
    }
    format!(
        ": {}{text}{}",
        color("\x1b[2m", options.presentation.color),
        color("\x1b[0m", options.presentation.color)
    )
}

/// The handle every path that writes the product of a run takes: standard
/// output, locked once for the whole run and buffered.
///
/// `println!` panics when its write fails, and the release profile aborts on
/// panic, so a reader that stops early — `ocomment … | head` — would end the
/// process with SIGABRT. Writing through a handle that returns its errors lets
/// the caller decide instead, and `main` ends a closed pipe quietly.
pub type Stdout = BufWriter<io::StdoutLock<'static>>;

/// Lock standard output for the rest of the run and buffer it.
pub fn stdout() -> Stdout {
    BufWriter::new(io::stdout().lock())
}

/// The reader of the program's own output went away mid-run.
///
/// A broken pipe is only benign when it is *our* report that could not be
/// written; `ocomment … | head` is a reader that finished, not a run that
/// failed. Every other broken pipe — writing a rewritten blob into
/// `git hash-object`, for one — is a real failure, so the benign case is
/// tagged with this marker at the write that raised it instead of being
/// recognized by error kind anywhere in the chain.
#[derive(Debug)]
pub struct OutputPipeClosed;

impl std::fmt::Display for OutputPipeClosed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the reader of standard output closed the pipe")
    }
}

impl std::error::Error for OutputPipeClosed {}

/// Push the last buffered bytes out.
///
/// A `BufWriter` drops the error of the write it performs while being dropped,
/// so every writer is finished by hand and the failure reaches the caller.
pub fn finish(writer: &mut impl Write) -> Result<()> {
    wrote(writer.flush())
}

/// Raise one write to the program's own output, tagging the reader that closed
/// the pipe so `main` can end quietly for that case alone.
pub fn wrote(result: io::Result<()>) -> Result<()> {
    result.map_err(output_failure)
}

/// The error one failed write to our own output becomes.
fn output_failure(error: io::Error) -> anyhow::Error {
    if error.kind() == io::ErrorKind::BrokenPipe {
        return anyhow::Error::new(OutputPipeClosed);
    }
    anyhow::Error::new(error).context("cannot write standard output")
}

/// Write one line of commentary to standard error.
///
/// Commentary — the `-v` trace, the end-of-run summary — is not the product of
/// the run, so a reader that has already gone away is not a failure to report:
/// a closed pipe is dropped and only a real write failure is raised. What must
/// not happen is what `eprintln!` does, which is panic, and so abort under the
/// release profile.
pub fn note(writer: &mut impl Write, line: &str) -> Result<()> {
    match writeln!(writer, "{line}") {
        Err(error) if error.kind() != io::ErrorKind::BrokenPipe => {
            Err(anyhow::Error::new(error).context("cannot write standard error"))
        }
        _ => Ok(()),
    }
}

/// Turn a serialization failure back into the I/O error it usually is.
///
/// `serde_json` reports a failed write as an error of its own whose `source`
/// is the *source* of the I/O error rather than the I/O error itself, so a
/// closed pipe would be invisible to anything walking the chain. Its `From`
/// conversion hands the original error back.
fn write_error(error: serde_json::Error) -> anyhow::Error {
    output_failure(io::Error::from(error))
}

pub fn render(
    files: &[ProcessedFile],
    skipped: &[SkippedFile],
    options: &RenderOptions,
) -> Result<()> {
    let mut output = stdout();
    match options.format {
        OutputFormat::Human => render_human(&mut output, files, skipped, options),
        OutputFormat::Json => render_json(&mut output, files, skipped),
        OutputFormat::Jsonl => render_jsonl(&mut output, files, skipped),
        OutputFormat::Sarif => render_sarif(&mut output, files, skipped),
        OutputFormat::Github => render_github(&mut output, files, skipped),
    }?;
    finish(&mut output)
}

fn render_human(
    output: &mut impl Write,
    files: &[ProcessedFile],
    skipped: &[SkippedFile],
    options: &RenderOptions,
) -> Result<()> {
    let operation = options.operation;
    let presentation = options.presentation;
    let quiet = options.verbosity == Verbosity::Quiet;
    let verbose = options.verbosity == Verbosity::Verbose;
    for file in files {
        if operation == Operation::Diff && file.source != file.result.output {
            // The patch is the product of `diff`, so `-q` keeps it and drops
            // only the summary that follows on standard error.
            wrote(write!(
                output,
                "{}",
                unified_diff(&file.path, &file.source, &file.result.output)
            ))?;
            continue;
        }
        for diagnostic in &file.result.report.diagnostics {
            let (line, column) = line_column(&file.source, diagnostic.span.start);
            wrote(writeln!(
                output,
                "{}:{line}:{column}: {}{}[{}]{}: {}",
                display_path(&file.path, presentation.hyperlinks),
                color("\x1b[31m", presentation.color),
                diagnostic.severity,
                diagnostic.code,
                color("\x1b[0m", presentation.color),
                diagnostic.message
            ))?;
        }
        if operation == Operation::Scan {
            // The listing is the product of `scan`; `-q` keeps it too.
            for comment in &file.result.report.comments {
                let (line, column) = line_column(&file.source, comment.span.start);
                wrote(writeln!(
                    output,
                    "{}:{line}:{column}: {} {} {}..{}{}",
                    display_path(&file.path, presentation.hyperlinks),
                    comment.kind,
                    comment.disposition,
                    comment.span.start,
                    comment.span.end,
                    preview_suffix(&file.source, comment.span, options)
                ))?;
            }
        } else if quiet {
            continue;
        } else if operation == Operation::Fix {
            if options.applied && file.source != file.result.output {
                wrote(writeln!(
                    output,
                    "fixed {}: removed {}",
                    display_path(&file.path, presentation.hyperlinks),
                    comments(removable_count(file), "")
                ))?;
            }
        } else {
            for comment in file
                .result
                .report
                .comments
                .iter()
                .filter(|comment| comment.disposition.is_remove())
            {
                let (line, column) = line_column(&file.source, comment.span.start);
                wrote(writeln!(
                    output,
                    "{}:{line}:{column}: {}{}{}{}",
                    display_path(&file.path, presentation.hyperlinks),
                    color("\x1b[33m", presentation.color),
                    removable_label(comment.kind),
                    color("\x1b[0m", presentation.color),
                    preview_suffix(&file.source, comment.span, options)
                ))?;
            }
        }
    }
    if operation != Operation::Diff {
        for item in skipped {
            if !item.error && (quiet || !(item.explicit || verbose)) {
                continue;
            }
            wrote(writeln!(
                output,
                "{}: {}: {}",
                display_path(&item.path, presentation.hyperlinks),
                if item.error { "error" } else { "skipped" },
                item.reason
            ))?;
        }
    }
    if quiet {
        return Ok(());
    }
    // The findings are on standard output and the summary that follows is on
    // standard error; a terminal sees both, so the buffer is emptied first to
    // keep the report in the order it was written.
    finish(output)?;
    let summary = Summary::compute(files, skipped, operation);
    let folded = !verbose && skipped.iter().any(|item| !item.error && !item.explicit);
    let stderr = io::stderr();
    let mut report = stderr.lock();
    if verbose && let Some(line) = kind_breakdown(files, options) {
        note(&mut report, &line)?;
    }
    note(&mut report, &summary_report(&summary, options, folded))?;
    if summary.invalid_files > 0 && !options.force_invalid {
        let (verb, pronoun) = if summary.invalid_files == 1 {
            ("has", "it")
        } else {
            ("have", "them")
        };
        note(
            &mut report,
            &format!(
                "{} {verb} invalid syntax; nothing was written for {pronoun} \
                 (use --force-invalid to apply known-safe edits).",
                plural(summary.invalid_files, "file")
            ),
        )?;
    }
    Ok(())
}

/// The whole end-of-run summary: the verdict for the run, the folded skips,
/// and the I/O errors that were listed one by one above it.
fn summary_report(summary: &Summary, options: &RenderOptions, folded: bool) -> String {
    let skips = skip_clause(summary, folded);
    let nothing = nothing_to(options);
    let mut report = if summary.files_scanned > 0 {
        format!("{}{skips}", summary_line(summary, options))
    } else if !skips.is_empty() {
        // Nothing was scanned, so the verdict would count zero files; what the
        // run actually did was pass every candidate over.
        format!("Nothing to {nothing}:{skips}")
    } else if summary.named_skips > 0 {
        format!("Nothing to {nothing}.")
    } else {
        summary_line(summary, options)
    };
    if summary.io_errors > 0 {
        report.push_str(&format!(" {}.", plural(summary.io_errors, "I/O error")));
    }
    report
}

/// The verb a run uses for the work it found nothing to do. `fix --dry-run`
/// borrows the vocabulary of the `fix` it is standing in for, as it does
/// everywhere else in the summary.
fn nothing_to(options: &RenderOptions) -> &'static str {
    match options.operation {
        Operation::Check => "check",
        Operation::Fix => "fix",
        Operation::Diff if options.dry_run => "fix",
        Operation::Diff => "diff",
        Operation::Scan => "scan",
    }
}

/// The one-line verdict for the run, without the skipped-file clause.
fn summary_line(summary: &Summary, options: &RenderOptions) -> String {
    let scanned = plural(summary.files_scanned, "file");
    let found = || {
        format!(
            "Found {} in {} ({scanned} scanned).",
            comments(summary.removable_comments, "removable"),
            plural(summary.files_with_removable, "file")
        )
    };
    match options.operation {
        // `fix --dry-run` is the diff of a fix: it counts what a real run would
        // take out and points back at the run that would write it.
        Operation::Diff if options.dry_run => {
            if summary.removable_comments == 0 {
                return format!("Nothing to fix in {scanned}.");
            }
            format!(
                "Would remove {} in {}. Rerun without --dry-run to apply.",
                comments(summary.removable_comments, ""),
                plural(summary.files_with_removable, "file")
            )
        }
        Operation::Check | Operation::Diff => {
            if summary.removable_comments == 0 {
                return format!("No removable comments in {scanned}.");
            }
            let next = if options.operation == Operation::Diff {
                "apply the patch"
            } else if summary.removable_comments == 1 {
                "remove it"
            } else {
                "remove them"
            };
            format!("{} Run `ocomment fix` to {next}.", found())
        }
        Operation::Fix => {
            if options.applied && summary.files_changed > 0 {
                format!(
                    "Removed {} in {} ({scanned} scanned).",
                    comments(summary.comments_removed, ""),
                    plural(summary.files_changed, "file")
                )
            } else if summary.removable_comments == 0 {
                format!("Nothing to fix in {scanned}.")
            } else {
                // The transaction never reached the disk; report what is still
                // there rather than claiming a removal.
                found()
            }
        }
        Operation::Scan => format!(
            "Scanned {scanned}: {} ({} removable, {} kept).",
            comments(summary.removable_comments + summary.kept_comments, ""),
            summary.removable_comments,
            summary.kept_comments
        ),
    }
}

/// The skipped-file clause appended to the summary line. Only the skips met
/// while walking are folded here; a named path was already reported on its own
/// line.
fn skip_clause(summary: &Summary, folded: bool) -> String {
    let total = summary.skipped_files();
    if total == 0 {
        return String::new();
    }
    let reasons: Vec<_> = summary
        .skipped_by_reason
        .iter()
        .map(|(label, count)| format!("{label}: {count}"))
        .collect();
    let hint = if folded { "; use -v to list" } else { "" };
    format!(
        " {} skipped ({}{hint}).",
        plural(total, "file"),
        reasons.join(", ")
    )
}

/// The `-v` breakdown of what each comment kind contributed.
fn kind_breakdown(files: &[ProcessedFile], options: &RenderOptions) -> Option<String> {
    let verb = if options.operation == Operation::Fix && options.applied {
        "removed"
    } else {
        "removable"
    };
    let mut removable = [0usize; CommentKind::ALL.len()];
    let mut kept = [0usize; CommentKind::ALL.len()];
    for file in files {
        for comment in &file.result.report.comments {
            let slot = CommentKind::ALL
                .iter()
                .position(|kind| *kind == comment.kind)
                .expect("CommentKind::ALL lists every kind");
            if comment.disposition.is_remove() {
                removable[slot] += 1;
            } else {
                kept[slot] += 1;
            }
        }
    }
    let mut parts = Vec::new();
    for (slot, kind) in CommentKind::ALL.into_iter().enumerate() {
        if removable[slot] > 0 {
            parts.push(format!("{kind} {} {verb}", removable[slot]));
        }
        if kept[slot] > 0 {
            parts.push(format!("{kind} {} kept", kept[slot]));
        }
    }
    (!parts.is_empty()).then(|| format!("kinds: {}", parts.join(", ")))
}

fn color(code: &'static str, enabled: bool) -> &'static str {
    if enabled { code } else { "" }
}

fn display_path(path: &Path, hyperlinks: bool) -> String {
    let display = path.display().to_string();
    if !hyperlinks {
        return display;
    }
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let target = absolute
        .to_string_lossy()
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23");
    format!("\x1b]8;;file://{target}\x1b\\{display}\x1b]8;;\x1b\\")
}

fn render_json(
    output: &mut impl Write,
    files: &[ProcessedFile],
    skipped: &[SkippedFile],
) -> Result<()> {
    let values: Vec<_> = files.iter().map(json_file).collect();
    let skipped: Vec<_> = skipped
        .iter()
        .map(|item| {
            json!({"path": item.path.to_string_lossy(), "reason": item.reason, "error": item.error})
        })
        .collect();
    serde_json::to_writer_pretty(
        &mut *output,
        &json!({"version": 1, "files": values, "skipped": skipped}),
    )
    .map_err(write_error)?;
    wrote(writeln!(output))?;
    Ok(())
}

fn render_jsonl(
    output: &mut impl Write,
    files: &[ProcessedFile],
    skipped: &[SkippedFile],
) -> Result<()> {
    for file in files {
        serde_json::to_writer(&mut *output, &json_file(file)).map_err(write_error)?;
        wrote(writeln!(output))?;
    }
    for item in skipped {
        serde_json::to_writer(
            &mut *output,
            &json!({"type": "skip", "path": item.path.to_string_lossy(), "reason": item.reason, "error": item.error}),
        )
        .map_err(write_error)?;
        wrote(writeln!(output))?;
    }
    Ok(())
}

fn json_file(file: &ProcessedFile) -> JsonFile<'_> {
    JsonFile {
        path: file.path.to_string_lossy().into_owned(),
        language: file.language,
        changed: file.source != file.result.output,
        report: &file.result.report,
        edits: &file.result.edits,
        source_map: &file.result.source_map,
    }
}

fn render_sarif(
    output: &mut impl Write,
    files: &[ProcessedFile],
    skipped: &[SkippedFile],
) -> Result<()> {
    let mut results = Vec::new();
    for file in files {
        for comment in file
            .result
            .report
            .comments
            .iter()
            .filter(|comment| comment.disposition.is_remove())
        {
            let (line, column) = line_column(&file.source, comment.span.start);
            let (end_line, end_column) = line_column(&file.source, comment.span.end);
            let kind = comment.kind.as_str();
            results.push(json!({
                "ruleId": format!("removable-{kind}"),
                "level": "note",
                "message": {"text": removable_label(comment.kind)},
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": file.path.to_string_lossy()},
                    "region": {"startLine": line, "startColumn": column,
                        "endLine": end_line, "endColumn": end_column}
                }}],
                "fixes": [{
                    "description": {"text": "Remove comment with OComment"},
                    "artifactChanges": [{
                        "artifactLocation": {"uri": file.path.to_string_lossy()},
                        "replacements": [{"deletedRegion": {
                            "startLine": line, "startColumn": column,
                            "endLine": end_line, "endColumn": end_column
                        }, "insertedContent": {"text": replacement_for_span(file, comment.span)}}]
                    }]
                }]
            }));
        }
        for diagnostic in &file.result.report.diagnostics {
            let (line, column) = line_column(&file.source, diagnostic.span.start);
            let (end_line, end_column) = line_column(&file.source, diagnostic.span.end);
            let level = match diagnostic.severity {
                ocomment_core::Severity::Error => "error",
                ocomment_core::Severity::Warning => "warning",
                ocomment_core::Severity::Info | ocomment_core::Severity::Hint => "note",
            };
            results.push(json!({
                "ruleId": diagnostic.code,
                "level": level,
                "message": {"text": diagnostic.message},
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": file.path.to_string_lossy()},
                    "region": {"startLine": line, "startColumn": column,
                        "endLine": end_line, "endColumn": end_column}
                }}]
            }));
        }
    }
    for item in skipped {
        results.push(json!({
            "ruleId": if item.error { "io-error" } else { "skipped-file" },
            "level": if item.error { "error" } else { "note" },
            "message": {"text": item.reason},
            "locations": [{"physicalLocation": {
                "artifactLocation": {"uri": item.path.to_string_lossy()}
            }}]
        }));
    }
    let sarif = json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{"tool": {"driver": {"name": "ocomment", "informationUri": "https://github.com/P4suta/OComment"}}, "results": results}]
    });
    serde_json::to_writer_pretty(&mut *output, &sarif).map_err(write_error)?;
    wrote(writeln!(output))?;
    Ok(())
}

fn replacement_for_span(file: &ProcessedFile, span: ByteSpan) -> String {
    file.result
        .edits
        .iter()
        .find(|edit| edit.span == span)
        .map(|edit| String::from_utf8_lossy(&edit.replacement).into_owned())
        .unwrap_or_default()
}

fn render_github(
    output: &mut impl Write,
    files: &[ProcessedFile],
    skipped: &[SkippedFile],
) -> Result<()> {
    for file in files {
        for comment in file
            .result
            .report
            .comments
            .iter()
            .filter(|comment| comment.disposition.is_remove())
        {
            let (line, column) = line_column(&file.source, comment.span.start);
            wrote(writeln!(
                output,
                "::notice file={},line={line},col={column}::{}",
                github_escape(&file.path.to_string_lossy()),
                removable_label(comment.kind)
            ))?;
        }
        for diagnostic in &file.result.report.diagnostics {
            let (line, column) = line_column(&file.source, diagnostic.span.start);
            wrote(writeln!(
                output,
                "::error file={},line={line},col={column},title={}::{}",
                github_escape(&file.path.to_string_lossy()),
                github_escape(&diagnostic.code),
                github_escape(&diagnostic.message)
            ))?;
        }
    }
    for item in skipped {
        wrote(writeln!(
            output,
            "::{} file={},title={}::{}",
            if item.error { "error" } else { "notice" },
            github_escape(&item.path.to_string_lossy()),
            if item.error {
                "OComment I/O error"
            } else {
                "OComment skipped file"
            },
            github_escape(&item.reason)
        ))?;
    }
    Ok(())
}

pub fn unified_diff(path: &Path, original: &[u8], transformed: &[u8]) -> String {
    let old = String::from_utf8_lossy(original);
    let new = String::from_utf8_lossy(transformed);
    let display = path.to_string_lossy().replace('\\', "/");
    let mut output = format!("--- a/{display}\n+++ b/{display}\n");
    let diff = TextDiff::from_lines(&old, &new);
    for group in diff.grouped_ops(3) {
        let old_start = group.first().map_or(0, |op| op.old_range().start) + 1;
        let new_start = group.first().map_or(0, |op| op.new_range().start) + 1;
        let old_len: usize = group.iter().map(|op| op.old_range().len()).sum();
        let new_len: usize = group.iter().map(|op| op.new_range().len()).sum();
        output.push_str(&format!(
            "@@ -{old_start},{old_len} +{new_start},{new_len} @@\n"
        ));
        for op in group {
            for change in diff.iter_changes(&op) {
                let prefix = match change.tag() {
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                    ChangeTag::Equal => ' ',
                };
                output.push(prefix);
                output.push_str(change.value());
                if !change.value().ends_with('\n') {
                    output.push_str("\n\\ No newline at end of file\n");
                }
            }
        }
    }
    output
}

fn line_column(source: &[u8], offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let mut line = 1usize;
    let mut start = 0usize;
    let mut index = 0usize;
    while index < offset {
        if source[index] == b'\r' {
            index += if source.get(index + 1) == Some(&b'\n') && index + 1 < offset {
                2
            } else {
                1
            };
            line += 1;
            start = index;
        } else if source[index] == b'\n' {
            index += 1;
            line += 1;
            start = index;
        } else {
            index += 1;
        }
    }
    (line, offset - start + 1)
}

fn github_escape(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

pub fn changed(files: &[ProcessedFile]) -> bool {
    files.iter().any(|file| file.source != file.result.output)
}
pub fn invalid(files: &[ProcessedFile]) -> bool {
    files.iter().any(|file| !file.result.report.valid)
}

#[allow(dead_code)]
fn _span(_: ByteSpan) -> Value {
    Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview_of(source: &[u8], max_columns: usize) -> String {
        preview(source, ByteSpan::new(0, source.len()), max_columns)
    }

    #[test]
    fn preview_collapses_every_run_of_whitespace_and_trims() {
        assert_eq!(
            preview_of(b"  /*\r\n\tkeep\t this  tidy \x0c*/  ", 72),
            "/* keep this tidy */"
        );
    }

    #[test]
    fn preview_truncates_on_display_width_without_splitting_a_wide_character() {
        let source = "ab漢字漢字漢字ab".as_bytes();
        assert_eq!(preview_of(source, 20), "ab漢字漢字漢字ab");
        let cut = preview_of(source, 10);
        assert_eq!(cut, "ab漢字漢…");
        assert!(cut.ends_with('…'), "truncation is unmarked: {cut}");
        let width: usize = cut
            .chars()
            .map(|ch| unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0))
            .sum();
        assert!(width <= 10, "`{cut}` is {width} columns wide");
    }

    #[test]
    fn preview_replaces_control_characters_with_the_replacement_character() {
        let source = b"// \x1b[31m\x07 \xc2\x9b\x7f bell";
        let rendered = preview_of(source, 72);
        assert_eq!(rendered, "// \u{fffd}[31m\u{fffd} \u{fffd}\u{fffd} bell");
        assert!(
            !rendered.contains('\x1b'),
            "an escape sequence survived: {rendered:?}"
        );
    }

    #[test]
    fn preview_replaces_invalid_utf8_bytes() {
        assert_eq!(
            preview_of(b"// \xff\xfe end", 72),
            "// \u{fffd}\u{fffd} end"
        );
    }

    /// Bidi overrides and isolates can make a comment render as its own
    /// reverse, and the line/paragraph separators break the one-line promise.
    #[test]
    fn preview_replaces_bidirectional_and_separator_controls() {
        let source = "// \u{202e}reverse\u{202c} \u{200e}\u{200f} \u{2066}iso\u{2069} \
                      \u{2028}\u{2029} \u{61c}\u{feff} end";
        assert_eq!(
            preview_of(source.as_bytes(), 72),
            "// \u{fffd}reverse\u{fffd} \u{fffd}\u{fffd} \u{fffd}iso\u{fffd} \
             \u{fffd}\u{fffd} \u{fffd}\u{fffd} end"
        );
        for character in [
            '\u{61c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}',
            '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', '\u{2028}', '\u{2029}',
            '\u{feff}',
        ] {
            assert!(
                is_control(character),
                "U+{:04X} still reaches the terminal",
                character as u32
            );
        }
    }

    /// Zero-width characters cost no display columns, so the width budget alone
    /// cannot bound the line; a hard character cap must.
    #[test]
    fn preview_caps_the_character_count_of_a_zero_width_run() {
        let source = format!("a{}", "\u{301}".repeat(1000));
        let rendered = preview_of(source.as_bytes(), 8);
        assert!(
            rendered.chars().count() <= 8 * 4,
            "preview is {} characters wide",
            rendered.chars().count()
        );
        assert!(rendered.ends_with('\u{2026}'), "truncation is unmarked");
    }

    #[test]
    fn preview_reads_only_the_span() {
        let source = b"let x = 1; // TODO remove\n";
        assert_eq!(preview(source, ByteSpan::new(11, 25), 72), "// TODO remove");
    }
}
