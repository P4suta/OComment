//! What to do about a comment, read from where it sits.
//!
//! A verdict says a comment may go. That is not the question its author has.
//! Theirs is "and then what" — and the answer is not the same for a line that
//! explains the function under it, a note wedged beside an assignment, and a
//! promise nobody kept. The engine cannot tell them apart because the verdict
//! does not depend on the difference; a reader deciding what to do can tell
//! them apart at a glance, from the bytes around the comment, and so can this.
//!
//! Nothing here changes a verdict. Every decision below is a rendering of a
//! comment the scanner already reported, and a comment whose situation is not
//! one this recognises gets the weakest advice rather than a guess.
//!
//! It lives in the CLI rather than in the engine on purpose. The two
//! implementations are held to each other over what they *decide*, and that is
//! where the cross-check earns its keep; advice decides nothing, so mirroring
//! it in OCaml would double the work and prove nothing.

use crate::output::{ProcessedFile, sanitize_source_line};
use ocomment_core::{Age, Comment, CommentKind, Language, Policy, ShapeRule};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The decision a removable comment asks its author for.
///
/// Ordered by which claim wins when a comment is several of these at once: a
/// `// TODO:` beside an assignment is a promise first, because "do it or delete
/// it" is a larger question than which line it sits on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Decision {
    /// It opens with a tag that promises something. The tag is on the item
    /// rather than here, so that two promises are one decision asked twice
    /// rather than two decisions that happen to rhyme.
    Promise,
    /// It shares a line with code.
    BesideCode,
    /// Its text reads as code rather than as prose.
    CommentedOutCode,
    /// The next thing in the file is an item the language documents.
    ExplainsTheItemBelow,
    /// Nothing but blank lines come before it.
    AtTheTopOfTheFile,
    /// Anything else: it sits among statements.
    AmongStatements,
    /// A promise whose deadline has passed. The engine decided this one and
    /// knows by how much, so its words win over anything read from the
    /// surrounding lines.
    Expired { age: Age, limit: Age },
    /// Longer than the configured paragraph. Also the engine's, and also
    /// answered by an edit to the comment rather than by deleting it.
    TooLong { limit: usize },
    /// The policy is stricter than the kind of comment this is, and a gentler
    /// one keeps it. Nothing about where it sits enters into that, and reading
    /// the surrounding lines for advice would answer a question nobody asked:
    /// a documentation comment taken out by `--policy all` is not a comment in
    /// the wrong place, it is a run asking for more than the reader meant.
    StricterThanTheKind { kind: CommentKind, keeps: Policy },
}

impl Decision {
    /// The imperative a reader acts on, in the order this is asked.
    #[must_use]
    pub fn instruction(&self) -> String {
        match self {
            Self::Promise => "do what it promises, or delete it".to_owned(),
            Self::BesideCode => "move it above the code, or drop it".to_owned(),
            Self::CommentedOutCode => "delete it — the history has the code".to_owned(),
            Self::ExplainsTheItemBelow => "make it a documentation comment".to_owned(),
            Self::AtTheTopOfTheFile => "make it the file's documentation comment".to_owned(),
            Self::AmongStatements => "let the code say it, or tag it".to_owned(),
            Self::Expired { age, limit } => {
                format!("do it or drop it ({age} old, {limit} allowed)")
            }
            Self::TooLong { limit } => format!(
                "shorten to {}, or move the rest into documentation",
                crate::output::plural(*limit, "line")
            ),
            Self::StricterThanTheKind { kind, keeps } => {
                format!("keep `{kind}` comments with `{keeps}`, or mean to remove them")
            }
        }
    }

    /// A short name a machine reads, and a heading groups by.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Promise => "promise",
            Self::BesideCode => "beside-code",
            Self::CommentedOutCode => "commented-out-code",
            Self::ExplainsTheItemBelow => "explains-the-item-below",
            Self::AtTheTopOfTheFile => "top-of-the-file",
            Self::AmongStatements => "among-statements",
            Self::Expired { .. } => "expired",
            Self::TooLong { .. } => "too-long",
            Self::StricterThanTheKind { .. } => "stricter-than-the-kind",
        }
    }

    /// The setting that would stop this being reported, and the table it goes
    /// in.
    ///
    /// The other half of every decision. A gate that can only say "delete it"
    /// is a gate somebody turns off the first time it is wrong about one
    /// comment, so the way to keep a comment has to be as visible as the way to
    /// remove it — and it has to be a setting rather than a flag, because a
    /// flag makes one run pass and a setting is a decision the repository
    /// keeps.
    #[must_use]
    pub fn keep_route(&self, tags: &BTreeSet<String>, longest: usize) -> Option<String> {
        let named = |names: &BTreeSet<String>| {
            let list: Vec<String> = names.iter().map(|tag| format!("\"{tag}\"")).collect();
            format!("[policy.allow]\ntags = [{}]", list.join(", "))
        };
        match self {
            Self::Promise => Some(named(tags)),
            Self::BesideCode => Some("[policy.allow]\ntrailing = true".to_owned()),
            Self::AmongStatements | Self::AtTheTopOfTheFile | Self::ExplainsTheItemBelow => {
                Some("[policy.allow]\ntags = [\"NOTE\"]".to_owned())
            }
            /* NOTE: A deadline is kept by changing the deadline, so the value
             * is left for the reader to choose. Filling one in would be this
             * offering a number nobody decided, for a rule whose whole point is
             * that somebody did. */
            Self::Expired { .. } => Some(format!(
                "[policy.allow.expiry]\n{} = \"...\"  # longer than the oldest above",
                tags.iter().next().map_or("TODO", String::as_str)
            )),
            Self::TooLong { limit } => Some(format!(
                "[policy.allow]\nmax_lines = {longest}  # {limit} now, {longest} is the longest above"
            )),
            /* NOTE: The policy itself, because the policy is what decided it.
             * Offering `[policy.allow] tags` here was the old answer and it was
             * wrong twice over: a `///` carries no tag to allow, and allowing
             * one would not reach a rule that is about kinds. */
            Self::StricterThanTheKind { keeps, .. } => {
                Some(format!("[policy]\nmode = \"{keeps}\""))
            }
            /* NOTE: None on purpose. Commented-out code is the one situation
             * with nothing worth keeping, and offering a way to keep it would
             * be this file's own advice arguing against itself. */
            Self::CommentedOutCode => None,
        }
    }
}

/// One comment, or one run of them, and what its author can do about it.
#[derive(Clone, Debug)]
pub struct Item {
    pub path: PathBuf,
    /// One-based, inclusive on both ends.
    pub first_line: usize,
    pub last_line: usize,
    /// How many comments the run holds.
    pub comments: usize,
    /// The lines as they are, sanitised.
    pub old: Vec<String>,
    /// What would replace them, sanitised. Empty when the advice is to delete.
    pub new: Vec<String>,
    /// The line the comment is about, when there is one worth showing.
    pub subject: Option<String>,
    /// The promise it opens with, when it opens with one.
    pub tag: Option<String>,
}

/// One decision and everything that asks for it.
#[derive(Clone, Debug)]
pub struct Group {
    pub decision: Decision,
    pub items: Vec<Item>,
}

impl Group {
    /// How many comments the group covers, which is not how many items it has.
    #[must_use]
    pub fn comments(&self) -> usize {
        self.items.iter().map(|item| item.comments).sum()
    }

    /// The setting that would stop this group being reported.
    #[must_use]
    pub fn keep_route(&self) -> Option<String> {
        let tags: BTreeSet<String> = self
            .items
            .iter()
            .filter_map(|item| item.tag.clone())
            .collect();
        let longest = self
            .items
            .iter()
            .map(|item| item.old.len())
            .max()
            .unwrap_or(1);
        self.decision.keep_route(&tags, longest)
    }
}

/// The tags that promise something, as opposed to the ones that remark.
///
/// `NOTE`, `SAFETY` and the rest say why the code is as it is and are answered
/// by reading them. These say the code is not as it should be, which is a debt
/// somebody took on, and the only two ways to answer a debt are to pay it or to
/// write it off.
const PROMISES: [&str; 5] = ["TODO", "FIXME", "HACK", "XXX", "BUG"];

/// How a language spells a documentation comment that a prefix rewrite reaches.
///
/// Only the languages where the rewrite is the whole edit: `//` becomes `///`
/// and the comment is documentation. A language whose documentation lives
/// somewhere else -- Python's inside the item, Java's in a block that has to be
/// opened and closed -- is absent, and the advice for it says what to do
/// without pretending the edit is one character.
const DOC_PREFIX: [(Language, &str); 8] = [
    (Language::Rust, "///"),
    (Language::C, "///"),
    (Language::Cpp, "///"),
    (Language::CSharp, "///"),
    (Language::Swift, "///"),
    (Language::Dart, "///"),
    (Language::Zig, "///"),
    (Language::Lua, "---"),
];

/// Everything the run found, grouped by the decision it asks for.
///
/// Groups come out in the order the decisions are declared, so two runs over
/// the same tree print the same report.
#[must_use]
pub fn plan(files: &[ProcessedFile], policy: Policy) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for file in files {
        for item in file_items(file, policy) {
            let (decision, item) = item;
            match groups.iter_mut().find(|group| group.decision == decision) {
                Some(group) => group.items.push(item),
                None => groups.push(Group {
                    decision,
                    items: vec![item],
                }),
            }
        }
    }
    groups.sort_by_key(|group| std::cmp::Reverse(group.comments()));
    groups
}

/// The removable comments of one file, as runs, each with its decision.
///
/// Runs are formed before anything is decided, and that order is the whole of
/// it: a paragraph's decision is read from the line *under the paragraph*, so
/// deciding comment by comment asks the first line what the second line is,
/// gets "another comment", and files four lines of prose about a struct under
/// "it sits among statements".
fn file_items(file: &ProcessedFile, policy: Policy) -> Vec<(Decision, Item)> {
    let lines = source_lines(&file.source);
    /* NOTE: Once per file. Building it per comment turns a report over a large
     * file into a quadratic one, and a file with a thousand comments is exactly
     * the file somebody runs this on first. */
    let index = crate::output::LineIndex::new(&file.source);
    let mut runs: Vec<Run> = Vec::new();
    for comment in &file.result.report.comments {
        if !comment.disposition.is_remove() {
            runs.push(Run::BREAK);
            continue;
        }
        let Some(placed) = place(&lines, &index, comment) else {
            runs.push(Run::BREAK);
            continue;
        };
        match runs.last_mut() {
            /* NOTE: A run of adjacent comment lines is one paragraph to a
             * reader and one decision to its author. A promise never joins
             * one: it asks a different question, and merging it would hide
             * that question inside a block of prose. */
            Some(open)
                if open.comments > 0
                    && open.column == placed.column
                    && open.last + 1 == placed.first
                    && open.tag.is_none()
                    && placed.tag.is_none()
                    /* NOTE: A comment beside code is never part of a paragraph.
                     * Forty trailing notes on forty consecutive assignments
                     * share a column and are adjacent, and they are forty
                     * decisions: each one is about the statement it sits on,
                     * and the statement is different every time. */
                    && !open.beside
                    && !placed.beside
                    /* NOTE: Equal, not absent. A paragraph that ran over the
                     * line limit carries the same `TooLong` on every line it
                     * covers, and splitting it back into one finding per line
                     * would report a single paragraph as several. A deadline
                     * is the other way: each one is its own promise. */
                    && open.shape == placed.shape
                    && open.kind == placed.kind =>
            {
                open.last = placed.last;
                open.comments += 1;
            }
            _ => runs.push(Run {
                first: placed.first,
                last: placed.last,
                column: placed.column,
                comments: 1,
                tag: placed.tag,
                shape: placed.shape,
                beside: placed.beside,
                kind: placed.kind,
            }),
        }
    }
    runs.into_iter()
        .filter(|run| run.comments > 0)
        .filter_map(|run| item_of(file, &lines, run, policy))
        .collect()
}

/// A run of adjacent comments, before anything has been decided about it.
struct Run {
    first: usize,
    last: usize,
    column: usize,
    comments: usize,
    tag: Option<String>,
    shape: Option<ShapeRule>,
    beside: bool,
    kind: CommentKind,
}

impl Run {
    /// A run that nothing joins, which is how a kept comment or a comment this
    /// could not place separates the two paragraphs around it.
    const BREAK: Self = Self {
        first: 0,
        last: 0,
        column: 0,
        comments: 0,
        tag: None,
        shape: None,
        beside: false,
        kind: CommentKind::Line,
    };
}

/// One run, decided and rendered.
fn item_of(
    file: &ProcessedFile,
    lines: &[String],
    run: Run,
    policy: Policy,
) -> Option<(Decision, Item)> {
    let old: Vec<String> = lines.get(run.first - 1..run.last)?.to_vec();
    let below = lines
        .get(run.last..)?
        .iter()
        .find(|line| !line.trim().is_empty());
    let before = lines
        .get(run.first - 1)?
        .get(..run.column.saturating_sub(1))?
        .trim();
    let body = body_of(lines.get(run.first - 1)?, run.column);
    /* NOTE: A rule the engine decided over the whole file wins. It knows by
     * how much a deadline was missed and by how many lines a paragraph ran
     * over, and a situation read from the surrounding lines cannot say either.
     * `Tagged` never reaches here, because a tagged comment was kept. */
    let decision = if let Some(shape) = &run.shape {
        match shape {
            ShapeRule::Expired { age, limit, .. } => Decision::Expired {
                age: *age,
                limit: *limit,
            },
            ShapeRule::TooLong { limit, .. } => Decision::TooLong { limit: *limit },
            ShapeRule::Trailing => Decision::BesideCode,
            ShapeRule::Tagged { .. } => Decision::AmongStatements,
        }
    } else if let Some(keeps) = gentler_policy(run.kind, policy) {
        Decision::StricterThanTheKind {
            kind: run.kind,
            keeps,
        }
    } else if run.tag.is_some() {
        Decision::Promise
    } else if !before.is_empty() {
        Decision::BesideCode
    } else if reads_as_code(&body) {
        Decision::CommentedOutCode
    } else if below.is_some_and(|line| opens_an_item(line)) {
        Decision::ExplainsTheItemBelow
    } else if lines
        .get(..run.first - 1)?
        .iter()
        .all(|line| line.trim().is_empty())
    {
        Decision::AtTheTopOfTheFile
    } else {
        Decision::AmongStatements
    };
    let new = match (&decision, doc_prefix(file.language)) {
        (Decision::ExplainsTheItemBelow, Some(prefix)) => promote(&old, prefix),
        _ => Vec::new(),
    };
    let subject = matches!(decision, Decision::ExplainsTheItemBelow)
        .then(|| below.cloned())
        .flatten();
    Some((
        decision,
        Item {
            path: file.path.clone(),
            first_line: run.first,
            last_line: run.last,
            comments: run.comments,
            old,
            new,
            subject,
            tag: run.tag,
        },
    ))
}

/// One comment, placed: which lines it covers and whether it opens a promise.
struct Placed {
    first: usize,
    last: usize,
    column: usize,
    tag: Option<String>,
    shape: Option<ShapeRule>,
    beside: bool,
    kind: CommentKind,
}

fn place(lines: &[String], index: &crate::output::LineIndex, comment: &Comment) -> Option<Placed> {
    let (first, column) = index.line_column(comment.span.start);
    let (last_line, last_column) = index.line_column(comment.span.end);
    let last = if last_column == 1 {
        last_line.saturating_sub(1)
    } else {
        last_line
    };
    if first == 0 || last < first || last > lines.len() {
        return None;
    }
    let tag = promise_tag(&body_of(lines.get(first - 1)?, column));
    let beside = !lines
        .get(first - 1)?
        .get(..column.saturating_sub(1))?
        .trim()
        .is_empty();
    Some(Placed {
        first,
        last,
        column,
        tag,
        shape: comment.shape.clone(),
        beside,
        kind: comment.kind,
    })
}

/// A policy gentler than this run's that would keep a comment of this kind.
///
/// `None` when no policy keeps it, which is every ordinary comment: the reader
/// of one of those has a decision to make about the comment, and the reader of
/// a documentation comment removed by `--policy all` has a decision to make
/// about the run.
fn gentler_policy(kind: CommentKind, policy: Policy) -> Option<Policy> {
    let keeps = Policy::strongest_keeping(&[kind])?;
    (keeps != policy && !policy.keeps(kind)).then_some(keeps)
}

/// The comment's own text, without the code that may share its line.
fn body_of(line: &str, column: usize) -> String {
    line.get(column.saturating_sub(1)..)
        .unwrap_or("")
        .trim_start_matches(['/', '*', '#', '-', '<', '!', ';', '%', '\'', ' '])
        .trim()
        .to_owned()
}

/// The promise a comment opens with, upper-cased, or `None` when it opens with
/// prose.
///
/// The tag has to end at a boundary: `TODO:` and `TODO(name)` are promises and
/// `TODOs are tracked elsewhere` is a sentence about them.
fn promise_tag(body: &str) -> Option<String> {
    let word: String = body
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_ascii_uppercase();
    let rest = body.get(word.len()..).unwrap_or("");
    let bounded =
        rest.is_empty() || rest.starts_with([':', '(', '-', ' ', '!']) || rest.starts_with('\t');
    (bounded && PROMISES.contains(&word.as_str())).then_some(word)
}

/// Whether a comment's text reads as code rather than as prose.
///
/// Deliberately timid. Calling prose "code" advises deleting something a reader
/// wrote on purpose, so this only answers yes for text that ends the way a
/// statement ends and does not end the way a sentence does.
fn reads_as_code(body: &str) -> bool {
    let trimmed = body.trim_end();
    trimmed.ends_with([';', '{', '}'])
        && !trimmed.ends_with(['.', '!', '?'])
        && trimmed.contains(|character: char| "=()".contains(character))
}

/// Whether a line opens something a language would document.
///
/// One list for every language, which is coarse and is the right kind of
/// coarse: the question is only whether a doc comment would have somewhere to
/// attach, and a word that opens a definition in one language is not a word
/// that opens a statement in another.
fn opens_an_item(line: &str) -> bool {
    const OPENERS: [&str; 16] = [
        "fn ",
        "struct ",
        "enum ",
        "trait ",
        "impl ",
        "type ",
        "const ",
        "mod ",
        "class ",
        "def ",
        "func ",
        "interface ",
        "public ",
        "private ",
        "protected ",
        "let ",
    ];
    let trimmed = line.trim_start();
    let without = [
        "pub ",
        "pub(crate) ",
        "export ",
        "async ",
        "static ",
        "unsafe ",
        "extern ",
        "default ",
        "final ",
        "abstract ",
    ]
    .iter()
    .fold(trimmed, |rest, prefix| {
        rest.strip_prefix(prefix).unwrap_or(rest).trim_start()
    });
    OPENERS.iter().any(|opener| without.starts_with(opener))
}

/// The documentation spelling a prefix rewrite reaches in this language.
#[must_use]
pub fn doc_prefix(language: Language) -> Option<&'static str> {
    DOC_PREFIX
        .iter()
        .find(|(candidate, _)| *candidate == language)
        .map(|(_, prefix)| *prefix)
}

/// The file's lines, sanitised once, so nothing downstream has to remember to.
fn source_lines(source: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(source)
        .split('\n')
        .map(|line| sanitize_source_line(line.strip_suffix('\r').unwrap_or(line)))
        .collect()
}

/// Rewrite a run of comment lines as documentation, when a prefix reaches it.
#[must_use]
pub fn promote(lines: &[String], prefix: &str) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            let rest = line.trim_start();
            let opener: String = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && !c.is_alphanumeric())
                .collect();
            if opener.is_empty() {
                line.clone()
            } else {
                format!("{indent}{prefix}{}", &rest[opener.len()..])
            }
        })
        .collect()
}

impl Item {
    /// The path and lines as every report here writes them, so a reader
    /// searching one can search the other.
    #[must_use]
    pub fn where_it_is(&self) -> String {
        let path = crate::output::sanitize_path(&self.path.to_string_lossy());
        if self.first_line == self.last_line {
            format!("{path}:{}", self.first_line)
        } else {
            format!("{path}:{}-{}", self.first_line, self.last_line)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_promise_ends_at_a_boundary() {
        assert_eq!(promise_tag("TODO: do it"), Some("TODO".to_owned()));
        assert_eq!(promise_tag("TODO(sam): do it"), Some("TODO".to_owned()));
        assert_eq!(promise_tag("todo"), Some("TODO".to_owned()));
        assert_eq!(promise_tag("TODOs are tracked elsewhere"), None);
        assert_eq!(promise_tag("the retry budget"), None);
    }

    #[test]
    fn code_is_told_from_prose_by_how_it_ends() {
        assert!(reads_as_code("self.width = width;"));
        assert!(reads_as_code("if (a) {"));
        assert!(!reads_as_code("the width, in cells"));
        assert!(!reads_as_code("we set self.width = width."));
    }

    #[test]
    fn an_item_is_told_from_a_statement() {
        assert!(opens_an_item("pub struct Budget {"));
        assert!(opens_an_item("    def render(self, text):"));
        assert!(opens_an_item("export class Panel {"));
        assert!(!opens_an_item("    self.width = width"));
        assert!(!opens_an_item("return false;"));
    }

    #[test]
    fn a_rewrite_keeps_the_indent_and_replaces_the_opener() {
        let lines = vec!["    // a note".to_owned(), "// another".to_owned()];
        assert_eq!(
            promote(&lines, "///"),
            vec!["    /// a note".to_owned(), "/// another".to_owned()]
        );
    }
}
