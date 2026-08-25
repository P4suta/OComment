use crate::{
    config::PolicyTrace,
    files::{NO_LANGUAGE, STDIN_PATH, SkippedFile},
};
use anyhow::Result;
use clap::ValueEnum;
use ocomment_core::{
    ByteSpan, Comment, CommentKind, Disposition, DispositionExplanation, DispositionPatterns,
    Language, Policy, ScanOptions, TransformResult, explain_disposition_with,
};
use serde::Serialize;
use serde_json::{Value, json};
use similar::{ChangeTag, TextDiff};
use std::{
    collections::BTreeMap,
    io::{self, BufWriter, Write},
    path::{Component, Path, PathBuf},
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
    /// Human `check` and `scan` lines carry every comment, kept ones included,
    /// each under an indented line naming the rule that decided it.
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
    /// The policy the run was asked for. Only `all` promises to take every
    /// comment out, so only `all` owes an explanation for the ones it keeps.
    pub policy: Policy,
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
///
/// The per-file line says what to do about one file; the summary counts many,
/// so it trades the sentence for a key short enough to sit in a list of them.
fn skip_label(reason: &str) -> &str {
    if reason.starts_with("larger than ") {
        "too large"
    } else if reason.starts_with("binary file") {
        "binary"
    } else if reason.starts_with("language disabled") {
        "language disabled"
    } else if reason == NO_LANGUAGE {
        "unknown language"
    } else {
        reason
    }
}

/// The `Keep` reason the core scanner gives a shebang or encoding line that
/// `--force-protected` would have removed. It is one of the five reasons the
/// differential protocol freezes, so matching on it is stable; the end-to-end
/// test `policy_all_says_how_to_remove_a_kept_preamble` is what would catch it
/// drifting apart from the scanner.
const PROTECTED_PREAMBLE: &str = "required source preamble";

/// How many comments were kept only because `--force-protected` was absent.
///
/// Counted from the disposition rather than from the comment kind: a shebang
/// held back by `--keep-kind shebang` stays kept whatever `--force-protected`
/// says, and advertising the flag for it would be a lie.
fn protected_preambles(files: &[ProcessedFile]) -> usize {
    files
        .iter()
        .flat_map(|file| &file.result.report.comments)
        .filter(|comment| {
            matches!(&comment.disposition, Disposition::Keep { reason } if reason == PROTECTED_PREAMBLE)
        })
        .count()
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
    format!("{}: {reason}", kept_prefix(kind))
}

/// The same label without a reason, for a report that gives the reason on a
/// line of its own.
fn kept_prefix(kind: CommentKind) -> String {
    format!("kept {kind} comment")
}

/// What `--explain` needs to account for one file's comments: the options its
/// scan actually ran with, and where each of their settings came from.
#[derive(Clone, Debug)]
pub struct FileExplanation {
    pub options: ScanOptions,
    pub trace: PolicyTrace,
}

/// That material for the files of one run, under the path the run reports each
/// file by. A run that was not asked to explain anything carries none.
pub type Explanations = BTreeMap<PathBuf, FileExplanation>;

/// One file's explanation material with its policy patterns already compiled.
///
/// The two regex sets are the same for every comment in the file, so they are
/// built once when the file is reached rather than once per reported line.
struct Explainer<'a> {
    material: &'a FileExplanation,
    patterns: DispositionPatterns,
}

impl<'a> Explainer<'a> {
    /// An unparseable pattern list is ignored here as the scanner ignores it,
    /// which is exactly what `explain_disposition` falls back to on its own.
    fn new(material: &'a FileExplanation) -> Self {
        Self {
            patterns: DispositionPatterns::compile(&material.options)
                .unwrap_or_else(|_| DispositionPatterns::empty()),
            material,
        }
    }
}

/// The indented line under one reported comment: the rule that decided its
/// fate, and either the setting behind that rule or the flag that would
/// overrule it.
///
/// The pattern a regex explanation quotes and the globs a source names were
/// both written by whoever wrote the configuration, so the composed line gets a
/// comment preview's treatment before it reaches a terminal: one line, no
/// control sequences. The width is not capped — a line that ends in an ellipsis
/// where the pattern was answers nothing.
fn explanation_line(
    file: &ProcessedFile,
    comment: &Comment,
    explainer: &Explainer<'_>,
    options: &RenderOptions,
) -> String {
    let material = explainer.material;
    let start = comment.span.start.min(file.source.len());
    let end = comment.span.end.clamp(start, file.source.len());
    let verdict = explain_disposition_with(
        &explainer.patterns,
        comment.kind,
        &file.source[start..end],
        file.language,
        &material.options,
    );
    let tail = match material.trace.origin_of(&verdict, &material.options) {
        Some(origin) => format!(" ({origin})"),
        None => next_step(&verdict),
    };
    format!(
        "    {}{}{}",
        color("\x1b[2m", options.presentation.color),
        fold(&format!("{verdict}{tail}")),
        color("\x1b[0m", options.presentation.color)
    )
}

/// Write that line under the comment it is about, when the run has the
/// material to account for it.
fn explain_comment(
    output: &mut impl Write,
    file: &ProcessedFile,
    comment: &Comment,
    explainer: Option<&Explainer<'_>>,
    options: &RenderOptions,
) -> Result<()> {
    let Some(explainer) = explainer else {
        return Ok(());
    };
    wrote(writeln!(
        output,
        "{}",
        explanation_line(file, comment, explainer, options)
    ))
}

/// How to overrule a built-in rule, which no setting decided and no table can
/// be pointed at for.
fn next_step(verdict: &DispositionExplanation) -> String {
    match verdict {
        DispositionExplanation::ProtectedPreamble => {
            "; add --force-protected to remove it".to_owned()
        }
        DispositionExplanation::KeptHtml => format!(
            "; use --remove-kind {} or --policy all to remove it",
            CommentKind::HtmlComment
        ),
        DispositionExplanation::KeptDirective { kind, .. } => {
            format!("; use --remove-kind {kind} or --policy all to remove it")
        }
        _ => String::new(),
    }
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
    truncate(
        fold(&String::from_utf8_lossy(&source[start..end])),
        max_columns,
    )
}

/// The same treatment for a line that did not come out of a source file.
///
/// What an external tool on `PATH` says about itself is untrusted for exactly
/// the reason a comment is: `doctor` prints it to the same terminal, and a
/// tool planted there could otherwise clear the screen or repaint the report
/// from its own version line.
pub(crate) fn sanitize_line(text: &str) -> String {
    truncate(fold(text), PREVIEW_COLUMNS)
}

/// The same treatment for a line that must not be cut short.
///
/// A directory name is chosen by whoever made the directory, so the rows
/// `doctor` prints one on are untrusted for the same reason a version line is.
/// What they are not is commentary: an absolute path is easily longer than a
/// comment preview may be, and a row that ends in an ellipsis where the reader
/// was looking for the rest of the path answers nothing. Only the width cap is
/// dropped; every control character is still replaced.
pub(crate) fn sanitize_path(text: &str) -> String {
    fold(text)
}

/// The same treatment for a line of source a prompt has to show as code.
///
/// A hunk is read for its shape as much as for its text — indentation says
/// what a line belongs to — so unlike a comment preview this one keeps the
/// spaces it was given and expands a tab onto the same eight-column stop the
/// `columns` layout measures a replacement by. What it does not keep is
/// anything that drives the terminal: every control character, `ESC` and the
/// bidirectional overrides above all, still becomes U+FFFD, and the result is
/// still one line cut to a fixed width, because the question underneath it has
/// to stay on the screen with it.
pub(crate) fn sanitize_source_line(text: &str) -> String {
    let mut line = String::with_capacity(text.len());
    let mut column = 0usize;
    for character in text.chars() {
        if character == '\t' {
            let width = TAB_WIDTH - (column % TAB_WIDTH);
            line.extend(std::iter::repeat_n(' ', width));
            column += width;
        } else if is_control(character) {
            line.push('\u{fffd}');
            column += 1;
        } else {
            line.push(character);
            column += columns(character);
        }
    }
    truncate(line, PREVIEW_COLUMNS)
}

/// The tab stop `sanitize_source_line` expands to, the one the `columns`
/// layout already measures a tab by.
const TAB_WIDTH: usize = 8;

/// Fold `text` onto one control-free line.
fn fold(text: &str) -> String {
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
    folded
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
    render_explained(files, skipped, options, &Explanations::new())
}

/// The same report, with the material `--explain` needs for the files it has
/// it for. A file with none is reported exactly as `render` reports it.
pub fn render_explained(
    files: &[ProcessedFile],
    skipped: &[SkippedFile],
    options: &RenderOptions,
    explanations: &Explanations,
) -> Result<()> {
    let mut output = stdout();
    match options.format {
        OutputFormat::Human => render_human(&mut output, files, skipped, options, explanations),
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
    explanations: &Explanations,
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
        let explainer = options
            .explain
            .then(|| explanations.get(&file.path))
            .flatten()
            .map(Explainer::new);
        let explainer = explainer.as_ref();
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
                explain_comment(output, file, comment, explainer, options)?;
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
            // `check` reports what it would remove. Asked to explain itself it
            // reports the rest too, because a comment it left alone is exactly
            // the one the reader is asking about.
            for comment in &file.result.report.comments {
                let removable = comment.disposition.is_remove();
                if !options.explain && !removable {
                    continue;
                }
                let (line, column) = line_column(&file.source, comment.span.start);
                wrote(writeln!(
                    output,
                    "{}:{line}:{column}: {}{}{}{}",
                    display_path(&file.path, presentation.hyperlinks),
                    color(
                        if removable { "\x1b[33m" } else { "\x1b[32m" },
                        presentation.color
                    ),
                    if removable {
                        removable_label(comment.kind)
                    } else {
                        kept_prefix(comment.kind)
                    },
                    color("\x1b[0m", presentation.color),
                    preview_suffix(&file.source, comment.span, options)
                ))?;
                explain_comment(output, file, comment, explainer, options)?;
            }
        }
    }
    let skips = skip_lines(skipped, presentation, options.verbosity);
    // `diff` keeps standard output for the patch alone, so the skips it met
    // are left to standard error. `fix --dry-run` is that same `diff` speaking
    // for the `fix` it stands in for: a skipped path can be the whole answer
    // to the run, so the preview still owes the reader the reason — but beside
    // the summary that counts it, because what the preview promises on
    // standard output is a patch that has to survive being piped into `git
    // apply`. A plain `fix` writes no patch and keeps its skips there.
    if operation != Operation::Diff {
        for line in &skips {
            wrote(writeln!(output, "{line}"))?;
        }
    }
    // The findings are on standard output and the commentary that follows is
    // on standard error; a terminal sees both, so the buffer is emptied first
    // to keep the report in the order it was written.
    finish(output)?;
    let stderr = io::stderr();
    let mut report = stderr.lock();
    if operation == Operation::Diff && options.dry_run {
        for line in &skips {
            note(&mut report, line)?;
        }
    }
    if quiet {
        return Ok(());
    }
    let summary = Summary::compute(files, skipped, operation);
    let folded = !verbose && skipped.iter().any(|item| !item.error && !item.explicit);
    if verbose && let Some(line) = kind_breakdown(files, options) {
        note(&mut report, &line)?;
    }
    note(&mut report, &summary_report(&summary, options, folded))?;
    // Under any other policy a kept preamble is one of many deliberate keeps
    // and saying so every run would be noise. `all` said it would take
    // everything, so what it left behind is the surprise worth a line.
    if options.policy == Policy::All {
        let protected = protected_preambles(files);
        if protected > 0 {
            // The line counts what it kept, so the pronoun that stands for it
            // has to agree with that count.
            let pronoun = if protected == 1 { "it" } else { "them" };
            note(
                &mut report,
                &format!(
                    "{} kept; add --force-protected to remove {pronoun}.",
                    comments(protected, "protected preamble")
                ),
            )?;
        }
    }
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

/// The skips one run has to name, in one wording for whichever stream ends up
/// carrying them. An I/O error is named however quiet the run was asked to be:
/// it is a failure, not commentary.
///
/// Shared with `fix --interactive`, which writes no report of its own and would
/// otherwise be the one command that never says why it passed a file over.
pub(crate) fn skip_lines(
    skipped: &[SkippedFile],
    presentation: Presentation,
    verbosity: Verbosity,
) -> Vec<String> {
    let quiet = verbosity == Verbosity::Quiet;
    let verbose = verbosity == Verbosity::Verbose;
    skipped
        .iter()
        .filter(|item| item.error || (!quiet && (item.explicit || verbose)))
        .map(|item| {
            format!(
                "{}: {}: {}",
                display_path(&item.path, presentation.hyperlinks),
                if item.error { "error" } else { "skipped" },
                item.reason
            )
        })
        .collect()
}

/// The numbers an interactive run's verdict is built from.
///
/// They count answers rather than findings, which is the one thing the ordinary
/// summary cannot say: it counts what a run *could* have removed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct InteractiveOutcome {
    /// Comments the reader accepted for removal.
    pub removed: usize,
    /// Questions the reader answered. `a` and `d` answer for every remaining
    /// comment in their file, so those count here too.
    pub reviewed: usize,
    /// Comments the run had to offer, whether or not it got as far as asking.
    pub offered: usize,
    /// Files an accepted removal is written to.
    pub changed: usize,
    /// Files the run scanned.
    pub scanned: usize,
}

/// What an interactive run came to, in the vocabulary every other summary uses.
///
/// A run with nothing to offer borrows the wording the plain `fix` summary
/// gives the same answer, because the only number worth reporting there is how
/// much was looked at. A run stopped by `q` is counted against the questions it
/// actually asked, and says how many it never got to: measuring the acceptances
/// against every comment the run *could* have offered would read as a pile of
/// refusals nobody made.
///
/// Either way the verdict closes on the `(N files scanned)` every other summary
/// ends with. Answering questions about three files says nothing about how many
/// were opened to find them, and that is the number a reader checks a run
/// against.
pub(crate) fn interactive_summary(outcome: InteractiveOutcome) -> String {
    if outcome.offered == 0 {
        return format!("Nothing to fix in {}.", plural(outcome.scanned, "file"));
    }
    let unreviewed = outcome.offered.saturating_sub(outcome.reviewed);
    let tail = if unreviewed == 0 {
        String::new()
    } else {
        format!(" ({} not reviewed)", comments(unreviewed, ""))
    };
    format!(
        "Removed {} of {} in {}{tail} ({} scanned).",
        outcome.removed,
        comments(outcome.reviewed, ""),
        plural(outcome.changed, "file"),
        plural(outcome.scanned, "file")
    )
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

pub(crate) fn color(code: &'static str, enabled: bool) -> &'static str {
    if enabled { code } else { "" }
}

/// The path half of a report line, and the hyperlink wrapped around it.
///
/// A file name is chosen by whoever made the file, so the shown half is
/// untrusted input on its way to a terminal exactly like the preview beside
/// it, and gets `sanitize_path`'s treatment: one line, no control characters,
/// and no width cap, because a path cut to an ellipsis names no file. The
/// link *target* is a URL rather than terminal text and keeps the
/// percent-encoding it has always had.
fn display_path(path: &Path, hyperlinks: bool) -> String {
    let display = sanitize_path(&path.display().to_string());
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

/// Where a SARIF reader is sent to learn what the tool itself is.
const TOOL_INFORMATION_URI: &str = "https://github.com/P4suta/OComment";

/// Where a rule about a comment sends a reader asking why that comment is
/// reported — and why the one beside it is not.
const KIND_HELP_URI: &str = "https://github.com/P4suta/OComment#why-was-this-comment-kept";

/// The base id a path under the directory the run walked is reported against.
/// SARIF readers, GitHub code scanning among them, resolve `%SRCROOT%` to the
/// root of the checkout.
const SRCROOT: &str = "%SRCROOT%";

/// The one sentence every scan diagnostic is described by. The codes are as
/// varied as the languages that raise them, and the result carries the message
/// that says what was actually met.
const DIAGNOSTIC_DESCRIPTION: &str =
    "A problem OComment met while scanning the file; the message on the result says what it was.";

/// The spelling a machine format reports a path under.
///
/// A SARIF `artifactLocation.uri` and the `file=` of a GitHub annotation are
/// both matched against the paths the repository uses, so a reported path is
/// spelled the way the repository spells it: forward slashes on every
/// platform, and none of the `.` segments a walk root or a typed target leaves
/// behind — `sub/./doc.rs` names a file no checkout has. What a relative path
/// is measured *from* is said separately, by [`artifact_location`].
fn report_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let trimmed: Vec<&str> = text.split('/').filter(|segment| *segment != ".").collect();
    if trimmed.is_empty() {
        // The path was `.` (or `./`) and naming nothing at all would be worse
        // than naming the directory.
        return text;
    }
    trimmed.join("/")
}

/// The SARIF `artifactLocation` for a reported path.
///
/// A path under the directory the run started in is reported against
/// `%SRCROOT%`: SARIF resolves a relative URI against a base id, and a reader
/// given none has nothing to resolve it against, so the finding lands on no
/// file. An absolute path is not under the checkout as far as the run can
/// tell, one that climbs out through `..` has left it, and the pseudo-path
/// standard input is reported under is not a file at all — each of those is
/// reported as it stands, with no base id claiming otherwise.
fn artifact_location(path: &Path) -> Value {
    let uri = report_uri(path);
    if under_source_root(path) {
        json!({"uri": uri, "uriBaseId": SRCROOT})
    } else {
        json!({"uri": uri})
    }
}

fn under_source_root(path: &Path) -> bool {
    path != Path::new(STDIN_PATH)
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        && path
            .components()
            .any(|component| matches!(component, Component::Normal(_)))
}

/// The rules of one SARIF run, and the index each result points at.
///
/// A result names its rule twice: by `ruleId`, and by the position of that
/// rule's description in `tool.driver.rules`. A code-scanning UI shows a
/// finding through that description — its title, the sentence under it, and
/// the link it offers — so handing out the id and the index together is what
/// keeps a result from pointing at a description that is not there.
///
/// Every comment kind is described whether or not the run met one, because the
/// rules a tool reports are also read as the list of what it can find. The
/// rest — a scan diagnostic, a skipped file, a file that could not be read —
/// are described as the run meets them.
struct SarifRules {
    entries: Vec<Value>,
    indices: BTreeMap<String, usize>,
}

impl SarifRules {
    fn new() -> Self {
        let mut rules = Self {
            entries: Vec::new(),
            indices: BTreeMap::new(),
        };
        for kind in CommentKind::ALL {
            rules.describe(
                &format!("removable-{kind}"),
                "note",
                &format!("Removable {kind} comment"),
                &format!(
                    "A {kind} comment OComment can remove without changing what the file does."
                ),
                KIND_HELP_URI,
            );
        }
        rules
    }

    /// The index of the rule `id`, describing it first if this run has not
    /// reported it before.
    fn describe(&mut self, id: &str, level: &str, short: &str, full: &str, help: &str) -> usize {
        if let Some(&index) = self.indices.get(id) {
            return index;
        }
        let index = self.entries.len();
        self.entries.push(json!({
            "id": id,
            "shortDescription": {"text": short},
            "fullDescription": {"text": full},
            "helpUri": help,
            "defaultConfiguration": {"level": level},
        }));
        self.indices.insert(id.to_owned(), index);
        index
    }

    fn kind(&mut self, kind: CommentKind) -> usize {
        let id = format!("removable-{kind}");
        *self
            .indices
            .get(&id)
            .expect("every comment kind is described")
    }
}

/// A kebab-cased code read back as the title of a rule:
/// `unterminated-comment` is `Unterminated comment`.
fn sentence_case(code: &str) -> String {
    let spelled = code.replace('-', " ");
    let mut characters = spelled.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => spelled,
    }
}

fn render_sarif(
    output: &mut impl Write,
    files: &[ProcessedFile],
    skipped: &[SkippedFile],
) -> Result<()> {
    let mut rules = SarifRules::new();
    let mut results = Vec::new();
    for file in files {
        let location = artifact_location(&file.path);
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
            let index = rules.kind(comment.kind);
            results.push(json!({
                "ruleId": format!("removable-{kind}"),
                "ruleIndex": index,
                "level": "note",
                "message": {"text": removable_label(comment.kind)},
                "locations": [{"physicalLocation": {
                    "artifactLocation": location.clone(),
                    "region": {"startLine": line, "startColumn": column,
                        "endLine": end_line, "endColumn": end_column}
                }}],
                "fixes": [{
                    "description": {"text": "Remove comment with OComment"},
                    "artifactChanges": [{
                        "artifactLocation": location.clone(),
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
            let index = rules.describe(
                &diagnostic.code,
                level,
                &sentence_case(&diagnostic.code),
                DIAGNOSTIC_DESCRIPTION,
                TOOL_INFORMATION_URI,
            );
            results.push(json!({
                "ruleId": diagnostic.code,
                "ruleIndex": index,
                "level": level,
                "message": {"text": diagnostic.message},
                "locations": [{"physicalLocation": {
                    "artifactLocation": location.clone(),
                    "region": {"startLine": line, "startColumn": column,
                        "endLine": end_line, "endColumn": end_column}
                }}]
            }));
        }
    }
    for item in skipped {
        let (id, level, short, full) = if item.error {
            (
                "io-error",
                "error",
                "File could not be read",
                "A file OComment could not read or write; the message on the result carries the operating-system error.",
            )
        } else {
            (
                "skipped-file",
                "note",
                "Skipped file",
                "A file OComment did not scan; the message on the result says why it was left alone.",
            )
        };
        let index = rules.describe(id, level, short, full, TOOL_INFORMATION_URI);
        results.push(json!({
            "ruleId": id,
            "ruleIndex": index,
            "level": level,
            "message": {"text": item.reason},
            "locations": [{"physicalLocation": {
                "artifactLocation": artifact_location(&item.path)
            }}]
        }));
    }
    let sarif = json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{"tool": {"driver": {
            "name": "ocomment",
            "version": env!("CARGO_PKG_VERSION"),
            "informationUri": TOOL_INFORMATION_URI,
            "rules": rules.entries
        }}, "results": results}]
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
                github_escape(&report_uri(&file.path)),
                removable_label(comment.kind)
            ))?;
        }
        for diagnostic in &file.result.report.diagnostics {
            let (line, column) = line_column(&file.source, diagnostic.span.start);
            wrote(writeln!(
                output,
                "::error file={},line={line},col={column},title={}::{}",
                github_escape(&report_uri(&file.path)),
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
            github_escape(&report_uri(&item.path)),
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

pub(crate) fn line_column(source: &[u8], offset: usize) -> (usize, usize) {
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

    /// The reported path is read by a machine that has to find the file again:
    /// GitHub matches an annotation by `file=`, and a SARIF reader resolves
    /// `artifactLocation.uri` against the checkout. A Windows separator and a
    /// `.` segment both name a file no checkout has.
    #[test]
    fn report_uri_spells_a_path_the_way_a_repository_does() {
        assert_eq!(report_uri(Path::new("./a.rs")), "a.rs");
        assert_eq!(report_uri(Path::new("sub/./doc.rs")), "sub/doc.rs");
        assert_eq!(report_uri(Path::new("./sub/./doc.rs")), "sub/doc.rs");
        assert_eq!(report_uri(Path::new(r"sub\doc.rs")), "sub/doc.rs");
        assert_eq!(report_uri(Path::new(r".\sub\.\doc.rs")), "sub/doc.rs");
        // A path that leaves the tree, an absolute one, and standard input are
        // all left as they are; only the separators are normalised.
        assert_eq!(report_uri(Path::new("../sibling/a.rs")), "../sibling/a.rs");
        assert_eq!(report_uri(Path::new("/tmp/a.rs")), "/tmp/a.rs");
        assert_eq!(report_uri(Path::new(r"C:\src\a.rs")), "C:/src/a.rs");
        assert_eq!(report_uri(Path::new(STDIN_PATH)), STDIN_PATH);
        // Naming the working directory as nothing at all would be worse.
        assert_eq!(report_uri(Path::new(".")), ".");
    }

    /// `%SRCROOT%` says the path is measured from the root of the checkout, so
    /// it is claimed only for the paths that are.
    #[test]
    fn only_a_path_inside_the_tree_is_reported_against_the_source_root() {
        for inside in ["a.rs", "sub/doc.rs", "./sub/doc.rs"] {
            assert_eq!(
                artifact_location(Path::new(inside))["uriBaseId"],
                json!(SRCROOT),
                "`{inside}` is not reported against the source root"
            );
        }
        for outside in ["../sibling/a.rs", "/tmp/a.rs", STDIN_PATH] {
            let location = artifact_location(Path::new(outside));
            assert_eq!(
                location.get("uriBaseId"),
                None,
                "`{outside}` claims to be under the source root"
            );
        }
    }

    /// Every result points into the rules by index, so the two orders have to
    /// be the same one.
    #[test]
    fn a_rule_is_described_once_and_keeps_its_index() {
        let mut rules = SarifRules::new();
        assert_eq!(rules.entries.len(), CommentKind::ALL.len());
        assert_eq!(rules.kind(CommentKind::Line), 0);
        let first = rules.describe("io-error", "error", "short", "full", TOOL_INFORMATION_URI);
        assert_eq!(first, CommentKind::ALL.len());
        let again = rules.describe("io-error", "note", "other", "other", TOOL_INFORMATION_URI);
        assert_eq!(first, again, "a second sighting described the rule twice");
        assert_eq!(
            rules.entries[first]["defaultConfiguration"]["level"],
            "error"
        );
        assert_eq!(rules.entries.len(), CommentKind::ALL.len() + 1);
    }

    #[test]
    fn a_diagnostic_code_reads_back_as_a_title() {
        assert_eq!(
            sentence_case("unterminated-comment"),
            "Unterminated comment"
        );
        assert_eq!(sentence_case("nesting-limit"), "Nesting limit");
        assert_eq!(sentence_case(""), "");
    }

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

    /// A hunk is read as code, so the indentation that says what a line belongs
    /// to survives — but nothing that drives the terminal does, because the
    /// prompt asking about that line sits directly underneath it.
    #[test]
    fn a_source_line_keeps_its_shape_and_loses_its_control_characters() {
        assert_eq!(
            sanitize_source_line("    let x = 1; // note"),
            "    let x = 1; // note",
            "the indentation of a shown line was collapsed"
        );
        assert_eq!(
            sanitize_source_line("\tif (x) {"),
            "        if (x) {",
            "a tab did not reach its eight-column stop"
        );
        assert_eq!(
            sanitize_source_line("a\u{1b}[2Jb\u{202e}c"),
            "a\u{fffd}[2Jb\u{fffd}c",
            "an escape sequence reached the terminal verbatim"
        );
        let capped = sanitize_source_line(&"v".repeat(PREVIEW_COLUMNS * 3));
        let width: usize = capped
            .chars()
            .map(|ch| unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0))
            .sum();
        assert!(
            width <= PREVIEW_COLUMNS,
            "a shown line ran to {width} columns and pushed the question off the screen"
        );
    }

    /// The interactive verdict counts answers, and every noun agrees with the
    /// number in front of it. It closes on the same `(N files scanned)` the
    /// plain `fix` summary ends with: the reader still has to be told how much
    /// was looked at to reach the answers.
    #[test]
    fn the_interactive_summary_pluralizes_both_of_its_nouns() {
        assert_eq!(
            interactive_summary(InteractiveOutcome {
                removed: 1,
                reviewed: 1,
                offered: 1,
                changed: 1,
                scanned: 1,
            }),
            "Removed 1 of 1 comment in 1 file (1 file scanned)."
        );
        assert_eq!(
            interactive_summary(InteractiveOutcome {
                removed: 2,
                reviewed: 5,
                offered: 5,
                changed: 3,
                scanned: 4,
            }),
            "Removed 2 of 5 comments in 3 files (4 files scanned)."
        );
    }

    /// A run that was never asked a question says so in the vocabulary the
    /// plain `fix` summary uses for the same answer, and counts the files it
    /// scanned — `Removed 0 of 0 comments in 0 files` named three numbers, none
    /// of which was the one the reader wanted.
    #[test]
    fn an_interactive_run_with_nothing_to_offer_borrows_the_fix_wording() {
        assert_eq!(
            interactive_summary(InteractiveOutcome {
                scanned: 3,
                ..InteractiveOutcome::default()
            }),
            "Nothing to fix in 3 files."
        );
        assert_eq!(
            interactive_summary(InteractiveOutcome {
                scanned: 1,
                ..InteractiveOutcome::default()
            }),
            "Nothing to fix in 1 file."
        );
    }

    /// `q` stops the questions, so the verdict counts the ones that were
    /// answered and says how many were left unasked. Reporting `1 of 9` to a
    /// reader who answered twice would read as seven refusals.
    #[test]
    fn a_stopped_interactive_run_counts_the_questions_it_asked() {
        assert_eq!(
            interactive_summary(InteractiveOutcome {
                removed: 1,
                reviewed: 2,
                offered: 9,
                changed: 1,
                scanned: 4,
            }),
            "Removed 1 of 2 comments in 1 file (7 comments not reviewed) (4 files scanned)."
        );
        assert_eq!(
            interactive_summary(InteractiveOutcome {
                removed: 0,
                reviewed: 1,
                offered: 2,
                changed: 0,
                scanned: 1,
            }),
            "Removed 0 of 1 comment in 0 files (1 comment not reviewed) (1 file scanned)."
        );
    }

    /// What a probed tool says about itself gets the preview's treatment: one
    /// line, no control sequences, and no more of it than a preview shows.
    #[test]
    fn sanitize_line_replaces_controls_and_caps_the_width() {
        assert_eq!(
            sanitize_line("\u{1b}[2J\u{1b}[1;31mv1.0\tPWNED\u{1b}[0m"),
            "\u{fffd}[2J\u{fffd}[1;31mv1.0 PWNED\u{fffd}[0m"
        );
        let capped = sanitize_line(&"v".repeat(PREVIEW_COLUMNS * 3));
        let width: usize = capped
            .chars()
            .map(|ch| unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0))
            .sum();
        assert!(
            width <= PREVIEW_COLUMNS,
            "`{capped}` is {width} columns wide"
        );
        assert!(capped.ends_with('\u{2026}'), "truncation is unmarked");
    }

    #[test]
    fn preview_reads_only_the_span() {
        let source = b"let x = 1; // TODO remove\n";
        assert_eq!(preview(source, ByteSpan::new(11, 25), 72), "// TODO remove");
    }
}
