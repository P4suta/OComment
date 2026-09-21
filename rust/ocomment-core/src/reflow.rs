//! Where the line breaks in a paragraph of comment prose go.
//!
//! The unit is the run, not the comment.
//! Four consecutive `///` lines are four comments to a scanner and one paragraph to a reader, and joining two of them moves the newline and the indentation between them — bytes that belong to neither comment.
//! That is why a rewrite here is recorded against a [`ProseRun`] and every other style rule is recorded against a comment.
//!
//! # What it will not touch
//!
//! A great deal, and deliberately.
//! A formatter that reflows a fenced code block, a table, a list, a rustdoc section heading or an intra-doc link reference has not tidied a comment; it has broken the page the comment was.
//! Every line this cannot read as ordinary prose is passed through byte for byte, and a paragraph ends at it.
//!
//! The classification is timid on purpose.
//! Reading prose as structure costs a line break nobody wanted; reading structure as prose costs the structure.
//!
//! # What a break means
//!
//! Three kinds of line ending occur in a paragraph.
//! A break after a sentence is meant, and stays.
//! A break after a clause — a comma, a colon, a dash — is also meant, because the rule this implements says it may be used where it clarifies structure,
//! and a fixer that removed breaks its own checker accepts would not be a fixer whose output is its checker's fixed point.
//! A break anywhere else exists to keep a line short, and a column is not a unit of meaning: those are the ones undone.

use crate::types::Wrap;

/// Reflow a run's body lines, or `None` when they are already as the rule asks.
///
/// `lines` are the comment bodies with their indentation, marker and line ending already taken off, in order.
/// The answer is the bodies to write back, which may be a different number of them.
#[must_use]
pub(crate) fn reflow(lines: &[&str], wrap: Wrap) -> Option<Vec<String>> {
    if !wrap.rewrites() {
        return None;
    }
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut held: Vec<&str> = Vec::new();
    /* NOTE: What the held lines are written back under: nothing for ordinary prose, and for a list item the marker on its first line and the width of that marker on the rest.
     * An item's continuation belongs to the item, and the indentation that says so is what the gate this replaces trimmed away. */
    let mut under = Marker::none();
    let mut fenced = false;
    for (index, line) in lines.iter().enumerate() {
        let opener = fence_marker(line);
        if fenced || opener.is_some() {
            flush(&mut out, &mut held, &under, wrap);
            under = Marker::none();
            out.push((*line).to_owned());
            if opener.is_some() {
                fenced = !fenced;
            }
            continue;
        }
        if let Some(marker) = opens_an_item(line) {
            flush(&mut out, &mut held, &under, wrap);
            under = marker;
            held.push(line.get(under.first.len()..).unwrap_or_default());
        /* NOTE: Not "and something is held".
         * A paragraph flushed at a clause break leaves nothing held, and the item it belonged to has not ended — asking for held lines here left every line after the first break unreachable. */
        } else if under.is_item() && continues_an_item(line, &under) {
            held.push(line.trim_start());
        } else if reads_as_prose(line) {
            if under.is_item() {
                flush(&mut out, &mut held, &under, wrap);
                under = Marker::none();
            }
            held.push(line);
        } else {
            flush(&mut out, &mut held, &under, wrap);
            under = Marker::none();
            out.push((*line).to_owned());
            continue;
        }
        /* NOTE: A break the writer meant ends what is being held, and the line that carries it is the last of it.
         * A break nobody meant is simply not a boundary, so the next line joins what is already there. */
        let carries_on = lines.get(index + 1).is_some_and(|next| {
            fence_marker(next).is_none()
                && opens_an_item(next).is_none()
                && (if under.is_item() {
                    continues_an_item(next, &under)
                } else {
                    reads_as_prose(next)
                })
        });
        if ends_at_a_break(line) || !carries_on {
            flush(&mut out, &mut held, &under, wrap);
            /* NOTE: A break inside an item does not end the item.
             * Its first line may end at a clause, and forgetting the marker there left every line under it unreachable — indented, so read as something the structure is made of rather than as the item's own prose.
             * What the next group loses is the marker itself, which has been written once already. */
            under = if carries_on && under.is_item() {
                Marker {
                    first: under.rest.clone(),
                    rest: under.rest,
                }
            } else {
                Marker::none()
            };
        }
    }
    flush(&mut out, &mut held, &under, wrap);
    (out.len() != lines.len() || out.iter().zip(lines).any(|(now, before)| now != before))
        .then_some(out)
}

/// What a paragraph is written back under.
///
/// Empty for ordinary prose.
/// For a list item, the marker its first line carries and the white space its continuations are written at, which is the marker's width: an item's continuation is part of the item, and a rewrite that wrote it back at the margin would have made it a paragraph of its own.
struct Marker {
    /// What goes in front of the first line.
    first: String,
    /// What goes in front of every line after it.
    rest: String,
}

impl Marker {
    /// The one ordinary prose is written under.
    fn none() -> Self {
        Self {
            first: String::new(),
            rest: String::new(),
        }
    }

    /// Whether this is a list item's rather than prose's.
    fn is_item(&self) -> bool {
        !self.first.is_empty()
    }
}

/// The marker a line opens a list item with, if it opens one.
fn opens_an_item(line: &str) -> Option<Marker> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || !opens_a_list_item(trimmed) {
        return None;
    }
    let indent = &line[..line.len() - trimmed.len()];
    let width = trimmed.find(' ').map_or(trimmed.len(), |at| {
        trimmed[at..].len() - trimmed[at..].trim_start().len() + at
    });
    let marker = &trimmed[..width];
    Some(Marker {
        first: format!("{indent}{marker}"),
        rest: format!("{indent}{}", " ".repeat(marker.chars().count())),
    })
}

/// Whether a line continues the item `under` opened.
///
/// Indented under the marker, and not far enough under it to be an example:
/// four spaces past the marker is an indented code block, and a doc comment is where an example lives.
fn continues_an_item(line: &str, under: &Marker) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || !reads_as_prose(trimmed) {
        return false;
    }
    let indent = line.len() - trimmed.len();
    indent >= 1 && indent <= under.rest.chars().count() + 1
}

/// Write out what is being held, as the rule asks.
fn flush(out: &mut Vec<String>, held: &mut Vec<&str>, under: &Marker, wrap: Wrap) {
    if held.is_empty() {
        return;
    }
    let joined = join(held);
    held.clear();
    let pieces = if wrap.breaks_sentences() {
        sentences(&joined)
    } else {
        vec![joined]
    };
    for (index, piece) in pieces.into_iter().enumerate() {
        let prefix = if index == 0 {
            &under.first
        } else {
            &under.rest
        };
        out.push(format!("{prefix}{piece}"));
    }
}

/// Put the held lines back together as one.
///
/// A space goes between them, except where both sides of the join are characters a script writes without one.
/// Japanese does not put a space between the end of one line and the start of the next, and a reflow that inserted one would be adding a character to the prose rather than moving a line break.
fn join(held: &[&str]) -> String {
    let mut out = String::new();
    for piece in held {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if !out.is_empty() && needs_a_space(out.chars().next_back(), piece.chars().next()) {
            out.push(' ');
        }
        out.push_str(piece);
    }
    out
}

/// Whether a join between these two characters needs a space put in it.
fn needs_a_space(before: Option<char>, after: Option<char>) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => {
            !(written_without_spaces(before) && written_without_spaces(after))
        }
        _ => false,
    }
}

/// Whether a character belongs to a script that writes no space between words.
///
/// The blocks, in the order they are listed: CJK punctuation, the Japanese kana, the CJK ideographs and their extension A, the Korean syllables, the CJK compatibility ideographs, and the halfwidth and fullwidth forms.
/// They are the ones a comment in this workspace is actually written in.
const fn written_without_spaces(character: char) -> bool {
    matches!(
        character as u32,
        0x3000..=0x303f
            | 0x3040..=0x30ff
            | 0x3400..=0x4dbf
            | 0x4e00..=0x9fff
            | 0xac00..=0xd7af
            | 0xf900..=0xfaff
            | 0xff00..=0xffef
    )
}

/// The fence token a line opens or closes a code block with, if it does.
///
/// Three or more backticks or tildes, under four spaces of indentation, which is CommonMark's rule and the one the Markdown scanner in this crate already reads.
pub(crate) fn fence_marker(line: &str) -> Option<char> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() >= 4 {
        return None;
    }
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    (trimmed.chars().take_while(|c| *c == marker).count() >= 3).then_some(marker)
}

/// Whether a line is ordinary prose, as opposed to something whose line break carries meaning.
///
/// Every `false` here is a line passed through byte for byte.
fn reads_as_prose(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return false;
    }
    /* NOTE: Four spaces is an indented code block, and a doc comment is where an example lives.
     * Two is the continuation of a list item, whose break belongs to the item. */
    if line.len() - trimmed.len() >= 2 {
        return false;
    }
    let first = trimmed.as_bytes()[0];
    /* NOTE: A heading, which in a Rust doc comment is a rustdoc section:
     * joining `# Errors` into the paragraph under it deletes the section. */
    if first == b'#' {
        return false;
    }
    // NOTE: A quote, a table row, a horizontal rule, a setext underline.
    if matches!(first, b'>' | b'|') || is_rule(trimmed) {
        return false;
    }
    if opens_a_list_item(trimmed) {
        return false;
    }
    /* NOTE: A documentation tag, which every doc comment convention spells differently and all of them put at the start of its own line. */
    if matches!(first, b'@' | b'\\' | b':') || opens_a_named_section(trimmed) {
        return false;
    }
    if opens_a_link_reference(trimmed) {
        return false;
    }
    // NOTE: Columns somebody lined up by hand.
    if is_aligned(trimmed) {
        return false;
    }
    !reads_as_code(trimmed)
}

/// Whether a line is a link reference definition, whose destination a join would put inside a sentence.
///
/// The destination is what tells one from prose that merely opens with a link.
/// A definition is `[label]: destination`, and a destination is one token; a Rust doc comment writing ``[`AllowRules::max_lines`]: removed with the run it belongs to`` is a sentence about a link and joins like any other.
/// Reading the second as a definition cost every such sentence its reflow.
fn opens_a_link_reference(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix('[') else {
        return false;
    };
    let Some((_, destination)) = rest.split_once("]:") else {
        return false;
    };
    let destination = destination.trim();
    !destination.is_empty() && !destination.contains(char::is_whitespace)
}

/// Whether a line is a horizontal rule or a setext underline.
fn is_rule(trimmed: &str) -> bool {
    let marker = trimmed.as_bytes()[0];
    matches!(marker, b'-' | b'=' | b'_' | b'*')
        && trimmed.len() >= 3
        && trimmed
            .bytes()
            .all(|byte| byte == marker || byte == b' ' || byte == b'\t')
}

/// Whether a line opens a list item.
fn opens_a_list_item(trimmed: &str) -> bool {
    let bytes = trimmed.as_bytes();
    if matches!(bytes[0], b'-' | b'*' | b'+') && matches!(bytes.get(1), Some(b' ')) {
        return true;
    }
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    digits > 0
        && matches!(bytes.get(digits), Some(b'.' | b')'))
        && matches!(bytes.get(digits + 1), Some(b' '))
}

/// Whether a line opens a named section of a documentation convention.
///
/// Google's Python style and its neighbours write `Args:` and `Returns:` at the start of a line and indent what belongs to them.
fn opens_a_named_section(trimmed: &str) -> bool {
    let Some(head) = trimmed.split(':').next() else {
        return false;
    };
    head.len() < trimmed.len()
        && !head.is_empty()
        && head.len() <= 12
        && head.chars().next().is_some_and(char::is_uppercase)
        && head.chars().all(char::is_alphabetic)
        && trimmed[head.len() + 1..].trim().is_empty()
}

/// Whether a line has two or more runs of two spaces in it, which is how a hand-aligned column looks and is not how prose looks.
fn is_aligned(trimmed: &str) -> bool {
    trimmed.split("  ").filter(|part| !part.is_empty()).count() > 2
}

/// Whether a line reads as code rather than as prose.
///
/// Deliberately timid, as the same question is elsewhere in this workspace:
/// calling prose code costs a line break, and calling code prose costs the code.
/// It answers yes only for text that ends the way a block ends.
///
/// A semicolon is not among them, though the same question answered elsewhere counts one.
/// Here it would contradict the rule this file implements: a semicolon ends a clause, the rule allows a break after one, and a line ending in a clause is prose by construction.
/// Counting it made every sentence that ended in a semicolon into code, and code is never reflowed.
fn reads_as_code(trimmed: &str) -> bool {
    trimmed.ends_with('{') || trimmed.ends_with('}')
}

/// Whether a line ends at a break somebody meant.
///
/// A sentence ender or a clause ender, with any closing quotes and brackets that hug it.
fn ends_at_a_break(line: &str) -> bool {
    let trimmed = line.trim_end();
    let mut chars = trimmed.chars().rev();
    for character in chars.by_ref() {
        if !is_closer(character) {
            return is_sentence_ender(character) || is_clause_ender(character);
        }
    }
    false
}

/// A character that may hug the end of a sentence without ending it.
const fn is_closer(character: char) -> bool {
    matches!(
        character,
        '"' | '\'' | ')' | ']' | '}' | '»' | '”' | '’' | '」' | '』' | '）' | '】'
    )
}

/// A character that ends a sentence.
const fn is_sentence_ender(character: char) -> bool {
    matches!(
        character,
        '.' | '!' | '?' | '。' | '．' | '！' | '？' | '…' | '‥'
    )
}

/// A character that ends a clause, which the rule allows a break after and therefore never removes one after.
const fn is_clause_ender(character: char) -> bool {
    matches!(
        character,
        ',' | ';' | ':' | '、' | '，' | '；' | '：' | '—' | '–'
    )
}

/// One line per sentence.
///
/// The Latin full stop is the hard one, because it is also a decimal point, an abbreviation, a version number and part of every host name there is.
/// It ends a sentence only when white space follows it and the word in front of it is not one of the words that always carry one.
fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    let characters: Vec<(usize, char)> = text.char_indices().collect();
    let mut index = 0;
    while index < characters.len() {
        let (offset, character) = characters[index];
        if in_code_span(text, offset) {
            index += 1;
            continue;
        }
        if !is_sentence_ender(character) || !ends_a_word(text, offset) {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while end < characters.len() && is_closer(characters[end].1) {
            end += 1;
        }
        let after = characters.get(end).map(|(_, c)| *c);
        let breaks = match character {
            /* NOTE: Latin convention puts a space after the ender, and that space is what tells `1.5` and `e.g.` and `example.com` from the end of a sentence.
             * CJK convention puts none, so any visible remainder is a second sentence. */
            '.' | '!' | '?' => {
                after.is_some_and(char::is_whitespace) && !abbreviation_before(text, offset)
            }
            _ => after.is_some(),
        };
        if !breaks {
            index = end;
            continue;
        }
        let cut = characters.get(end).map_or(text.len(), |(at, _)| *at);
        let piece = text[start..cut].trim();
        if !piece.is_empty() {
            out.push(piece.to_owned());
        }
        start = cut;
        index = end;
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        out.push(rest.to_owned());
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Whether the ender at `offset` comes after something a sentence can end after.
///
/// A sentence ends after a word, a closing quote or bracket, another ender, or a character of a script that writes no spaces.
/// It does not end in the middle of a token built out of punctuation, and `"//!"` is such a token: read without this, the `!` in it ends a sentence and the paragraph is broken in two around a marker somebody was naming.
fn ends_a_word(text: &str, offset: usize) -> bool {
    text[..offset].chars().next_back().is_some_and(|before| {
        before.is_alphanumeric()
            || is_closer(before)
            || is_sentence_ender(before)
            || written_without_spaces(before)
            || before == '`'
    })
}

/// The words that carry a full stop and do not end a sentence with it.
///
/// A single letter is one of them: `J. Smith` is a name, not two sentences.
const ABBREVIATIONS: [&str; 18] = [
    "e.g", "i.e", "etc", "cf", "vs", "al", "mr", "mrs", "ms", "dr", "prof", "fig", "no", "vol",
    "ch", "pp", "st", "approx",
];

/// Whether the word ending at `offset` is one of those.
fn abbreviation_before(text: &str, offset: usize) -> bool {
    let head = &text[..offset];
    let word = head
        .rsplit(|c: char| c.is_whitespace())
        .next()
        .unwrap_or_default();
    /* NOTE: The word without the punctuation somebody put in front of it.
     * A quoted initial is still an initial: `"J. Smith"` is a name, and the opening quote made the test see two characters and break the name in half. */
    let word = word.trim_start_matches(|c: char| !c.is_alphanumeric());
    let folded = word.to_ascii_lowercase();
    word.chars().count() == 1 || ABBREVIATIONS.contains(&folded.as_str())
}

/// Whether `offset` falls inside a backtick code span.
///
/// A `.` inside one is part of a path, an identifier or a version, and a break there would put a line ending inside something a reader reads as one token.
fn in_code_span(text: &str, offset: usize) -> bool {
    !text[..offset].matches('`').count().is_multiple_of(2)
}
