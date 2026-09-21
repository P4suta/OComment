//! How a comment that survives is written.
//!
//! The other axis.
//! [`ScanOptions::allow`](crate::ScanOptions::allow) decides whether a comment stays; this decides how it reads once it has.
//! The two are deliberately not the same table: a comment that fails a condition of survival is removed, and a comment that fails a rule here is rewritten, and a reader adding a rule to a table whose entries have two different consequences would have to guess which they were adding.
//!
//! # What a rewrite may touch
//!
//! Only the bytes inside the comment.
//! The crate promises that "the only bytes that move are the ones a comment occupied", and a rewrite keeps that promise literally: the edit it plans replaces the comment's span and nothing else, so the code around it, its indentation, and the line ending after it are the same bytes afterwards.
//!
//! A comment whose bytes are not valid UTF-8 is never rewritten.
//! The engine does not decode the whole source, but it cannot reason about words without decoding the comment, and guessing at a boundary inside bytes it could not read is how a formatter corrupts a file it was asked to tidy.
//!
//! # The rules compose, and the first one recorded is the one that found
//! something
//!
//! [`restyle`] applies every rule the configuration asks for and returns the bytes with all of them applied, together with the first rule that had anything to do.
//! That rule is what the comment records and what `--explain` names: a reader is being told why the comment is in the report at all, and the answer is the rule that put it there.

use crate::scanner::marker_bounds_with;
use crate::types::{StyleRule, StyleRules};

/// The delimiters a comment in this file opens and closes with.
///
/// Carried rather than guessed at.
/// A file read under a declarative profile opens its comments with the tokens the profile declares, and the built-in list knows `--` but not Haddock's `-- |`: a rule about the text written against the marker would have judged the space that belongs to the marker.
#[derive(Clone, Copy, Debug)]
pub struct Markers<'a> {
    /// Every token that may open a comment here, in any order.
    pub openers: &'a [&'a [u8]],
    /// Every token that may close one.
    pub closers: &'a [&'a [u8]],
}

impl Markers<'static> {
    /// What the built-in languages use.
    pub const BUILTIN: Self = Self {
        openers: crate::scanner::BUILTIN_OPENERS,
        closers: crate::scanner::BUILTIN_CLOSERS,
    };
}

/// Rewrite one comment's bytes under `rules`.
///
/// `raw` is the comment's complete bytes, delimiters included, exactly as [`Comment::span`](crate::Comment::span) delimits them.
/// The answer is `None` when the rules find nothing to change — which is the ordinary case, and the case a scan must be cheap in.
///
/// # Examples
///
/// ```
/// use ocomment_core::{Markers, StyleRule, StyleRules, restyle};
///
/// let rules = StyleRules {
///     space_after_marker: Some(true),
///     ..StyleRules::default()
/// };
/// let (rule, bytes) = restyle(b"//note", &rules, Markers::BUILTIN).unwrap();
/// assert_eq!(rule, StyleRule::SpaceAfterMarker);
/// assert_eq!(bytes, b"// note");
///
/// // A ruler is not a comment missing its space.
/// assert!(restyle(b"////////", &rules, Markers::BUILTIN).is_none());
/// ```
#[must_use]
pub fn restyle(
    raw: &[u8],
    rules: &StyleRules,
    markers: Markers<'_>,
) -> Option<(StyleRule, Vec<u8>)> {
    if rules.is_empty() || std::str::from_utf8(raw).is_err() {
        return None;
    }
    let mut bytes = raw.to_vec();
    let mut first = None;
    for rule in StyleRule::ALL {
        if !rule.asked_for_by(rules) {
            continue;
        }
        let next = match rule {
            /* NOTE: Decided over a run rather than over a comment, because joining two comment lines moves the bytes between them and those belong to neither.
             * `reflow_run` is where it is applied, and a comment inside a run a rewrite reached is not asked these questions again. */
            StyleRule::Wrap => None,
            StyleRule::SpaceAfterMarker => space_after_marker(&bytes, markers),
            StyleRule::TrailingWhitespace => trailing_whitespace(&bytes),
        };
        if let Some(next) = next {
            bytes = next;
            first.get_or_insert(rule);
        }
    }
    first.map(|rule| (rule, bytes))
}

/// Reflow a run of comments on consecutive lines, or `None` when the rule leaves it as written.
///
/// `comments` are the run's tokens in order and `source` the bytes they came from.
/// The answer replaces everything from the first token's first byte to the last one's last byte, the white space between them included: that white space is what a join has to be allowed to move, and nothing before the first byte is touched, so the indentation the run sits at is read rather than written and the code around it cannot be reached.
///
/// A run this cannot take apart is left alone rather than guessed at.
/// Every token has to be a line comment, every one of them has to open with the same token, and nothing but white space may sit in front of any of them.
/// A block comment carries its own interior line structure — a continuation prefix that has to be inferred rather than read — and is not reflowed here.
#[must_use]
pub(crate) fn reflow_run(
    source: &[u8],
    comments: &[crate::Comment],
    rules: &StyleRules,
    markers: Markers<'_>,
    tags: &[&str],
) -> Option<Vec<u8>> {
    if !rules.wrap.rewrites() {
        return None;
    }
    if let [only] = comments {
        let raw = source.get(only.span.start..only.span.end)?;
        if is_block(raw, markers) {
            return reflow_block(source, only, rules, markers);
        }
    }
    let (opener, indent, mut bodies) = take_apart(source, comments, markers)?;
    let opener = opener.as_slice();
    let tag = shared_tag(&bodies, tags)?;
    for body in &mut bodies {
        *body = body.get(tag.len()..)?;
    }
    let reflowed = crate::reflow::reflow(&bodies, rules.wrap)?;
    let marker = RunMarker::new(indent, opener);
    let terminator = run_terminator(source, comments);
    let mut bytes = Vec::with_capacity(source.len());
    for (index, body) in reflowed.iter().enumerate() {
        if index > 0 {
            bytes.extend_from_slice(terminator);
        }
        bytes.extend_from_slice(marker.line(index));
        /* NOTE: One space, or none where there is nothing to separate.
         * A reflow has to write the marker back, so it has to choose; choosing anything else would be a second opinion about the spacing rule. */
        if !body.is_empty() || !tag.is_empty() {
            bytes.push(b' ');
        }
        bytes.extend_from_slice(tag.as_bytes());
        bytes.extend_from_slice(body.as_bytes());
    }
    let span = crate::ByteSpan::new(comments.first()?.span.start, comments.last()?.span.end);
    (bytes != source.get(span.start..span.end)?).then_some(bytes)
}

/// Whether a token is a delimited comment rather than one running to the end of its line.
///
/// A token that spans a line ending is one, and so is a token that ends at a closing delimiter: `(* NOTE: ... *)` fits on a line and is still a block.
pub(crate) fn is_block(raw: &[u8], markers: Markers<'_>) -> bool {
    raw.contains(&b'\n') || markers.closers.iter().any(|closer| raw.ends_with(closer))
}

/// Reflow one block comment, keeping its delimiters and the prefix its continuation lines are written with.
///
/// The prefix is *learned* rather than assumed.
/// A C-family block writes its continuation lines under a `*` and an OCaml one writes them aligned under the text, and a formatter that picked one would rewrite every comment in the other family into a shape nobody there writes.
/// What is read is the longest prefix the interior lines share, truncated at the first character that is neither white space nor the opener's own last character — so it can never reach into the prose, which is the mistake a plain common prefix makes with `The cat` above `The dog`.
///
/// A block that fits on one line is left alone: there are no interior lines to learn from, and a guess would be a guess.
fn reflow_block(
    source: &[u8],
    comment: &crate::Comment,
    rules: &StyleRules,
    markers: Markers<'_>,
) -> Option<Vec<u8>> {
    let raw = source.get(comment.span.start..comment.span.end)?;
    let text = std::str::from_utf8(raw).ok()?;
    if !text.contains('\n') {
        return None;
    }
    let opener = markers
        .openers
        .iter()
        .filter(|marker| raw.starts_with(marker))
        .max_by_key(|marker| marker.len())?;
    let closer = markers
        .closers
        .iter()
        .filter(|marker| raw.ends_with(marker))
        .max_by_key(|marker| marker.len())?;
    let opener = std::str::from_utf8(opener).ok()?;
    let closer = std::str::from_utf8(closer).ok()?;
    let inner = text.get(opener.len()..text.len() - closer.len())?;
    let mut rows: Vec<&str> = inner.split('\n').collect();
    /* NOTE: A carriage return belongs to the line ending rather than to the prose, and is put back with it. */
    for row in &mut rows {
        *row = row.strip_suffix('\r').unwrap_or(row);
    }
    let (first, interior) = rows.split_first()?;
    if interior.is_empty() {
        return None;
    }
    /* NOTE: Whether the closing delimiter sits on a line of its own, which is read before the prefix is: a line holding nothing but white space and the star carries no body, and learning the prefix from it would cut the prefix down to the indentation and leave the star in the prose. */
    let star = opener.chars().next_back()?;
    let closer_alone = interior
        .last()?
        .trim()
        .trim_start_matches(star)
        .trim()
        .is_empty();
    let carrying = if closer_alone {
        interior.get(..interior.len() - 1)?
    } else {
        interior
    };
    let prefix = continuation_prefix(carrying, opener)?;
    let mut bodies = Vec::with_capacity(rows.len());
    bodies.push(first.strip_prefix(' ').unwrap_or(first).trim_end());
    for row in carrying {
        let body = row.strip_prefix(prefix).unwrap_or(row.trim_start());
        let body = body.strip_prefix(' ').unwrap_or(body).trim_end();
        bodies.push(body);
    }
    let reflowed = crate::reflow::reflow(&bodies, rules.wrap)?;
    let terminator = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(text.len() + 16);
    for (index, body) in reflowed.iter().enumerate() {
        if index == 0 {
            out.push_str(opener);
            if !body.is_empty() {
                out.push(' ');
            }
        } else {
            out.push_str(terminator);
            if body.is_empty() {
                out.push_str(prefix.trim_end());
            } else {
                out.push_str(prefix);
                if !prefix.ends_with(' ') {
                    out.push(' ');
                }
            }
        }
        out.push_str(body);
    }
    if closer_alone {
        out.push_str(terminator);
        /* NOTE: The indentation the prefix begins with, and not the prefix itself.
         * A C-family prefix ends in the star the closer already begins with,
         * and writing both leaves the star twice. */
        let indent = prefix
            .find(|character| character != ' ' && character != '\t')
            .map_or(prefix, |at| &prefix[..at]);
        out.push_str(indent);
        out.push_str(closer);
    } else {
        out.push(' ');
        out.push_str(closer);
    }
    (out.as_bytes() != raw).then(|| out.into_bytes())
}

/// The prefix this block's continuation lines are written with.
///
/// The longest one they all share, cut at the first character that is neither white space nor the opener's last character.
fn continuation_prefix<'a>(interior: &[&'a str], opener: &str) -> Option<&'a str> {
    let star = opener.chars().next_back()?;
    /* NOTE: A blank line inside a block is a paragraph break and carries no prefix to learn from, so it is not asked. */
    let carrying: Vec<&'a str> = interior
        .iter()
        .copied()
        .filter(|row| !row.trim().is_empty())
        .collect();
    let mut shared = *carrying.first()?;
    for row in carrying.iter().skip(1) {
        let common = shared
            .char_indices()
            .zip(row.chars())
            .take_while(|((_, left), right)| left == right)
            .last()
            .map_or(0, |((at, left), _)| at + left.len_utf8());
        shared = shared.get(..common)?;
    }
    let cut = shared
        .char_indices()
        .find(|(_, character)| *character != ' ' && *character != '\t' && *character != star)
        .map_or(shared.len(), |(at, _)| at);
    shared.get(..cut)
}

/// The tag every line of this run opens with, which is part of its marker rather than part of its prose.
///
/// A project whose configuration names tags has to write one on every comment,
/// and a run of line comments is a run of comments: the convention puts the tag on each of them.
/// Read as prose those tags are words in the middle of a paragraph, and a join would leave `NOTE: one NOTE: two` behind; a split would leave lines the tag rule no longer keeps.
///
/// `None` refuses the run, which is what happens when some lines carry a tag and others do not.
/// That is a paragraph whose lines the tag rule already disagrees about, and moving its breaks would settle the disagreement by accident.
///
/// A common prefix would be the general form of this and is deliberately not what is looked for: `# The cat sat` above `# The dog ran` shares one, and treating `The ` as a marker would join them into nonsense.
/// What is looked for is a tag the configuration named, or — where it named none that matches — a label: a word in capitals with a colon after it.
/// The second is not a guess about prose.
/// A configuration's tag list says which tags keep a comment alive, which is a question a project answers; whether `NOTE:` at the start of every line of a paragraph is a label is a question about the text, and a machine-wide rule that removes nothing has no tag list to answer it with.
/// Without it, reflowing a run of `# NOTE:` lines under such a configuration wrote `as a setting NOTE: rather than` into the middle of a sentence.
fn shared_tag<'a>(bodies: &[&'a str], tags: &[&str]) -> Option<&'a str> {
    let opening = |body: &'a str| -> Option<&'a str> {
        tags.iter()
            .copied()
            .chain(label(body))
            .filter_map(|tag| {
                let rest = body.get(..tag.len())?;
                (rest.eq_ignore_ascii_case(tag)).then(|| {
                    /* NOTE: The tag and the punctuation that introduces what follows it, and nothing past that.
                     * Skipping every non-alphanumeric byte was greedy enough to swallow the opening backtick of the first word, which made one line's prefix differ from the next one's and refused the run. */
                    let rest = body.get(tag.len()..)?;
                    let rest = rest.strip_prefix('(').map_or(rest, |inner| {
                        inner.find(')').map_or(rest, |at| &inner[at + 1..])
                    });
                    let rest = rest.strip_prefix([':', '-', '.']).unwrap_or(rest);
                    let punctuation = body.len() - rest.len();
                    let spaces = rest.len() - rest.trim_start_matches(' ').len();
                    body.get(..punctuation + spaces)
                })
            })
            .flatten()
            .max_by_key(|found| found.len())
    };
    let Some(tag) = opening(bodies.first()?) else {
        // NOTE: No tag anywhere is the ordinary case, and the whole body is prose.
        return bodies
            .iter()
            .all(|body| opening(body).is_none())
            .then_some("");
    };
    bodies
        .iter()
        .all(|body| opening(body) == Some(tag))
        .then_some(tag)
}

/// The label a line opens with, which is a word in capitals with a colon and a space after it.
///
/// `NOTE:`, `TODO:`, `SAFETY:`, `INVARIANT:` — one convention, recognised by its shape rather than from a list, because the list a configuration keeps answers a different question.
/// The capitals are required and so is the space: `The cat sat` is prose that happens to open a line, and `HTTP://host` is an address.
fn label(body: &str) -> Option<&str> {
    let end = body.find(':')?;
    let word = body.get(..end)?;
    let next = body.as_bytes().get(end + 1);
    (word.len() >= 2
        && word.len() <= 16
        && word
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        && matches!(next, None | Some(b' ')))
    .then_some(word)
}

/// What each line of a rewritten run begins with.
///
/// Two prefixes rather than one, because the run's first line is already begun.
/// The replacement covers the run from its first comment's opener, so the indentation in front of that opener is source the rewrite does not cover and the first line must not write it; every line after a break is begun by the rewrite itself and carries it.
/// Writing one prefix for both moves the paragraph right by its own indentation each time it is reflowed, which is a formatter re-indenting a file it was told it could only reach inside comments.
struct RunMarker {
    /// What the replacement opens with, the indentation excluded.
    first: Vec<u8>,
    /// What each line after a break opens with, the indentation included.
    rest: Vec<u8>,
}

impl RunMarker {
    fn new(indent: &[u8], opener: &[u8]) -> Self {
        let mut rest = indent.to_vec();
        rest.extend_from_slice(opener);
        Self {
            first: opener.to_vec(),
            rest,
        }
    }

    /// What the line at `index` of the rewrite begins with.
    fn line(&self, index: usize) -> &[u8] {
        if index == 0 { &self.first } else { &self.rest }
    }
}

/// Take a run apart, or refuse it.
///
/// The opener and the indentation are returned once rather than per line, and that is the whole of what this refuses for.
/// A run whose lines open differently is not one paragraph, and neither is a run whose lines sit at different columns: a commented-out block of shell holds its structure in its indentation, and reading it as prose flattens the structure into a sentence.
/// Returning one of each is what stops a rewrite from having to choose between them — there is no line whose indentation the answer could disagree with.
fn take_apart<'a>(
    source: &'a [u8],
    comments: &[crate::Comment],
    markers: Markers<'_>,
) -> Option<(Vec<u8>, &'a [u8], Vec<&'a str>)> {
    let mut opener: Option<&[u8]> = None;
    let mut indentation: Option<&[u8]> = None;
    let mut lines = Vec::with_capacity(comments.len());
    for comment in comments {
        let raw = source.get(comment.span.start..comment.span.end)?;
        /* NOTE: A block comment, which this does not reflow.
         * A token that spans a line ending is one, and so is a token that ends at a closing delimiter: `(* NOTE: ... *)` fits on a line and is still a block, and reading one as a line comment swallows its `*)` into the prose and then breaks the prose in half.
         * That is the accident the gate this replaces shipped, met here in the source of the reference implementation. */
        if raw.contains(&b'\n') || markers.closers.iter().any(|closer| raw.ends_with(closer)) {
            return None;
        }
        let text = std::str::from_utf8(raw).ok()?;
        let found = markers
            .openers
            .iter()
            .filter(|marker| raw.starts_with(marker))
            .max_by_key(|marker| marker.len())?;
        if *opener.get_or_insert(found) != *found {
            return None;
        }
        let start = line_start(source, comment.span.start);
        let indent = source.get(start..comment.span.start)?;
        if !indent.iter().all(u8::is_ascii_whitespace) {
            return None;
        }
        if *indentation.get_or_insert(indent) != indent {
            return None;
        }
        /* NOTE: One space comes off, not all of them.
         * The space after the marker separates the marker from the text; anything past it is the writer's own indentation, and it is what tells a list item's continuation from a new paragraph.
         * Trimming it away is how the gate this replaces flattened a nested list. */
        let rest = text.get(found.len()..)?;
        lines.push(
            rest.strip_prefix(' ')
                .or_else(|| rest.strip_prefix('\t'))
                .unwrap_or(rest)
                .trim_end(),
        );
    }
    Some((opener?.to_vec(), indentation?, lines))
}

/// The byte the line holding `offset` begins at.
fn line_start(source: &[u8], offset: usize) -> usize {
    source[..offset.min(source.len())]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1)
}

/// The line ending this run is written with.
///
/// Read from the run itself where it has one to read, so a file with CRLF endings keeps them, and from the source around it where the run is a single line a rewrite is about to split.
fn run_terminator<'a>(source: &'a [u8], comments: &[crate::Comment]) -> &'a [u8] {
    let after = comments.first().map_or(0, |comment| comment.span.end);
    let carriage = source
        .get(after..)
        .and_then(|rest| rest.iter().position(|byte| *byte == b'\n'))
        .is_some_and(|at| at > 0 && source[after + at - 1] == b'\r');
    if carriage { b"\r\n" } else { b"\n" }
}

/// Put a space between the opening marker and the text written against it.
///
/// Deliberately timid, in the way [`crate`] is timid everywhere it reads text rather than syntax: it acts only when the first character of the text is neither white space nor ASCII punctuation.
/// That leaves `// "quoted"` alone,
/// which is a small miss, and it leaves `////////`, `#####`, `//-----` and `/*!` alone, which is the point — a ruler is not a comment missing its space, and inserting one there turns a divider into a divider with a hole in it.
fn space_after_marker(raw: &[u8], markers: Markers<'_>) -> Option<Vec<u8>> {
    let (start, end) = marker_bounds_with(raw, markers.openers, markers.closers);
    if start == 0 || start >= end {
        return None;
    }
    let first = raw.get(start)?;
    if first.is_ascii_whitespace() || first.is_ascii_punctuation() {
        return None;
    }
    let mut bytes = Vec::with_capacity(raw.len() + 1);
    bytes.extend_from_slice(&raw[..start]);
    bytes.push(b' ');
    bytes.extend_from_slice(&raw[start..]);
    Some(bytes)
}

/// Strip white space from the end of every line the comment covers.
///
/// The last line included: a line comment's span ends where its text ends, so the spaces a `// note   ` trails are inside it.
/// The space a *removal* would leave behind is not — that is the layout's business, and this rule has no opinion about it.
fn trailing_whitespace(raw: &[u8]) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(raw.len());
    let mut changed = false;
    let mut line = 0;
    for index in 0..=raw.len() {
        let terminator = index == raw.len() || raw[index] == b'\n';
        if !terminator {
            continue;
        }
        /* NOTE: `\r` is stripped with the rest and put back with the `\n`, so a CRLF source keeps its endings and a comment that trailed spaces before one loses only the spaces. */
        let mut stop = index;
        if stop > line && raw[stop - 1] == b'\r' {
            stop -= 1;
        }
        let carriage = stop != index;
        let kept = trim_end(&raw[line..stop]);
        changed |= kept.len() != stop - line;
        bytes.extend_from_slice(kept);
        if carriage {
            bytes.push(b'\r');
        }
        if index < raw.len() {
            bytes.push(b'\n');
        }
        line = index + 1;
    }
    changed.then_some(bytes)
}

/// `line` without the spaces and tabs at the end of it.
fn trim_end(line: &[u8]) -> &[u8] {
    let mut end = line.len();
    while end > 0 && matches!(line[end - 1], b' ' | b'\t' | 0x0b | 0x0c) {
        end -= 1;
    }
    &line[..end]
}

/// Whether a file of this language *is* prose rather than merely holding some.
///
/// A source file keeps its prose in comments; a Markdown document is prose,
/// and the paragraphs are what a rule about where a paragraph breaks is aimed at.
/// Exhaustive, so a language added later has to answer.
const fn is_a_document(language: crate::Language) -> bool {
    match language {
        crate::Language::Markdown => true,
        crate::Language::Rust
        | crate::Language::Ocaml
        | crate::Language::C
        | crate::Language::Cpp
        | crate::Language::Go
        | crate::Language::Java
        | crate::Language::JavaScript
        | crate::Language::TypeScript
        | crate::Language::Python
        | crate::Language::Shell
        | crate::Language::Html
        | crate::Language::Css
        | crate::Language::Jsonc
        | crate::Language::Sql
        | crate::Language::Kotlin
        | crate::Language::Toml
        | crate::Language::Lua
        | crate::Language::Yaml
        | crate::Language::Php
        | crate::Language::Ruby
        | crate::Language::Zig
        | crate::Language::R
        | crate::Language::Dart
        | crate::Language::Swift
        | crate::Language::CSharp
        | crate::Language::Scala
        | crate::Language::Vue
        | crate::Language::Svelte
        | crate::Language::Perl
        | crate::Language::Unknown => false,
    }
}

/// The paragraphs of a document that the style rules rewrite.
///
/// A paragraph is a stretch of consecutive lines with nothing between them:
/// a blank line ends one, and so does anything the document's structure is made of rather than its prose.
///
/// Three of those are tracked here rather than inside [`crate::reflow`],
/// because each of them runs across the blank lines that would otherwise end a paragraph: a fenced code block, the front matter at the top of a file, and an HTML comment.
/// The comment is the interesting one — it is prose too, and the comment path has already answered for it, so reading it again here would plan two edits over the same bytes.
#[must_use]
pub(crate) fn document_runs(
    source: &[u8],
    language: crate::Language,
    comments: &[crate::Comment],
    rules: &StyleRules,
) -> Vec<crate::ProseRun> {
    if !is_a_document(language) || !rules.wrap.rewrites() {
        return Vec::new();
    }
    let Ok(text) = std::str::from_utf8(source) else {
        return Vec::new();
    };
    let terminator = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut runs = Vec::new();
    let mut block: Vec<(usize, &str)> = Vec::new();
    let mut fenced = false;
    let mut front_matter = false;
    let mut offset = 0;
    for (number, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let start = offset;
        offset += raw.len() + 1;
        /* NOTE: The front matter is the block a `---` on the very first line opens, which is a document's metadata rather than its prose. */
        if number == 0 && line.trim_end() == "---" {
            front_matter = true;
            continue;
        }
        if front_matter {
            front_matter = !matches!(line.trim_end(), "---" | "...");
            continue;
        }
        let fence = crate::reflow::fence_marker(line).is_some();
        let inside_a_comment = comments
            .iter()
            .any(|comment| comment.span.start < start + line.len() && start < comment.span.end);
        if fence || fenced || inside_a_comment || line.trim().is_empty() {
            flush_block(&mut runs, &mut block, terminator, rules);
            if fence {
                fenced = !fenced;
            }
            continue;
        }
        block.push((start, line));
    }
    flush_block(&mut runs, &mut block, terminator, rules);
    runs
}

/// Reflow one paragraph, and record it when the rule asks for different bytes.
fn flush_block(
    runs: &mut Vec<crate::ProseRun>,
    block: &mut Vec<(usize, &str)>,
    terminator: &str,
    rules: &StyleRules,
) {
    let held = std::mem::take(block);
    let Some((first, _)) = held.first().copied() else {
        return;
    };
    let lines: Vec<&str> = held.iter().map(|(_, line)| *line).collect();
    let Some(reflowed) = crate::reflow::reflow(&lines, rules.wrap) else {
        return;
    };
    let (last, text) = held.last().copied().expect("the block is not empty");
    runs.push(crate::ProseRun {
        span: crate::ByteSpan::new(first, last + text.len()),
        origin: crate::ProseOrigin::Document,
        rule: crate::StyleRule::Wrap,
        replacement: reflowed.join(terminator).into_bytes(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(space: bool, trailing: bool) -> StyleRules {
        StyleRules {
            wrap: crate::Wrap::Preserve,
            space_after_marker: space.then_some(true),
            trailing_whitespace: trailing.then_some(false),
        }
    }

    #[test]
    fn a_rule_nobody_asked_for_changes_nothing() {
        assert!(restyle(b"//note   ", &StyleRules::default(), Markers::BUILTIN).is_none());
    }

    #[test]
    fn the_recorded_rule_is_the_first_one_that_found_something() {
        let (rule, bytes) = restyle(b"//note   ", &rules(true, true), Markers::BUILTIN).unwrap();
        assert_eq!(rule, StyleRule::SpaceAfterMarker);
        assert_eq!(bytes, b"// note");

        let (rule, bytes) = restyle(b"// note   ", &rules(true, true), Markers::BUILTIN).unwrap();
        assert_eq!(rule, StyleRule::TrailingWhitespace);
        assert_eq!(bytes, b"// note");
    }

    #[test]
    fn a_ruler_is_not_a_comment_missing_its_space() {
        for ruler in [
            b"////////".as_slice(),
            b"#######",
            b"//--------",
            b"/*!",
            b"(**",
        ] {
            assert!(
                restyle(ruler, &rules(true, false), Markers::BUILTIN).is_none(),
                "{ruler:?}"
            );
        }
    }

    #[test]
    fn a_marker_with_nothing_after_it_is_left_alone() {
        assert!(restyle(b"//", &rules(true, false), Markers::BUILTIN).is_none());
        assert!(restyle(b"#", &rules(true, false), Markers::BUILTIN).is_none());
    }

    #[test]
    fn every_marker_the_stripper_knows_is_reached() {
        for (raw, want) in [
            (b"///note".as_slice(), b"/// note".as_slice()),
            (b"//!note", b"//! note"),
            (b"#note", b"# note"),
            (b"--note", b"-- note"),
            (b";note", b"; note"),
            (b"%note", b"% note"),
            (b"/**note*/", b"/** note*/"),
            (b"<!--note-->", b"<!-- note-->"),
        ] {
            let (_, bytes) = restyle(raw, &rules(true, false), Markers::BUILTIN).unwrap();
            assert_eq!(bytes, want, "{raw:?}");
        }
    }

    #[test]
    fn every_line_of_a_block_loses_its_trailing_space() {
        let (_, bytes) = restyle(
            b"/* one  \n * two\t\n */",
            &rules(false, true),
            Markers::BUILTIN,
        )
        .unwrap();
        assert_eq!(bytes, b"/* one\n * two\n */");
    }

    #[test]
    fn a_crlf_comment_keeps_its_endings() {
        let (_, bytes) = restyle(
            b"/* one  \r\n * two  \r\n */",
            &rules(false, true),
            Markers::BUILTIN,
        )
        .unwrap();
        assert_eq!(bytes, b"/* one\r\n * two\r\n */");
    }

    #[test]
    fn bytes_that_are_not_utf8_are_never_rewritten() {
        assert!(restyle(b"//\xff\xfe   ", &rules(true, true), Markers::BUILTIN).is_none());
    }

    #[test]
    fn restyling_twice_is_restyling_once() {
        let rules = rules(true, true);
        for raw in [
            b"//note   ".as_slice(),
            b"/* one  \n * two  \n */",
            b"//! doc\t",
            b"#x",
        ] {
            let (_, once) = restyle(raw, &rules, Markers::BUILTIN).unwrap();
            assert!(
                restyle(&once, &rules, Markers::BUILTIN).is_none(),
                "{raw:?}"
            );
        }
    }
}
