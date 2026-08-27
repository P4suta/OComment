use crate::{
    ByteSpan, Language, Layout, ScanOptions, ScanReport, Severity, TransformOptions,
    TransformResult,
    scanner::{RestartRules, preamble_is_settled, scan_until_checkpoint, scan_with_checkpoints},
    transform::transform_report,
};
use thiserror::Error;

/// The units a client counts a position's `character` in.
///
/// This is the LSP `positionEncoding` capability. The engine's own
/// coordinates are always byte offsets; an encoding only says how to read
/// the numbers a client sends.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PositionEncoding {
    /// UTF-8 code units, which are the bytes themselves.
    Utf8,
    /// UTF-16 code units, the LSP default and this one too.
    #[default]
    Utf16,
    /// Unicode scalar values, one per character.
    Utf32,
}

/// One edit a client made to a document.
///
/// The spans of a batch address the document as it stands *before* the
/// batch, so a client never has to compensate for its own earlier changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentChange {
    /// The bytes to replace, empty to insert at that offset.
    pub span: ByteSpan,
    /// The bytes to put there, empty to delete.
    pub replacement: Vec<u8>,
}

/// A document that rescans only what an edit disturbed.
///
/// This is the path an editor takes, where a full scan on every keystroke
/// would be wasted work. The document keeps the previous revision's report
/// and its restart points, and an edit is answered by scanning from the last
/// safe point before it up to the first point where the new scan converges
/// with the old one; everything outside that window is reused, with the spans
/// past the edit shifted by its length delta.
///
/// The result is byte-for-byte the report [`scan`](crate::scan) would have
/// produced for the same bytes — a restart point is only used while the
/// bytes around it still permit one, and a scan that fails to converge
/// simply runs to the end. [`Self::last_rescan_span`] says how much of the
/// document the last edit actually cost.
///
/// # Examples
///
/// ```
/// use ocomment_core::{
///     ByteSpan, DocumentChange, IncrementalDocument, IncrementalError, Language, ScanOptions,
/// };
///
/// let mut document = IncrementalDocument::new(
///     b"let x = 1; // note\nlet y = 2;\n".to_vec(),
///     Language::Rust,
///     ScanOptions::default(),
///     1,
/// );
/// assert_eq!(document.report().comments.len(), 1);
///
/// // Type a second comment onto the end of the second line.
/// let end = document.source().len() - 1;
/// let report = document
///     .apply_changes(
///         &[DocumentChange {
///             span: ByteSpan::new(end, end),
///             replacement: b" // more".to_vec(),
///         }],
///         2,
///     )
///     .unwrap();
/// assert_eq!(report.comments.len(), 2);
///
/// // A batch that fails validation changes nothing, the version included.
/// assert_eq!(
///     document.apply_changes(&[], 2),
///     Err(IncrementalError::StaleVersion {
///         received: 2,
///         current: 2,
///     }),
/// );
/// assert_eq!(document.version(), 2);
/// assert_eq!(document.report().comments.len(), 2);
/// ```
#[derive(Clone, Debug)]
pub struct IncrementalDocument {
    source: Vec<u8>,
    language: Language,
    options: ScanOptions,
    report: ScanReport,
    checkpoints: Vec<usize>,
    safe_checkpoints: Vec<usize>,
    version: i64,
    last_rescan: ByteSpan,
}

/// Why an edit or a position was refused.
///
/// Every one of these is raised before the document is touched, so a refused
/// call leaves the previous revision intact.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum IncrementalError {
    /// The batch's version does not advance on the current one, so it
    /// describes a revision that has already been overtaken.
    #[error("stale document version {received}; current version is {current}")]
    StaleVersion {
        /// The version the batch claimed.
        received: i64,
        /// The version the document is already at.
        current: i64,
    },
    /// A change is inverted, starts before its predecessor ends, or reaches
    /// past the end of the document.
    #[error("change span lies outside the document")]
    InvalidSpan,
    /// The line does not exist, or `character` does not land on a boundary of
    /// the encoding it was counted in.
    #[error("position does not lie on a valid encoding boundary")]
    InvalidPosition,
}

impl IncrementalDocument {
    /// Scan `source` once and hold on to what it takes to rescan cheaply.
    ///
    /// `version` is the client's revision number for these bytes; every later
    /// [`Self::apply_changes`] has to advance on it.
    pub fn new(source: Vec<u8>, language: Language, options: ScanOptions, version: i64) -> Self {
        let (report, safe_checkpoints) =
            scan_with_checkpoints(&source, language, options.clone(), 0);
        let checkpoints = line_checkpoints(&source);
        let last_rescan = ByteSpan::new(0, source.len());
        Self {
            source,
            language,
            options,
            report,
            checkpoints,
            safe_checkpoints,
            version,
            last_rescan,
        }
    }

    /// The current bytes of the document.
    pub fn source(&self) -> &[u8] {
        &self.source
    }
    /// The scan of the current bytes.
    pub fn report(&self) -> &ScanReport {
        &self.report
    }
    /// The language the document is scanned as, fixed when it was created.
    pub const fn language(&self) -> Language {
        self.language
    }
    /// The options the document is scanned under, fixed when it was created.
    pub fn scan_options(&self) -> &ScanOptions {
        &self.options
    }
    /// The bytes a removal would write, from the report already in hand.
    ///
    /// No comment is scanned again: this is the current report run through the
    /// same layout and source-map engine [`transform`](crate::transform) uses.
    /// A YAML document does get one extra lexical pass in there, because where
    /// a block scalar body ends decides which comment lines a removal has to
    /// take whole and no report carries that; it is linear, like the edit walk
    /// beside it, and every other language skips it on the language check.
    pub fn transform(&self, layout: Layout) -> TransformResult {
        transform_report(
            &self.source,
            self.report.clone(),
            TransformOptions {
                scan: self.options.clone(),
                layout,
            },
        )
    }
    /// The revision number of the current bytes.
    pub const fn version(&self) -> i64 {
        self.version
    }
    /// The stretch of the current bytes the last edit had to rescan.
    ///
    /// A fresh document reports the whole source. An edit that reused both
    /// ends reports only the window between them, which is what makes the
    /// saving measurable rather than assumed.
    pub const fn last_rescan_span(&self) -> ByteSpan {
        self.last_rescan
    }
    /// The byte offset each line starts at, `0` first.
    ///
    /// A CRLF pair counts as one terminator, so `checkpoints()[n]` is where
    /// line `n` begins for [`Self::byte_offset`].
    pub fn checkpoints(&self) -> &[usize] {
        &self.checkpoints
    }
    /// The offsets a rescan may restart from and still reproduce a full scan.
    ///
    /// Far fewer than [`Self::checkpoints`]: a line start only qualifies while
    /// the scanner is in a clean top-level state there and the bytes around it
    /// keep it that way.
    pub fn safe_checkpoints(&self) -> &[usize] {
        &self.safe_checkpoints
    }

    /// Apply a sorted, non-overlapping batch whose spans refer to the current
    /// document snapshot. Validation is transactional: an invalid batch does
    /// not alter the source, report, checkpoints, or version.
    ///
    /// An empty batch is accepted and only advances the version, which is what
    /// a client that saved without typing sends.
    ///
    /// # Errors
    ///
    /// [`IncrementalError::StaleVersion`] when `version` does not advance on
    /// the current one, and [`IncrementalError::InvalidSpan`] when a change is
    /// inverted, starts before its predecessor ends, or reaches past the end
    /// of the document. Both are raised before anything is written.
    ///
    /// # Examples
    ///
    /// See [`IncrementalDocument`] for a worked edit.
    pub fn apply_changes(
        &mut self,
        changes: &[DocumentChange],
        version: i64,
    ) -> Result<&ScanReport, IncrementalError> {
        if version <= self.version {
            return Err(IncrementalError::StaleVersion {
                received: version,
                current: self.version,
            });
        }
        let earliest = changes
            .first()
            .map_or(self.source.len(), |change| change.span.start);
        let mut cursor = 0usize;
        for change in changes {
            if change.span.start > change.span.end
                || change.span.start < cursor
                || change.span.end > self.source.len()
            {
                return Err(IncrementalError::InvalidSpan);
            }
            cursor = change.span.end;
        }
        if changes.is_empty() {
            self.version = version;
            self.last_rescan = ByteSpan::new(self.source.len(), self.source.len());
            return Ok(&self.report);
        }
        let output_len = changes.iter().fold(self.source.len(), |length, change| {
            length
                .saturating_sub(change.span.len())
                .saturating_add(change.replacement.len())
        });
        let mut next = Vec::with_capacity(output_len);
        cursor = 0;
        for change in changes {
            next.extend_from_slice(&self.source[cursor..change.span.start]);
            next.extend_from_slice(&change.replacement);
            cursor = change.span.end;
        }
        let old_tail_start = cursor;
        let new_tail_start = next.len();
        next.extend_from_slice(&self.source[cursor..]);
        let can_reuse = self.report.valid;
        let safe_start = if !can_reuse {
            0
        } else {
            /* INVARIANT: The checkpoints belong to the *previous* revision, and a
             * checkpoint is only a restart point while the bytes around it
             * still allow one: an edit that turns line 2 into a Python encoding
             * declaration, or that splices two C lines together, withdraws that
             * permission. Every candidate is therefore re-asked against the
             * edited document, falling back to an earlier checkpoint and
             * ultimately to a full scan. */
            let rules = RestartRules::of(&next, self.language);
            let usable = self
                .safe_checkpoints
                .partition_point(|point| *point <= earliest);
            self.safe_checkpoints[..usable]
                .iter()
                .copied()
                .rev()
                .find(|point| rules.permit_restart_at(&next, *point))
                .unwrap_or(0)
        };
        let old_convergence = if can_reuse {
            /* INVARIANT: Converging keeps the previous revision's report for every byte
             * past the convergence point, shifted by the edit's length delta —
             * including each comment's kind. Only the preamble rules care where
             * a comment sits, so the tail may be reused exactly while it lies
             * past the preamble both where it was and where the edit moves it;
             * otherwise the scan runs on to the first checkpoint that does. */
            self.safe_checkpoints.iter().copied().find(|point| {
                *point >= old_tail_start.max(safe_start)
                    && preamble_is_settled(&self.source, *point)
                    && preamble_is_settled(&next, new_tail_start + point - old_tail_start)
            })
        } else {
            None
        };
        let mut reused_tail = None;
        let mut partial = None;
        if let Some(old_convergence) = old_convergence {
            let new_convergence = new_tail_start + old_convergence - old_tail_start;
            /* INVARIANT: The scanner is handed the whole suffix, never a slice cut at the
             * convergence point: lexical lookahead that reaches past the cut
             * would otherwise decide differently than it does in the real
             * document and the rescan would lose comments or diagnostics. */
            let (report, checkpoints, converged) = scan_until_checkpoint(
                &next[safe_start..],
                self.language,
                self.options.clone(),
                safe_start,
                new_convergence,
            );
            if converged {
                reused_tail = Some((old_convergence, new_convergence));
                partial = Some((report, checkpoints, new_convergence));
            } else {
                /* NOTE: Lexical state diverged, so the scan already ran to the end of
                 * the suffix; that report is exactly the fallback. */
                partial = Some((report, checkpoints, next.len()));
            }
        }
        let (mut suffix, suffix_checkpoints, rescan_end) = partial.unwrap_or_else(|| {
            let (report, checkpoints) = scan_with_checkpoints(
                &next[safe_start..],
                self.language,
                self.options.clone(),
                safe_start,
            );
            (report, checkpoints, next.len())
        });
        let mut comments: Vec<_> = self
            .report
            .comments
            .iter()
            .take_while(|comment| comment.span.start < safe_start && comment.span.end <= safe_start)
            .cloned()
            .collect();
        comments.append(&mut suffix.comments);
        if let Some((old_convergence, new_convergence)) = reused_tail {
            comments.extend(
                self.report
                    .comments
                    .iter()
                    .filter(|comment| comment.span.start >= old_convergence)
                    .cloned()
                    .map(|mut comment| {
                        comment.span =
                            shift_tail_span(comment.span, old_convergence, new_convergence);
                        comment
                    }),
            );
        }
        let mut diagnostics: Vec<_> = self
            .report
            .diagnostics
            .iter()
            .take_while(|diagnostic| {
                diagnostic.span.start < safe_start && diagnostic.span.end <= safe_start
            })
            .cloned()
            .collect();
        diagnostics.append(&mut suffix.diagnostics);
        if let Some((old_convergence, new_convergence)) = reused_tail {
            diagnostics.extend(
                self.report
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.span.start >= old_convergence)
                    .cloned()
                    .map(|mut diagnostic| {
                        diagnostic.span =
                            shift_tail_span(diagnostic.span, old_convergence, new_convergence);
                        diagnostic
                    }),
            );
        }
        let report = ScanReport {
            language: self.language,
            valid: !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error),
            comments,
            diagnostics,
        };
        let mut safe_checkpoints: Vec<_> = self
            .safe_checkpoints
            .iter()
            .copied()
            .take_while(|point| *point < safe_start)
            .collect();
        safe_checkpoints.extend(suffix_checkpoints);
        if let Some((old_convergence, new_convergence)) = reused_tail {
            /* INVARIANT: the converged tail's checkpoints come from the previous
             * revision, and an edit can grow a lexical construct — a `<` tag,
             * a quote pair, an XML literal — across one, so each is re-asked
             * against the edited document exactly as a candidate restart is. */
            let rules = RestartRules::of(&next, self.language);
            safe_checkpoints.extend(
                self.safe_checkpoints
                    .iter()
                    .copied()
                    .filter(|point| *point > old_convergence)
                    .map(|point| new_convergence + point - old_convergence)
                    .filter(|point| rules.permit_restart_at(&next, *point)),
            );
        }
        safe_checkpoints.dedup();
        let checkpoints = line_checkpoints(&next);
        self.last_rescan = ByteSpan::new(safe_start, rescan_end);
        self.source = next;
        self.report = report;
        self.checkpoints = checkpoints;
        self.safe_checkpoints = safe_checkpoints;
        self.version = version;
        Ok(&self.report)
    }

    /// The byte offset of a line-and-character position.
    ///
    /// `line` is zero-based, and `character` is a zero-based offset into that
    /// line counted in the units `encoding` names. The end of a line is a
    /// valid position; the terminator itself is not part of the line.
    ///
    /// # Errors
    ///
    /// [`IncrementalError::InvalidPosition`] when the line does not exist,
    /// when `character` reaches past the end of the line, or when it lands
    /// inside a character instead of on a boundary. A line whose bytes are
    /// not valid UTF-8 has no UTF-16 or UTF-32 positions at all.
    pub fn byte_offset(
        &self,
        line: u32,
        character: u32,
        encoding: PositionEncoding,
    ) -> Result<usize, IncrementalError> {
        let start = *self
            .checkpoints
            .get(line as usize)
            .ok_or(IncrementalError::InvalidPosition)?;
        let raw_end = self
            .checkpoints
            .get(line as usize + 1)
            .copied()
            .unwrap_or(self.source.len());
        let end = if raw_end > start && self.source.get(raw_end - 1) == Some(&b'\n') {
            if raw_end > start + 1 && self.source.get(raw_end - 2) == Some(&b'\r') {
                raw_end - 2
            } else {
                raw_end - 1
            }
        } else if raw_end > start && self.source.get(raw_end - 1) == Some(&b'\r') {
            raw_end - 1
        } else {
            raw_end
        };
        let line_bytes = &self.source[start..end];
        match encoding {
            PositionEncoding::Utf8 => {
                let offset = start + character as usize;
                if offset <= end && std::str::from_utf8(&self.source[start..offset]).is_ok() {
                    Ok(offset)
                } else {
                    Err(IncrementalError::InvalidPosition)
                }
            }
            PositionEncoding::Utf16 | PositionEncoding::Utf32 => {
                let text = std::str::from_utf8(line_bytes)
                    .map_err(|_| IncrementalError::InvalidPosition)?;
                let mut units = 0u32;
                for (relative, ch) in text.char_indices() {
                    if units == character {
                        return Ok(start + relative);
                    }
                    units += if encoding == PositionEncoding::Utf16 {
                        ch.len_utf16() as u32
                    } else {
                        1
                    };
                    if units > character {
                        return Err(IncrementalError::InvalidPosition);
                    }
                }
                if units == character {
                    Ok(end)
                } else {
                    Err(IncrementalError::InvalidPosition)
                }
            }
        }
    }
}

fn shift_tail_span(span: ByteSpan, old_base: usize, new_base: usize) -> ByteSpan {
    ByteSpan::new(
        new_base + span.start - old_base,
        new_base + span.end - old_base,
    )
}

fn line_checkpoints(source: &[u8]) -> Vec<usize> {
    let mut lines = vec![0];
    let mut index = 0;
    while index < source.len() {
        if source[index] == b'\r' && source.get(index + 1) == Some(&b'\n') {
            index += 2;
            lines.push(index);
        } else if matches!(source[index], b'\r' | b'\n') {
            index += 1;
            lines.push(index);
        } else {
            index += 1;
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Disposition,
        scanner::{scan_checkpoint_watermarks, scan_with_checkpoints},
    };
    use proptest::{prelude::*, sample::select};

    /// A pool length as a `prop_oneof!` weight, so that drawing uniformly from
    /// a pool of `n` gives each of its members the weight one arm would have.
    fn weight(length: usize) -> u32 {
        u32::try_from(length).expect("the pool is far smaller than a weight")
    }

    /// One byte of the shared pool, or a uniformly random one.
    ///
    /// The pool is [`crate::lexical_pool::BYTES`], and `tests/properties.rs`
    /// draws from the same one: a fragment worth generating against the
    /// whole-file scanner is worth generating against the incremental one. The
    /// extra `\n` arm doubles that byte's weight, because a line boundary is
    /// where a checkpoint may be offered and every one of them is a restart
    /// this suite gets to try.
    fn lexical_byte() -> impl Strategy<Value = u8> {
        prop_oneof![
            4 => any::<u8>(),
            1 => Just(b'\n'),
            weight(crate::lexical_pool::BYTES.len()) => select(crate::lexical_pool::BYTES),
        ]
    }

    /// A fragment: one byte of the pool, or one whole token from it.
    ///
    /// The tokens are [`crate::lexical_pool::TOKENS`] — multi-byte openers a
    /// single-byte alphabet can never synthesise — and each is drawn as often
    /// as one byte is, which is what the eight-to-one weight in front of the
    /// byte arm keeps in proportion.
    fn lexical_fragment() -> impl Strategy<Value = Vec<u8>> {
        prop_oneof![
            8 => lexical_byte().prop_map(|byte| vec![byte]),
            weight(crate::lexical_pool::TOKENS.len()) => select(crate::lexical_pool::TOKENS)
                .prop_map(<[u8]>::to_vec),
        ]
    }

    /// A source built from at most `fragments` raw bytes and literal tokens.
    fn lexical_source(fragments: std::ops::Range<usize>) -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(lexical_fragment(), fragments)
            .prop_map(|fragments| fragments.concat())
    }

    /// One end of an edit span, drawn with a heavy bias towards the two
    /// document boundaries. The degenerate spans live there — an empty edit at
    /// offset 0, an append at the end, a replacement that swallows the whole
    /// document — and a uniform draw finds them only as often as it finds any
    /// other single offset.
    fn edit_endpoint() -> impl Strategy<Value = usize> {
        prop_oneof![
            1 => Just(0usize),
            1 => Just(usize::MAX),
            2 => any::<usize>(),
        ]
    }

    /// Place a drawn endpoint in `source`. The document length is unknown when
    /// the endpoint is drawn, so `usize::MAX` is the name of its end; every
    /// other draw wraps into the document and keeps the interior offsets
    /// spread evenly.
    fn endpoint(source: &[u8], drawn: usize) -> usize {
        if drawn == usize::MAX {
            source.len()
        } else {
            drawn % (source.len() + 1)
        }
    }

    /// The endpoint mapping has to reach both boundaries exactly, or the
    /// biased draws below would still miss the degenerate spans they exist to
    /// produce.
    #[test]
    fn an_edit_endpoint_reaches_both_document_boundaries() {
        let source = b"// comment\n";
        assert_eq!(endpoint(source, 0), 0);
        assert_eq!(endpoint(source, usize::MAX), source.len());
        assert_eq!(endpoint(source, source.len()), source.len());
        assert_eq!(endpoint(b"", usize::MAX), 0);
        assert!(endpoint(source, 12345) <= source.len());
    }

    proptest! {
        /* NOTE: Unit-test proptests cannot persist regressions next to `src`, so the
         * shrunk counterexample is reported inline instead. */
        #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

        /// Every safe checkpoint must be a restart point: scanning the suffix
        /// that begins there, at that offset, has to reproduce exactly the part
        /// of the full scan that begins there. The incremental engine reuses the
        /// prefix of the previous report on the strength of this invariant, so a
        /// checkpoint that is not a clean lexical state silently corrupts a
        /// rescan.
        ///
        /// The rescan is the observable half. The other half is the mechanism
        /// under it: no checkpoint may stand at or before the furthest byte any
        /// decision made before it consulted. A rescan that agrees only because
        /// the lookahead which read across the checkpoint happens to re-lex the
        /// same bytes and reach the same answer is agreeing by luck, and the
        /// luck runs out at the next lookahead — so the watermark is asserted
        /// directly, per language, before the restarts are tried.
        #[test]
        fn safe_checkpoints_restart_every_builtin_scan_exactly(
            source in lexical_source(0..48),
        ) {
            for language in Language::ALL {
                for (point, consulted) in
                    scan_checkpoint_watermarks(&source, language, ScanOptions::default())
                {
                    prop_assert!(
                        consulted <= point,
                        "{} offers a checkpoint at {} that a decision before it read through {}",
                        language, point, consulted,
                    );
                }
                let (full, checkpoints) =
                    scan_with_checkpoints(&source, language, ScanOptions::default(), 0);
                for point in checkpoints.iter().copied() {
                    let (suffix, suffix_checkpoints) = scan_with_checkpoints(
                        &source[point..],
                        language,
                        ScanOptions::default(),
                        point,
                    );
                    let comments: Vec<_> = full
                        .comments
                        .iter()
                        .filter(|comment| comment.span.start >= point)
                        .cloned()
                        .collect();
                    let diagnostics: Vec<_> = full
                        .diagnostics
                        .iter()
                        .filter(|diagnostic| diagnostic.span.start >= point)
                        .cloned()
                        .collect();
                    let tail: Vec<_> = checkpoints
                        .iter()
                        .copied()
                        .filter(|candidate| *candidate >= point)
                        .collect();
                    prop_assert_eq!(
                        &suffix.comments, &comments,
                        "{} comments diverge restarting at {}", language, point,
                    );
                    prop_assert_eq!(
                        &suffix.diagnostics, &diagnostics,
                        "{} diagnostics diverge restarting at {}", language, point,
                    );
                    prop_assert_eq!(
                        &suffix_checkpoints, &tail,
                        "{} checkpoints diverge restarting at {}", language, point,
                    );
                }
            }
        }

        /// The cross-edit form of the same invariant. A checkpoint is chosen
        /// from the *previous* document's list, so it also has to survive the
        /// edit: after `apply_changes` the document must be indistinguishable
        /// from a full scan of the edited bytes — comments, diagnostics,
        /// validity and the checkpoint list alike.
        #[test]
        fn arbitrary_edits_leave_every_builtin_document_equal_to_a_full_scan(
            source in lexical_source(0..48),
            replacement in lexical_source(0..8),
            first in edit_endpoint(),
            second in edit_endpoint(),
        ) {
            let left = endpoint(&source, first);
            let right = endpoint(&source, second);
            let span = ByteSpan::new(left.min(right), left.max(right));
            for language in Language::ALL {
                let mut document = IncrementalDocument::new(
                    source.clone(),
                    language,
                    ScanOptions::default(),
                    1,
                );
                document.apply_changes(&[DocumentChange {
                    span,
                    replacement: replacement.clone(),
                }], 2).unwrap();
                let (full, checkpoints) = scan_with_checkpoints(
                    document.source(),
                    language,
                    ScanOptions::default(),
                    0,
                );
                prop_assert_eq!(
                    &document.report().comments, &full.comments,
                    "{} comments diverge after editing {:?}", language, span,
                );
                prop_assert_eq!(
                    &document.report().diagnostics, &full.diagnostics,
                    "{} diagnostics diverge after editing {:?}", language, span,
                );
                prop_assert_eq!(
                    document.report().valid, full.valid,
                    "{} validity diverges after editing {:?}", language, span,
                );
                prop_assert_eq!(
                    document.report(), &full,
                    "{} report diverges after editing {:?}", language, span,
                );
                prop_assert_eq!(
                    document.safe_checkpoints(), &checkpoints[..],
                    "{} checkpoints diverge after editing {:?}", language, span,
                );
            }
        }
    }

    /// Regression: Python emitted a safe checkpoint at the start of line 2, but
    /// an encoding declaration is only recognised while scanning from offset 0,
    /// so a rescan that restarted there demoted `# coding:` from `Encoding`
    /// (kept) to a plain line comment (removed).
    #[test]
    fn a_rescan_never_demotes_a_second_line_python_encoding_declaration() {
        let source = b"value = 1\n# coding: latin-1\ntail = 2\n".to_vec();
        let mut document =
            IncrementalDocument::new(source, Language::Python, ScanOptions::default(), 1);
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(26, 27),
                    replacement: b"2".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) = scan_with_checkpoints(
            document.source(),
            Language::Python,
            ScanOptions::default(),
            0,
        );
        assert_eq!(document.report().comments, expected.comments);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);
    }

    /// Ruby reads a source-encoding declaration out of the same two lines
    /// Python does, and only while scanning from offset 0, so the start of line
    /// 2 is a restart point for it under exactly the same condition. A rescan
    /// that restarted there anyway would demote `# coding:` from `Encoding`
    /// (kept) to a plain line comment (removed).
    #[test]
    fn a_rescan_never_demotes_a_second_line_ruby_encoding_declaration() {
        let source = b"value = 1\n# coding: latin-1\ntail = 2\n".to_vec();
        let mut document =
            IncrementalDocument::new(source, Language::Ruby, ScanOptions::default(), 1);
        /* NOTE: The edit falls inside the declaration itself, so the last
         * checkpoint before it is the start of line 2 — the one offset this
         * rule exists to refuse. */
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(26, 27),
                    replacement: b"2".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(document.source(), Language::Ruby, ScanOptions::default(), 0);
        assert_eq!(
            document.report().comments[0].kind,
            crate::CommentKind::Encoding,
            "{:?}",
            document.report().comments,
        );
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);
    }

    /// A here document body and an embedded document are two Ruby states whose
    /// lines say nothing about themselves: the `#` at the head of one is a byte
    /// of the value, and the line that decides so sits above it. Neither offers
    /// a restart point, so the checkpoints of such a file are the line starts
    /// outside them and nothing else.
    #[test]
    fn a_ruby_line_start_inside_an_opaque_construct_is_no_restart_point() {
        let heredoc = IncrementalDocument::new(
            b"a = <<~EOS\n  # opaque\n  EOS\nb = 2\n".to_vec(),
            Language::Ruby,
            ScanOptions::default(),
            1,
        );
        assert_eq!(heredoc.safe_checkpoints(), [0, 28, 34]);

        let document = IncrementalDocument::new(
            b"=begin\n# opaque\n=end\nb = 2\n".to_vec(),
            Language::Ruby,
            ScanOptions::default(),
            1,
        );
        assert_eq!(document.safe_checkpoints(), [0, 21, 27]);

        /* NOTE: The DATA section behind the marker is not source at all, so the
         * marker's own line break is the last restart point there is. */
        let data = IncrementalDocument::new(
            b"a = 1\n__END__\nnot source\n".to_vec(),
            Language::Ruby,
            ScanOptions::default(),
            1,
        );
        assert_eq!(data.safe_checkpoints(), [0, 6]);
    }

    /// Regression: `safe_start` was chosen from the *previous* document's
    /// checkpoint list and never re-validated against the edited bytes, so an
    /// edit that turned line 2 into a Python encoding declaration restarted the
    /// scan at a checkpoint the edited document no longer admits and demoted
    /// the declaration from `Encoding` (kept) to a line comment (removed).
    #[test]
    fn an_edit_that_creates_an_encoding_declaration_invalidates_the_reused_checkpoint() {
        let source = b"value = 1\n# note\ntail\n".to_vec();
        let mut document =
            IncrementalDocument::new(source, Language::Python, ScanOptions::default(), 1);
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(10, 16),
                    replacement: b"# coding: latin-1".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) = scan_with_checkpoints(
            document.source(),
            Language::Python,
            ScanOptions::default(),
            0,
        );
        assert_eq!(
            document.report().comments[0].kind,
            crate::CommentKind::Encoding
        );
        assert!(!document.report().comments[0].disposition.is_remove());
        assert_eq!(document.report().comments, expected.comments);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);
    }

    /// The same shape, stated without naming a language: whenever an edit
    /// rewrites a line whose bytes decide whether an earlier checkpoint is a
    /// restart point, the engine must agree with a full scan of the edited
    /// document. Only Python's encoding rule has that property today, so the
    /// loop also guards every other built-in against acquiring one silently.
    #[test]
    fn edits_to_a_preamble_line_never_reuse_a_checkpoint_the_edit_invalidates() {
        for language in Language::ALL {
            for replacement in [
                &b"# coding: latin-1"[..],
                b"# -*- coding: utf-8 -*-",
                b"#!/bin/sh",
                b"//go:build linux",
                b"/*#__PURE__*/",
            ] {
                let mut document = IncrementalDocument::new(
                    b"value = 1\n# note\ntail\n".to_vec(),
                    language,
                    ScanOptions::default(),
                    1,
                );
                document
                    .apply_changes(
                        &[DocumentChange {
                            span: ByteSpan::new(10, 16),
                            replacement: replacement.to_vec(),
                        }],
                        2,
                    )
                    .unwrap();
                let (expected, expected_checkpoints) =
                    scan_with_checkpoints(document.source(), language, ScanOptions::default(), 0);
                let token = String::from_utf8_lossy(replacement).into_owned();
                assert_eq!(
                    document.report(),
                    &expected,
                    "{language} report diverges after inserting {token}",
                );
                assert_eq!(
                    document.safe_checkpoints(),
                    expected_checkpoints,
                    "{language} checkpoints diverge after inserting {token}",
                );
            }
        }
    }

    /// Regression: the reused *tail* carries the previous revision's
    /// classification, and a shebang is a shebang only at absolute offset 0.
    /// Inserting a line in front of one used to shift the old `Shebang` comment
    /// down and keep it, where a full scan of the edited bytes sees an ordinary
    /// line comment.
    #[test]
    fn an_edit_that_pushes_a_shebang_off_offset_zero_stops_reusing_its_kind() {
        let mut document = IncrementalDocument::new(
            b"#!/bin/sh\nvalue\n".to_vec(),
            Language::Shell,
            ScanOptions::default(),
            1,
        );
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(0, 0),
                    replacement: b"\n".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) = scan_with_checkpoints(
            document.source(),
            Language::Shell,
            ScanOptions::default(),
            0,
        );
        assert_eq!(document.report().comments, expected.comments);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);
    }

    /// The mirror image: deleting the lines in front of a `#!` line pulls it to
    /// offset 0, where a full scan reads a shebang, so the previous revision's
    /// ordinary line comment must not be reused either.
    #[test]
    fn an_edit_that_pulls_a_hashbang_line_to_offset_zero_stops_reusing_its_kind() {
        let mut document = IncrementalDocument::new(
            b"x\n#!/bin/sh\ntail\n".to_vec(),
            Language::Shell,
            ScanOptions::default(),
            1,
        );
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(0, 2),
                    replacement: Vec::new(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) = scan_with_checkpoints(
            document.source(),
            Language::Shell,
            ScanOptions::default(),
            0,
        );
        assert_eq!(document.report().comments, expected.comments);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);
    }

    /// Regression: C and C++ splice `\\<newline>` out of the input before
    /// lexing, and a spliced document is scanned through a remapped copy that
    /// tracks no checkpoints at all — a full scan of it offers offset 0 and
    /// nothing else. An edit that introduces a splice therefore invalidates
    /// every checkpoint the previous revision recorded, but the reused one was
    /// never re-checked against the edited bytes, so the document went on
    /// advertising a restart point the edited source no longer has.
    #[test]
    fn an_edit_that_introduces_a_c_line_splice_invalidates_every_checkpoint() {
        let mut document = IncrementalDocument::new(
            b"int a;\nx\n/ hidden\nint c;\n".to_vec(),
            Language::C,
            ScanOptions::default(),
            1,
        );
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(7, 8),
                    replacement: b"/\\".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(document.source(), Language::C, ScanOptions::default(), 0);
        assert_eq!(document.report().comments, expected.comments);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);
    }

    /// Regression: how far a YAML block scalar body reaches is decided by the
    /// lines below it, so an edit under one can swallow an offset the previous
    /// revision recorded as a line start — appending a line to a document that
    /// ended inside a body is enough, and a restart there would read the
    /// content of a scalar as YAML. No body begins before its own header, so
    /// the checkpoints a YAML document offers stop at the first one, and a
    /// document that opens none offers every line start as before.
    #[test]
    fn a_yaml_block_scalar_ends_the_checkpoints_of_the_document_it_opens() {
        let plain = IncrementalDocument::new(
            b"a: 1\nb: 2 # note\n".to_vec(),
            Language::Yaml,
            ScanOptions::default(),
            1,
        );
        assert_eq!(plain.safe_checkpoints(), [0, 5, 17]);

        let mut document = IncrementalDocument::new(
            b"key: |\n  body # content\n".to_vec(),
            Language::Yaml,
            ScanOptions::default(),
            1,
        );
        assert_eq!(document.safe_checkpoints(), [0]);
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(24, 24),
                    replacement: b"  more # content\n".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(document.source(), Language::Yaml, ScanOptions::default(), 0);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);
        assert!(
            document.report().comments.is_empty(),
            "the body swallowed both lines: {:?}",
            document.report().comments
        );
    }

    /// The keep a block scalar's trail decides is a property of the whole
    /// document, so a rescan that reuses a tail has to reach the same one. The
    /// checkpoints a YAML document offers stop at its first block scalar, which
    /// puts every trail inside the suffix a rescan reads or inside the tail it
    /// carries over untouched; either way the answer is the full scan's.
    #[test]
    fn a_yaml_structural_trail_keep_survives_an_incremental_rescan() {
        let source = b"a: 1
k: |
  x
# ends the block
  # yamllint disable
z: 1
";
        let mut document =
            IncrementalDocument::new(source.to_vec(), Language::Yaml, ScanOptions::default(), 1);
        assert_eq!(
            document.report().comments[0].disposition,
            Disposition::Keep {
                reason: "structural in a YAML block scalar trail".to_owned()
            },
        );
        /* NOTE: Deepening the body past the directive under it takes the value
         * away from that directive, and the comment above it stops being
         * structure the moment it does. */
        let deepen = source
            .windows(4)
            .position(|window| window == b"\n  x")
            .expect("the body line");
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(deepen + 1, deepen + 1),
                    replacement: b"  ".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(document.source(), Language::Yaml, ScanOptions::default(), 0);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);
        assert!(
            document.report().comments[0].disposition.is_remove(),
            "the directive is outside the deeper body: {:?}",
            document.report().comments,
        );
    }

    /// PHP mode is document state rather than line state: whether the `#` at a
    /// line start opens a comment depends on whether an unclosed `<?php` sits
    /// above it, and the bytes of the line itself say nothing about that. Only
    /// a line break the scanner meets in inline HTML is a restart point, so a
    /// file that is all PHP offers offset 0 and nothing else and a template
    /// offers the line starts of its HTML.
    #[test]
    fn a_php_line_start_is_a_restart_point_only_in_inline_html() {
        let html = IncrementalDocument::new(
            b"<p>a</p>\n<p>b</p>\n".to_vec(),
            Language::Php,
            ScanOptions::default(),
            1,
        );
        assert_eq!(html.safe_checkpoints(), [0, 9, 18]);

        let code = IncrementalDocument::new(
            b"<?php\n$a = 1;\n$b = 2;\n".to_vec(),
            Language::Php,
            ScanOptions::default(),
            1,
        );
        assert_eq!(code.safe_checkpoints(), [0]);

        /* NOTE: The line break behind a `?>` belongs to the tag, so the byte
         * after it is the start of the first inline-HTML line and a restart
         * point like any other. */
        let mut template = IncrementalDocument::new(
            b"<?php $a = 1; ?>\n<p>x</p>\n".to_vec(),
            Language::Php,
            ScanOptions::default(),
            1,
        );
        assert_eq!(template.safe_checkpoints(), [0, 17, 26]);
        template
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(26, 26),
                    replacement: b"<?php # note\n".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(template.source(), Language::Php, ScanOptions::default(), 0);
        assert_eq!(template.report(), &expected);
        assert_eq!(template.safe_checkpoints(), expected_checkpoints);
    }

    /// Regression: a checkpoint sits immediately after a line terminator, and
    /// CRLF is one terminator. Inserting the LF of a CRLF pair right after an
    /// existing CR moves the boundary one byte on, so the offset the previous
    /// revision recorded now splits the pair — a full scan never offers it, and
    /// restarting there would resume in the middle of a line ending.
    #[test]
    fn an_edit_that_completes_a_crlf_pair_invalidates_the_checkpoint_it_splits() {
        let mut document = IncrementalDocument::new(
            b"let x = 1;\r\rlet y = 2;\r".to_vec(),
            Language::Rust,
            ScanOptions::default(),
            1,
        );
        assert_eq!(document.safe_checkpoints()[..3], [0, 11, 12]);
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(11, 11),
                    replacement: b"\n".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(document.source(), Language::Rust, ScanOptions::default(), 0);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);
    }

    #[test]
    fn incremental_matches_full_scan() {
        let mut document = IncrementalDocument::new(
            b"let x = 1; // old\n".to_vec(),
            Language::Rust,
            ScanOptions::default(),
            1,
        );
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(14, 17),
                    replacement: b"new".to_vec(),
                }],
                2,
            )
            .unwrap();
        assert_eq!(
            document.report(),
            &crate::scan(document.source(), Language::Rust, ScanOptions::default())
        );
        assert_eq!(
            document.transform(Layout::Lines),
            crate::transform(
                document.source(),
                Language::Rust,
                TransformOptions::default()
            )
        );
    }

    #[test]
    fn rescans_from_a_lexically_safe_line_checkpoint() {
        let source =
            b"let text = r#\"first\nsecond\"#;\nlet value = 1; // old\nlet tail = 2; // tail\n"
                .to_vec();
        let mut document =
            IncrementalDocument::new(source, Language::Rust, ScanOptions::default(), 1);
        let comment = document.report().comments[0].span;
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(comment.start + 3, comment.end),
                    replacement: b"newer".to_vec(),
                }],
                2,
            )
            .unwrap();
        assert!(document.last_rescan_span().start > 0);
        assert!(document.last_rescan_span().start > b"let text = r#\"first\n".len());
        assert!(document.last_rescan_span().end < document.source().len());
        assert_eq!(
            document.report(),
            &crate::scan(document.source(), Language::Rust, ScanOptions::default())
        );
    }

    #[test]
    fn suffix_scan_does_not_reclassify_a_late_python_encoding_comment() {
        let source = b"value = 1\nother = 2\n# coding: latin-1\n".to_vec();
        let mut document =
            IncrementalDocument::new(source, Language::Python, ScanOptions::default(), 1);
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(18, 19),
                    replacement: b"3".to_vec(),
                }],
                2,
            )
            .unwrap();
        assert!(document.last_rescan_span().start > 0);
        assert_eq!(document.report().comments[0].kind, crate::CommentKind::Line);
        assert_eq!(
            document.report(),
            &crate::scan(document.source(), Language::Python, ScanOptions::default())
        );
    }

    #[test]
    fn lexical_divergence_falls_back_to_the_document_end() {
        let source = b"let first = 1;\nlet second = 2;\nlet tail = 3;\n".to_vec();
        let mut document =
            IncrementalDocument::new(source, Language::Rust, ScanOptions::default(), 1);
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(28, 29),
                    replacement: b"r#\"open".to_vec(),
                }],
                2,
            )
            .unwrap();
        assert_eq!(document.last_rescan_span().end, document.source().len());
        assert_eq!(
            document.report(),
            &crate::scan(document.source(), Language::Rust, ScanOptions::default())
        );
    }

    /// Regression: the rescan window used to be scanned as a *truncated* byte
    /// slice, so lexical decisions that peek past the window end (here Rust's
    /// six-byte character-literal lookahead) saw a different document than a
    /// full scan and the unterminated literal was silently lost.
    #[test]
    fn a_truncated_rescan_window_still_reports_an_unterminated_char_literal() {
        let source = vec![
            0, 0, 35, 0, 39, 128, 34, 39, 10, 39, 0, 35, 35, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let mut document =
            IncrementalDocument::new(source, Language::Rust, ScanOptions::default(), 1);
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(5, 8),
                    replacement: vec![128],
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(document.source(), Language::Rust, ScanOptions::default(), 0);
        assert_eq!(document.report().comments, expected.comments);
        assert_eq!(document.report().diagnostics, expected.diagnostics);
        assert_eq!(document.report().valid, expected.valid);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);
    }

    /// Regression: `rust_char_start` decided whether an apostrophe opened a
    /// character literal by reading up to six bytes forward, and the window was
    /// allowed to run past a line terminator — while `scan_c_family` still
    /// offers a checkpoint at the line start behind it. The decision for a
    /// token on line 1 therefore depended on bytes on line 2, which is exactly
    /// what a checkpoint promises cannot happen: an edit on line 2 left the
    /// reused prefix describing a literal a full scan no longer sees.
    #[test]
    fn a_rust_character_literal_never_decides_across_a_line_terminator() {
        /* NOTE: The bare window: `'` at the end of line 1 and the apostrophe
         * that would close it two bytes on, past the terminator. */
        let mut document = IncrementalDocument::new(
            b"let a = '\nx;\n".to_vec(),
            Language::Rust,
            ScanOptions::default(),
            1,
        );
        assert_eq!(document.safe_checkpoints(), [0, 10, 13]);
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(10, 11),
                    replacement: b"'".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(document.source(), Language::Rust, ScanOptions::default(), 0);
        assert_eq!(document.report().diagnostics, expected.diagnostics);
        assert_eq!(document.report().valid, expected.valid);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);

        /* NOTE: The escaped window, which reaches one byte further: `'\` at the
         * end of line 1 and the closing apostrophe at the head of line 2. A
         * full scan used to read the terminator as the escaped character and
         * swallow it, dropping the checkpoint the line start had. */
        let mut escaped = IncrementalDocument::new(
            b"let a = '\\\nx;\n".to_vec(),
            Language::Rust,
            ScanOptions::default(),
            1,
        );
        assert_eq!(escaped.safe_checkpoints(), [0, 11, 14]);
        escaped
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(11, 12),
                    replacement: b"'".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(escaped.source(), Language::Rust, ScanOptions::default(), 0);
        assert_eq!(escaped.report(), &expected);
        assert_eq!(escaped.safe_checkpoints(), expected_checkpoints);
        assert_eq!(escaped.safe_checkpoints(), [0, 11, 14]);
    }

    /// The OCaml half of the same rule. `ocaml_char_start` reads two bytes
    /// forward for a bare character and eight for an escaped one, and both
    /// windows used to run past a line terminator that `scan_ocaml` offers a
    /// checkpoint behind.
    #[test]
    fn an_ocaml_character_literal_never_decides_across_a_line_terminator() {
        let mut document = IncrementalDocument::new(
            b"let c = '\nz\n".to_vec(),
            Language::Ocaml,
            ScanOptions::default(),
            1,
        );
        assert_eq!(document.safe_checkpoints(), [0, 10, 12]);
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(10, 11),
                    replacement: b"'".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) = scan_with_checkpoints(
            document.source(),
            Language::Ocaml,
            ScanOptions::default(),
            0,
        );
        assert_eq!(document.report().diagnostics, expected.diagnostics);
        assert_eq!(document.report().valid, expected.valid);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);

        let mut escaped = IncrementalDocument::new(
            b"let c = '\\\nz;\n".to_vec(),
            Language::Ocaml,
            ScanOptions::default(),
            1,
        );
        assert_eq!(escaped.safe_checkpoints(), [0, 11, 14]);
        escaped
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(12, 13),
                    replacement: b"'".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(escaped.source(), Language::Ocaml, ScanOptions::default(), 0);
        assert_eq!(escaped.report(), &expected);
        assert_eq!(escaped.safe_checkpoints(), expected_checkpoints);
        assert_eq!(escaped.safe_checkpoints(), [0, 11, 14]);
    }

    /// A here-document delimiter is a word, and a quoted word may span lines:
    /// `<<"EO`, a line break, `F"` names the delimiter `EO\nF`. The parse that
    /// reads it is therefore a lookahead with no line bound, and the path that
    /// gives up on an unterminated quote rewinds the scan to the byte after the
    /// operator and lexes the same bytes again from a state it already decided
    /// out of them. That the re-lex reaches the same end today is two lexers
    /// agreeing, not a promise, so the watermark withdraws every checkpoint
    /// the parse read through and the two edits below — one that opens the
    /// quote, one that closes it again — stay equal to a full scan.
    #[test]
    fn a_quoted_shell_heredoc_delimiter_withdraws_the_checkpoints_it_read_past() {
        let closed = b"cat <<\"EO\nF\"\nx\nEO\nF\n# c\n".to_vec();
        let mut document =
            IncrementalDocument::new(closed.clone(), Language::Shell, ScanOptions::default(), 1);
        assert_eq!(document.safe_checkpoints(), [0]);
        /* NOTE: deleting the closing quote leaves the delimiter word open, so
         * the parse reads to the end of the document and gives up there. */
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(11, 12),
                    replacement: Vec::new(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) = scan_with_checkpoints(
            document.source(),
            Language::Shell,
            ScanOptions::default(),
            0,
        );
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);

        let mut reopened = IncrementalDocument::new(
            document.source().to_vec(),
            Language::Shell,
            ScanOptions::default(),
            1,
        );
        reopened
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(11, 11),
                    replacement: b"\"".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) = scan_with_checkpoints(
            reopened.source(),
            Language::Shell,
            ScanOptions::default(),
            0,
        );
        assert_eq!(reopened.source(), &closed[..]);
        assert_eq!(reopened.report(), &expected);
        assert_eq!(reopened.safe_checkpoints(), expected_checkpoints);
        /* NOTE: The document above is invalid — a delimiter word holding a line
         * terminator matches no line of the body, so the here-document runs off
         * the end — and an invalid report is never reused, which leaves the
         * watermark unexercised. This one is valid, and its checkpoints are
         * withdrawn by nothing but the reach. `#` is an ordinary word character
         * to the delimiter parse, so `<<#"` reads a word that opens a quote;
         * the quote finds no partner and the parse gives up at the end of the
         * document, having read every byte of it. The scan rewinds to the byte
         * after the operator, where `#` is a comment opener instead, and lexes
         * a comment, a line, and a comment — line starts a checkpoint would
         * otherwise be offered at. */
        let mut giving_up = IncrementalDocument::new(
            b"cat <<#\"\nx\n# c\n".to_vec(),
            Language::Shell,
            ScanOptions::default(),
            1,
        );
        assert!(giving_up.report().valid);
        assert_eq!(giving_up.report().comments.len(), 2);
        assert_eq!(giving_up.safe_checkpoints(), [0]);
        /* NOTE: Closing the quote on line 3 gives the delimiter word the whole
         * file, and the here-document it opens is then the unterminated one: the
         * full scan finds no comment at all. A rescan restarted from the line
         * start at 11 — which is what stands there without the reach — would
         * keep both of the old comments and call the file valid. */
        giving_up
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(14, 14),
                    replacement: b"\"".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) = scan_with_checkpoints(
            giving_up.source(),
            Language::Shell,
            ScanOptions::default(),
            0,
        );
        assert!(!expected.valid);
        assert!(expected.comments.is_empty());
        assert_eq!(giving_up.report(), &expected);
        assert_eq!(giving_up.safe_checkpoints(), expected_checkpoints);
    }

    /// OCaml's quoted strings are where that property first caught a
    /// checkpoint standing inside what a decision had read — and the answer is
    /// not to withdraw the rest of the document but to stop reading it. A
    /// quoted-string tag is `[a-z_]*` with the `|` directly behind it, so the
    /// search is bounded by that class: an ordinary `{` gives up at the first
    /// byte outside it, and every line under it keeps the restart point it
    /// earned. What the bound has to be worth is soundness, so both edits are
    /// checked against a full scan — the one on a later line, which restarts
    /// from a kept checkpoint, and the one that turns the `{` into a real
    /// quoted string, which lies before every checkpoint but 0.
    #[test]
    fn an_ocaml_quoted_string_tag_search_is_bounded_by_its_tag_class() {
        let stray = b"let x = {aa\n(* c *)\ny\n".to_vec();
        let mut document =
            IncrementalDocument::new(stray.clone(), Language::Ocaml, ScanOptions::default(), 1);
        assert!(document.report().valid);
        assert_eq!(document.report().comments.len(), 1);
        assert_eq!(document.safe_checkpoints(), [0, 12, 20, 22]);

        /* NOTE: An edit on the last line restarts from one of those kept
         * checkpoints rather than from 0, which is the whole point of keeping
         * them, and it still answers what a full scan answers. */
        document
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(21, 21),
                    replacement: b" (* d *)".to_vec(),
                }],
                2,
            )
            .unwrap();
        assert!(document.last_rescan_span().start >= 12);
        let (expected, expected_checkpoints) = scan_with_checkpoints(
            document.source(),
            Language::Ocaml,
            ScanOptions::default(),
            0,
        );
        assert_eq!(document.report().comments.len(), 2);
        assert_eq!(document.report(), &expected);
        assert_eq!(document.safe_checkpoints(), expected_checkpoints);

        /* NOTE: The `|` is what makes the tag a tag, and it stands before every
         * checkpoint the file has but 0: the `{aa|` opens a quoted string that
         * no `|aa}` closes, so the file is one unterminated literal and the
         * comment on line 2 is inside it. */
        let mut opened =
            IncrementalDocument::new(stray, Language::Ocaml, ScanOptions::default(), 1);
        opened
            .apply_changes(
                &[DocumentChange {
                    span: ByteSpan::new(11, 11),
                    replacement: b"|".to_vec(),
                }],
                2,
            )
            .unwrap();
        let (expected, expected_checkpoints) =
            scan_with_checkpoints(opened.source(), Language::Ocaml, ScanOptions::default(), 0);
        assert!(!expected.valid);
        assert!(expected.comments.is_empty());
        assert_eq!(opened.report(), &expected);
        assert_eq!(opened.safe_checkpoints(), expected_checkpoints);
    }

    #[test]
    fn invalid_change_batches_leave_the_document_untouched() {
        let source = b"abcdef".to_vec();
        let mut document =
            IncrementalDocument::new(source.clone(), Language::Rust, ScanOptions::default(), 1);
        assert_eq!(
            document.apply_changes(
                &[
                    DocumentChange {
                        span: ByteSpan::new(1, 3),
                        replacement: b"x".to_vec(),
                    },
                    DocumentChange {
                        span: ByteSpan::new(2, 4),
                        replacement: b"y".to_vec(),
                    },
                ],
                2,
            ),
            Err(IncrementalError::InvalidSpan)
        );
        assert_eq!(document.source(), source);
        assert_eq!(document.version(), 1);
    }

    #[test]
    fn utf16_positions_handle_astral_characters() {
        let document = IncrementalDocument::new(
            "😀x".as_bytes().to_vec(),
            Language::Rust,
            ScanOptions::default(),
            1,
        );
        assert_eq!(
            document.byte_offset(0, 2, PositionEncoding::Utf16).unwrap(),
            4
        );
        assert!(document.byte_offset(0, 1, PositionEncoding::Utf16).is_err());
    }

    #[test]
    fn positions_exclude_crlf_and_lone_cr_line_endings() {
        let document = IncrementalDocument::new(
            b"ab\r\ncd\ref".to_vec(),
            Language::Rust,
            ScanOptions::default(),
            1,
        );
        assert_eq!(document.byte_offset(0, 2, PositionEncoding::Utf8), Ok(2));
        assert!(document.byte_offset(0, 3, PositionEncoding::Utf8).is_err());
        assert_eq!(document.byte_offset(1, 2, PositionEncoding::Utf16), Ok(6));
        assert_eq!(document.byte_offset(2, 2, PositionEncoding::Utf32), Ok(9));
    }
}
