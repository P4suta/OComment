//! The comment-by-comment prompt behind `fix --interactive`.
//!
//! The run has already transformed every file by the time this module is
//! reached, so what it asks about is a list of edits that were computed
//! together. Applying only some of them is safe because a replacement is
//! computed from the *source* alone: under `layout = "columns"` it is exactly
//! as wide as the comment it stands for, so a removal moves nothing that comes
//! after it, and under every other layout it depends only on the bytes either
//! side of its own span. `partial_column_edits_keep_the_replacement_the_transform_computed`
//! pins that.
//!
//! The prompt is line-based on purpose: no raw mode, no cursor addressing, no
//! terminal library. One question, one line of answer, and a transcript that a
//! test can read.

use crate::{
    atomic::WritePlan,
    output::{
        LineIndex, Presentation, ProcessedFile, color, sanitize_path, sanitize_source_line, wrote,
    },
};
use anyhow::Result;
use ocomment_core::{Comment, Edit, apply_edits};
use std::{
    borrow::Cow,
    io::{BufRead, Write},
};

/// What the reader decided about the removable comments of one run.
///
/// Deliberately not `Debug`: a plan carries the whole before-and-after text of
/// a source file, and the one thing this type must never do is put it on a
/// terminal by accident.
#[derive(Default)]
pub struct Selection<'a> {
    /// One plan per file that keeps at least one accepted removal.
    pub plans: Vec<WritePlan<'a>>,
    pub accepted: usize,
    pub declined: usize,
    /// The reader asked for the run to write nothing at all.
    pub aborted: bool,
}

/// The question, ending in a space rather than a newline so the answer is typed
/// on the same line.
const PROMPT: &str = "Remove? [y,n,a,d,q,x,?] ";

/// What each answer does, in the order the prompt lists them.
const HELP: [&str; 7] = [
    "y - remove this comment",
    "n - keep it",
    "a - remove it and every remaining comment in this file",
    "d - keep it and every remaining comment in this file",
    "q - stop asking and apply the removals accepted so far",
    "x - abort; write nothing",
    "? - show this help",
];

/// What is said to an answer that is not one of them. A typo is never taken for
/// a decision about somebody's source file.
const UNKNOWN: &str = "unknown answer; press ? for help";

/// How many unchanged lines are shown either side of the change.
const CONTEXT: usize = 3;

/// Ask about every comment this run would remove and collect the answers into
/// the writes they come to.
///
/// `input` and `output` are the reader's terminal; they are parameters so the
/// whole conversation can be driven from a script in a test.
pub fn select<'a>(
    files: &'a [ProcessedFile],
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    presentation: &Presentation,
) -> Result<Selection<'a>> {
    let total: usize = files.iter().map(|file| file.result.edits.len()).sum();
    let mut selection = Selection::default();
    let mut position = 0usize;
    let mut stopped = false;
    for file in files {
        let items = offers(file);
        if items.is_empty() {
            continue;
        }
        let mut accepted: Vec<Edit> = Vec::new();
        let lines = LineIndex::new(&file.source);
        // NOTE: The answer `a` or `d` left standing for the rest of this file.
        let mut standing: Option<bool> = None;
        for (index, item) in items.iter().enumerate() {
            let (comment, edit) = *item;
            position += 1;
            let remove = match standing {
                Some(answer) => answer,
                None => {
                    let place = Place {
                        index: index + 1,
                        of: items.len(),
                        position,
                        total,
                    };
                    show(output, file, comment, edit, place, &lines, presentation)?;
                    match ask(input, output, presentation)? {
                        Answer::Yes => true,
                        Answer::No => false,
                        Answer::AllInFile => {
                            standing = Some(true);
                            true
                        }
                        Answer::NoneInFile => {
                            standing = Some(false);
                            false
                        }
                        Answer::Stop => {
                            stopped = true;
                            break;
                        }
                        /* NOTE: Everything accepted so far goes with it: `x` is the
                         * answer for a run that should never have started. */
                        Answer::Abort => {
                            return Ok(Selection {
                                aborted: true,
                                ..Selection::default()
                            });
                        }
                        Answer::Help => unreachable!("`ask` answers `?` itself"),
                    }
                }
            };
            if remove {
                selection.accepted += 1;
                accepted.push(edit.clone());
            } else {
                selection.declined += 1;
            }
        }
        if !accepted.is_empty() {
            let replacement = apply_edits(&file.source, &accepted);
            if replacement != file.source {
                selection.plans.push(WritePlan {
                    path: file.path.clone(),
                    original: Cow::Borrowed(&file.source),
                    replacement: Cow::Owned(replacement),
                });
            }
        }
        if stopped {
            break;
        }
    }
    Ok(selection)
}

/// The comments this run would remove, each with the edit that removes it.
///
/// `transform` pushes exactly one edit per removable comment, in source order,
/// so the two lists line up pairwise. A file whose source failed to scan has no
/// edits at all, and nothing about it is offered — the same gate a
/// non-interactive `fix` applies before it writes.
fn offers(file: &ProcessedFile) -> Vec<(&Comment, &Edit)> {
    file.result
        .report
        .comments
        .iter()
        .filter(|comment| comment.disposition.is_remove())
        .zip(file.result.edits.iter())
        .collect()
}

/// Where one question sits, in its file and in the run.
struct Place {
    index: usize,
    of: usize,
    position: usize,
    total: usize,
}

/// Write the question's heading and the hunk it is about.
fn show(
    output: &mut dyn Write,
    file: &ProcessedFile,
    comment: &Comment,
    edit: &Edit,
    place: Place,
    lines: &LineIndex,
    presentation: &Presentation,
) -> Result<()> {
    let (line, column) = lines.line_column(comment.span.start);
    wrote(writeln!(
        output,
        "{}:{line}:{column}  {} comment  ({} of {} in file, {} of {} total)",
        sanitize_path(&file.path.display().to_string()),
        comment.kind,
        place.index,
        place.of,
        place.position,
        place.total
    ))?;
    for row in hunk(&file.source, edit, presentation) {
        wrote(writeln!(output, "{row}"))?;
    }
    Ok(())
}

/// The lines the reader is answering for: the ones the comment sits on as they
/// are, the same ones as this single edit would leave them, and `CONTEXT` lines
/// of unchanged source either side.
///
/// The "after" text is produced by applying this one edit and nothing else, so
/// what is shown is what answering `y` to this question alone would do.
fn hunk(source: &[u8], edit: &Edit, presentation: &Presentation) -> Vec<String> {
    let length = source.len();
    let begin = edit.span.start.min(length);
    let finish = edit.span.end.clamp(begin, length);
    let start = line_start(source, begin);
    /* NOTE: The last byte the span covers, so a span that ends exactly on a line
     * break does not drag the following line into the hunk. */
    let inner = if finish > begin { finish - 1 } else { begin };
    let end = line_end(source, inner);
    let after = apply_edits(source, std::slice::from_ref(edit));
    let removed = finish - begin;
    let moved = if edit.replacement.len() >= removed {
        end.saturating_add(edit.replacement.len() - removed)
    } else {
        end.saturating_sub(removed - edit.replacement.len())
    }
    .clamp(start, after.len());

    let mut rows = Vec::new();
    for line in preceding(source, start, CONTEXT) {
        rows.push(rendered(' ', line, presentation));
    }
    changed(&mut rows, '-', &rows_of(&source[start..end]), presentation);
    changed(
        &mut rows,
        '+',
        &collapse_blanks(rows_of(&after[start..moved])),
        presentation,
    );
    for line in following(source, end, CONTEXT) {
        rows.push(rendered(' ', line, presentation));
    }
    rows
}

/// How many lines of one changed side are shown before the rest are folded
/// into a single marker: `CONTEXT` at each end, the same window the unchanged
/// context gets.
const BLOCK: usize = 2 * CONTEXT;

/// One side of the change, capped so a comment taller than the screen cannot
/// push the question off it.
///
/// A block comment can run to any length, and the reader is answering about
/// the comment, not reading it here: the first and last `CONTEXT` lines say
/// which comment it is and where it ends, and the marker between them says how
/// much was left out rather than pretending there was nothing.
fn changed(rows: &mut Vec<String>, marker: char, lines: &[&[u8]], presentation: &Presentation) {
    let show = |rows: &mut Vec<String>, block: &[&[u8]]| {
        rows.extend(
            block
                .iter()
                .map(|line| rendered(marker, line, presentation)),
        );
    };
    if lines.len() <= BLOCK {
        show(rows, lines);
        return;
    }
    show(rows, &lines[..CONTEXT]);
    rows.push(elision(marker, lines.len() - BLOCK, presentation));
    show(rows, &lines[lines.len() - CONTEXT..]);
}

/// What stands in for the lines a capped side folded away. It carries the
/// marker of the side it belongs to so the two columns stay aligned, and is
/// dimmed rather than tinted so it is never read as a line of the source.
fn elision(marker: char, hidden: usize, presentation: &Presentation) -> String {
    format!(
        "{}{marker}... {hidden} more line{} ...{}",
        color("\x1b[2m", presentation.color),
        if hidden == 1 { "" } else { "s" },
        color("\x1b[0m", presentation.color)
    )
}

/// Runs of the same blank line folded to one.
///
/// Under `layout = "lines"` a removed block comment leaves exactly as many
/// empty lines as it occupied, and the twenty-seventh of them tells the reader
/// nothing the first did not.
fn collapse_blanks(lines: Vec<&[u8]>) -> Vec<&[u8]> {
    let mut kept: Vec<&[u8]> = Vec::with_capacity(lines.len());
    for line in lines {
        let blank = line.iter().all(u8::is_ascii_whitespace);
        if blank && kept.last() == Some(&line) {
            continue;
        }
        kept.push(line);
    }
    kept
}

/// One line of the hunk: its marker, its terminal-safe text, and the colour
/// that says which of the three it is.
fn rendered(marker: char, line: &[u8], presentation: &Presentation) -> String {
    let tint = match marker {
        '-' => "\x1b[31m",
        '+' => "\x1b[32m",
        _ => "\x1b[2m",
    };
    format!(
        "{}{marker}{}{}",
        color(tint, presentation.color),
        sanitize_source_line(&String::from_utf8_lossy(line)),
        color("\x1b[0m", presentation.color)
    )
}

/// One block of bytes as the lines it holds, with the carriage return of a
/// CRLF file left out of the text rather than shown as a control character.
fn rows_of(block: &[u8]) -> Vec<&[u8]> {
    block
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .collect()
}

/// The start of the line byte `offset` falls on.
fn line_start(source: &[u8], offset: usize) -> usize {
    source[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1)
}

/// The end of the line byte `offset` falls on, before its terminator.
fn line_end(source: &[u8], offset: usize) -> usize {
    source[offset..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(source.len(), |at| offset + at)
}

/// Up to `count` whole lines ending just before `start`, in source order.
fn preceding(source: &[u8], start: usize, count: usize) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut at = start;
    while lines.len() < count && at > 0 {
        /* NOTE: `at` is a line start, so the byte before it is the terminator of the
         * line being collected. */
        let end = at - 1;
        let begin = line_start(source, end);
        lines.push(&source[begin..end]);
        at = begin;
    }
    lines.reverse();
    lines
}

/// Up to `count` whole lines starting just after `end`.
fn following(source: &[u8], end: usize, count: usize) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut at = end;
    while lines.len() < count && at < source.len() {
        /* NOTE: Step over the terminator `end` stopped in front of. A file whose last
         * line ends in one has nothing after it, and the loop ends here. */
        at += 1;
        if at >= source.len() {
            break;
        }
        let stop = line_end(source, at);
        lines.push(&source[at..stop]);
        at = stop;
    }
    lines
}

/// One decision about one comment.
enum Answer {
    Yes,
    No,
    AllInFile,
    NoneInFile,
    /// Stop asking and apply what was accepted.
    Stop,
    /// Throw the whole run away.
    Abort,
    Help,
}

/// Put the question and read one answer, explaining itself and asking again
/// until the reader gives one.
///
/// The answer is read as bytes rather than as a line of text: a terminal can
/// deliver anything, and a stray byte is a typo to ask about again, not an I/O
/// failure that ends a run somebody is in the middle of.
fn ask(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    presentation: &Presentation,
) -> Result<Answer> {
    loop {
        wrote(write!(output, "{PROMPT}"))?;
        /* NOTE: The question ends without a newline, so it has to be pushed out by
         * hand before the run blocks waiting for the answer to it. */
        wrote(output.flush())?;
        let mut line = Vec::new();
        /* NOTE: Nothing left to read is a reader who is no longer there to answer,
         * which is the one answer that must not be guessed at. */
        if input.read_until(b'\n', &mut line)? == 0 {
            return Ok(Answer::Abort);
        }
        match parse(&String::from_utf8_lossy(&line)) {
            Some(Answer::Help) => {
                for entry in HELP {
                    wrote(writeln!(output, "{}", dimmed(entry, presentation)))?;
                }
            }
            Some(answer) => return Ok(answer),
            None => wrote(writeln!(output, "{}", dimmed(UNKNOWN, presentation)))?,
        }
    }
}

/// Commentary beside the question, told apart from it by being dimmed.
fn dimmed(text: &str, presentation: &Presentation) -> String {
    format!(
        "{}{text}{}",
        color("\x1b[2m", presentation.color),
        color("\x1b[0m", presentation.color)
    )
}

/// The answer one typed line stands for, or `None` for anything else.
fn parse(line: &str) -> Option<Answer> {
    match line.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Some(Answer::Yes),
        "n" | "no" => Some(Answer::No),
        "a" => Some(Answer::AllInFile),
        "d" => Some(Answer::NoneInFile),
        "q" => Some(Answer::Stop),
        "x" => Some(Answer::Abort),
        "?" | "h" | "help" => Some(Answer::Help),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocomment_core::{Language, Layout, TransformOptions, apply_edits, transform};
    use std::{io::Cursor, path::PathBuf};

    /// Two removable block comments, one line, one file.
    const TWO: &str = "a/* one */b/* two */c\n";

    fn file(name: &str, text: &str) -> ProcessedFile {
        let source = text.as_bytes().to_vec();
        let result = transform(&source, Language::C, TransformOptions::default());
        ProcessedFile {
            path: PathBuf::from(name),
            source,
            language: Language::C,
            result: crate::output::ProcessedResult::complete(result),
        }
    }

    /// Drive `select` with a scripted answer per line and collect everything it
    /// wrote to the terminal.
    fn ask<'a>(files: &'a [ProcessedFile], script: &str) -> (Selection<'a>, String) {
        let mut input = Cursor::new(script.as_bytes().to_vec());
        let mut written: Vec<u8> = Vec::new();
        let selection = select(files, &mut input, &mut written, &Presentation::default()).unwrap();
        (selection, String::from_utf8(written).unwrap())
    }

    fn replacement(selection: &Selection<'_>) -> String {
        assert_eq!(selection.plans.len(), 1, "expected exactly one write plan");
        String::from_utf8(selection.plans[0].replacement.to_vec()).unwrap()
    }

    /// The answers apply to one comment each: the accepted span is gone and the
    /// declined one is still in the bytes that would be written.
    #[test]
    fn yes_and_no_apply_only_the_accepted_comment() {
        let files = [file("a.c", TWO)];
        let (selection, _) = ask(&files, "y\nn\n");
        assert_eq!((selection.accepted, selection.declined), (1, 1));
        assert!(!selection.aborted);
        assert_eq!(replacement(&selection), "a b/* two */c\n");
        assert_eq!(selection.plans[0].path, PathBuf::from("a.c"));
        assert_eq!(selection.plans[0].original, TWO.as_bytes());
    }

    /// The question says which comment it is about — where it starts, what kind
    /// it is, and how far through the file and the run it sits — and shows the
    /// line as it stands against the line the answer would leave behind.
    #[test]
    fn the_prompt_names_the_comment_and_shows_the_hunk() {
        let (_, transcript) = ask(&[file("a.c", TWO)], "y\nn\n");
        assert!(
            transcript.contains("a.c:1:2  block comment  (1 of 2 in file, 1 of 2 total)\n"),
            "the first question did not name its comment:\n{transcript}"
        );
        assert!(
            transcript.contains("a.c:1:12  block comment  (2 of 2 in file, 2 of 2 total)\n"),
            "the second question did not name its comment:\n{transcript}"
        );
        assert!(
            transcript.contains("-a/* one */b/* two */c\n+a b/* two */c\n"),
            "the first question did not show the line it would rewrite:\n{transcript}"
        );
        assert!(
            transcript.contains("-a/* one */b/* two */c\n+a/* one */b c\n"),
            "the second question did not show the line it would rewrite:\n{transcript}"
        );
        assert_eq!(
            transcript.matches("Remove? [y,n,a,d,q,x,?] ").count(),
            2,
            "one question was asked per comment:\n{transcript}"
        );
    }

    /// Three lines either side of the comment are shown unprefixed, so the
    /// reader can tell what the line is doing before answering for it.
    #[test]
    fn the_hunk_carries_three_lines_of_context_on_each_side() {
        let source = "1\n2\n3\n4\n5\nx/* c */y\n6\n7\n8\n9\n10\n";
        let (_, transcript) = ask(&[file("a.c", source)], "n\n");
        assert!(
            transcript.contains(" 3\n 4\n 5\n-x/* c */y\n+x y\n 6\n 7\n 8\n"),
            "the hunk is not three lines of context around the change:\n{transcript}"
        );
        assert!(
            !transcript.contains(" 2\n"),
            "the hunk reached a fourth line above the change:\n{transcript}"
        );
        assert!(
            !transcript.contains(" 9\n"),
            "the hunk reached a fourth line below the change:\n{transcript}"
        );
    }

    /// A block comment 27 lines tall, with a line of source either side.
    fn tall(lines: usize) -> String {
        let mut text = String::from("before\n/* comment line 1\n");
        for line in 2..lines {
            text.push_str(&format!(" * comment line {line}\n"));
        }
        text.push_str(&format!(" * comment line {lines} */\nafter\n"));
        text
    }

    /// A comment tall enough to fill the screen would push the question off it.
    /// Both sides of the change are capped at `CONTEXT` lines each end, with one
    /// marker standing for everything folded away, so the prompt stays in view.
    #[test]
    fn a_tall_hunk_is_capped_on_both_sides() {
        let source = tall(27);
        let (_, transcript) = ask(&[file("a.c", &source)], "n\n");
        let removed = transcript
            .lines()
            .filter(|line| line.starts_with('-'))
            .count();
        let added = transcript
            .lines()
            .filter(|line| line.starts_with('+'))
            .count();
        assert!(
            removed <= 2 * CONTEXT + 1,
            "the removed side printed {removed} lines:\n{transcript}"
        );
        assert!(
            added <= 2 * CONTEXT + 1,
            "the added side printed {added} lines:\n{transcript}"
        );
        assert!(
            transcript.contains("-/* comment line 1\n"),
            "the removed side lost the first line of the comment:\n{transcript}"
        );
        assert!(
            transcript.contains("- * comment line 27 */\n"),
            "the removed side lost the last line of the comment:\n{transcript}"
        );
        assert!(
            transcript.contains("more line"),
            "a capped hunk did not say how much it folded away:\n{transcript}"
        );
        assert!(
            transcript.contains("Remove? [y,n,a,d,q,x,?] "),
            "the question never arrived:\n{transcript}"
        );
    }

    /// A change that fits is shown whole: nothing is folded and nothing says it
    /// was.
    #[test]
    fn a_short_hunk_is_shown_whole() {
        let (_, transcript) = ask(&[file("a.c", TWO)], "n\nn\n");
        assert!(
            !transcript.contains("more line"),
            "a hunk that fits was capped anyway:\n{transcript}"
        );
        assert!(
            transcript.contains("-a/* one */b/* two */c\n+a b/* two */c\n"),
            "the whole change was not shown:\n{transcript}"
        );
    }

    /// `a` answers for the rest of the file at once and asks nothing more about
    /// it; the next file starts asking again.
    #[test]
    fn a_removes_the_rest_of_the_file_without_asking() {
        let files = [file("a.c", TWO), file("b.c", TWO)];
        let (selection, transcript) = ask(&files, "a\ny\nn\n");
        assert_eq!((selection.accepted, selection.declined), (3, 1));
        assert_eq!(selection.plans.len(), 2);
        assert_eq!(
            String::from_utf8(selection.plans[0].replacement.to_vec()).unwrap(),
            "a b c\n"
        );
        assert_eq!(
            String::from_utf8(selection.plans[1].replacement.to_vec()).unwrap(),
            "a b/* two */c\n"
        );
        assert_eq!(
            transcript.matches("Remove? [y,n,a,d,q,x,?] ").count(),
            3,
            "`a` kept asking about the file it answered for:\n{transcript}"
        );
    }

    /// `d` is the same for the other answer: nothing in the file is removed, so
    /// the file has no plan at all.
    #[test]
    fn d_keeps_the_rest_of_the_file_without_asking() {
        let files = [file("a.c", TWO), file("b.c", TWO)];
        let (selection, transcript) = ask(&files, "d\ny\nn\n");
        assert_eq!((selection.accepted, selection.declined), (1, 3));
        assert_eq!(selection.plans.len(), 1);
        assert_eq!(selection.plans[0].path, PathBuf::from("b.c"));
        assert_eq!(
            transcript.matches("Remove? [y,n,a,d,q,x,?] ").count(),
            3,
            "`d` kept asking about the file it answered for:\n{transcript}"
        );
    }

    /// `q` stops the run where it stands and keeps what was already accepted.
    #[test]
    fn q_applies_what_was_accepted_and_stops_asking() {
        let files = [file("a.c", TWO), file("b.c", TWO)];
        let (selection, transcript) = ask(&files, "y\nq\ny\n");
        assert_eq!(selection.accepted, 1);
        assert!(!selection.aborted);
        assert_eq!(replacement(&selection), "a b/* two */c\n");
        assert_eq!(selection.plans[0].path, PathBuf::from("a.c"));
        assert_eq!(
            transcript.matches("Remove? [y,n,a,d,q,x,?] ").count(),
            2,
            "`q` asked another question:\n{transcript}"
        );
    }

    /// `x` throws the run away, accepted answers included.
    #[test]
    fn x_writes_nothing() {
        let files = [file("a.c", TWO), file("b.c", TWO)];
        let (selection, _) = ask(&files, "y\nx\ny\n");
        assert!(selection.aborted);
        assert!(
            selection.plans.is_empty(),
            "an aborted run still produced something to write"
        );
    }

    /// A closed input is a reader who is no longer there to answer, which is
    /// the one answer that cannot be guessed at: it aborts.
    #[test]
    fn end_of_input_aborts_like_x() {
        let files = [file("a.c", TWO)];
        let (selection, _) = ask(&files, "y\n");
        assert!(selection.aborted, "the second question ran out of input");
        assert!(selection.plans.is_empty());
    }

    /// `?` is not an answer; it explains the answers and asks again.
    #[test]
    fn help_is_shown_and_the_question_repeated() {
        let files = [file("a.c", TWO)];
        let (selection, transcript) = ask(&files, "?\nn\nn\n");
        assert_eq!((selection.accepted, selection.declined), (0, 2));
        assert!(
            transcript.contains("a - remove it and every remaining comment in this file"),
            "`?` did not explain the answers:\n{transcript}"
        );
        assert_eq!(
            transcript.matches("Remove? [y,n,a,d,q,x,?] ").count(),
            3,
            "`?` did not ask the first question again:\n{transcript}"
        );
    }

    /// Anything else is a typo, not a decision, and is never taken for one.
    #[test]
    fn an_unknown_answer_re_prompts() {
        let files = [file("a.c", TWO)];
        let (selection, transcript) = ask(&files, "z\n\ny\nn\n");
        assert_eq!((selection.accepted, selection.declined), (1, 1));
        assert_eq!(replacement(&selection), "a b/* two */c\n");
        assert!(
            transcript.contains("unknown answer"),
            "an unknown answer went unremarked:\n{transcript}"
        );
        assert_eq!(
            transcript.matches("Remove? [y,n,a,d,q,x,?] ").count(),
            4,
            "an unknown answer did not ask again:\n{transcript}"
        );
    }

    /// Under `layout = "columns"` a removal is replaced by exactly as many
    /// display columns as the comment occupied, and every such replacement is
    /// measured from the *source*, not from whatever earlier removals left
    /// behind. That is what lets this command apply a subset of the edits a
    /// transform produced: a width-preserving replacement moves nothing, so
    /// each remaining comment still begins at the display column its own
    /// replacement was computed for.
    ///
    /// Pinned by transforming the partially edited bytes again and requiring
    /// the replacement to come out byte-identical to the one the full transform
    /// computed — the tab inside the second comment makes that replacement
    /// depend on the column it starts at.
    #[test]
    fn partial_column_edits_keep_the_replacement_the_transform_computed() {
        let source = b"x/* one */y/* a\tb */z\n";
        let options = TransformOptions {
            layout: Layout::Columns,
            ..TransformOptions::default()
        };
        let full = transform(source, Language::C, options.clone());
        assert_eq!(full.edits.len(), 2, "the fixture lost a comment");

        for taken in [0usize, 1] {
            let kept = 1 - taken;
            let partial = apply_edits(source, std::slice::from_ref(&full.edits[taken]));
            let again = transform(&partial, Language::C, options.clone());
            assert_eq!(
                again.edits.len(),
                1,
                "the partially edited source lost the comment that was kept"
            );
            assert_eq!(
                again.edits[0].replacement, full.edits[kept].replacement,
                "applying edit #{taken} on its own changed what edit #{kept} replaces"
            );
            assert_eq!(
                again.output, full.output,
                "applying edit #{taken} and then the rest is not the whole transform"
            );
        }

        let both: Vec<Edit> = full.edits.clone();
        assert_eq!(apply_edits(source, &both), full.output);
    }
}
