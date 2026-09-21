//! How a comment that survives is written.
//!
//! The other axis. [`ScanOptions::allow`](crate::ScanOptions::allow) decides
//! whether a comment stays; this decides how it reads once it has. The two are
//! deliberately not the same table: a comment that fails a condition of
//! survival is removed, and a comment that fails a rule here is rewritten, and
//! a reader adding a rule to a table whose entries have two different
//! consequences would have to guess which they were adding.
//!
//! # What a rewrite may touch
//!
//! Only the bytes inside the comment. The crate promises that "the only bytes
//! that move are the ones a comment occupied", and a rewrite keeps that
//! promise literally: the edit it plans replaces the comment's span and
//! nothing else, so the code around it, its indentation, and the line ending
//! after it are the same bytes afterwards.
//!
//! A comment whose bytes are not valid UTF-8 is never rewritten. The engine
//! does not decode the whole source, but it cannot reason about words without
//! decoding the comment, and guessing at a boundary inside bytes it could not
//! read is how a formatter corrupts a file it was asked to tidy.
//!
//! # The rules compose, and the first one recorded is the one that found
//! something
//!
//! [`restyle`] applies every rule the configuration asks for and returns the
//! bytes with all of them applied, together with the first rule that had
//! anything to do. That rule is what the comment records and what `--explain`
//! names: a reader is being told why the comment is in the report at all, and
//! the answer is the rule that put it there.

use crate::scanner::marker_bounds_with;
use crate::types::{StyleRule, StyleRules};

/// The delimiters a comment in this file opens and closes with.
///
/// Carried rather than guessed at. A file read under a declarative profile
/// opens its comments with the tokens the profile declares, and the built-in
/// list knows `--` but not Haddock's `-- |`: a rule about the text written
/// against the marker would have judged the space that belongs to the marker.
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
/// `raw` is the comment's complete bytes, delimiters included, exactly as
/// [`Comment::span`](crate::Comment::span) delimits them. The answer is `None`
/// when the rules find nothing to change — which is the ordinary case, and the
/// case a scan must be cheap in.
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

/// Put a space between the opening marker and the text written against it.
///
/// Deliberately timid, in the way [`crate`] is timid everywhere it reads text
/// rather than syntax: it acts only when the first character of the text is
/// neither white space nor ASCII punctuation. That leaves `// "quoted"` alone,
/// which is a small miss, and it leaves `////////`, `#####`, `//-----` and
/// `/*!` alone, which is the point — a ruler is not a comment missing its
/// space, and inserting one there turns a divider into a divider with a hole
/// in it.
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
/// The last line included: a line comment's span ends where its text ends, so
/// the spaces a `// note   ` trails are inside it. The space a *removal* would
/// leave behind is not — that is the layout's business, and this rule has no
/// opinion about it.
fn trailing_whitespace(raw: &[u8]) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(raw.len());
    let mut changed = false;
    let mut line = 0;
    for index in 0..=raw.len() {
        let terminator = index == raw.len() || raw[index] == b'\n';
        if !terminator {
            continue;
        }
        /* NOTE: `\r` is stripped with the rest and put back with the `\n`, so
         * a CRLF source keeps its endings and a comment that trailed spaces
         * before one loses only the spaces. */
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(space: bool, trailing: bool) -> StyleRules {
        StyleRules {
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
