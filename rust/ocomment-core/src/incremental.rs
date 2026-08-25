use crate::{
    ByteSpan, Language, Layout, ScanOptions, ScanReport, Severity, TransformOptions,
    TransformResult,
    scanner::{RestartRules, preamble_is_settled, scan_until_checkpoint, scan_with_checkpoints},
    transform::transform_report,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PositionEncoding {
    Utf8,
    #[default]
    Utf16,
    Utf32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentChange {
    pub span: ByteSpan,
    pub replacement: Vec<u8>,
}

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

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum IncrementalError {
    #[error("stale document version {received}; current version is {current}")]
    StaleVersion { received: i64, current: i64 },
    #[error("change span lies outside the document")]
    InvalidSpan,
    #[error("position does not lie on a valid encoding boundary")]
    InvalidPosition,
}

impl IncrementalDocument {
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

    pub fn source(&self) -> &[u8] {
        &self.source
    }
    pub fn report(&self) -> &ScanReport {
        &self.report
    }
    pub const fn language(&self) -> Language {
        self.language
    }
    pub fn scan_options(&self) -> &ScanOptions {
        &self.options
    }
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
    pub const fn version(&self) -> i64 {
        self.version
    }
    pub const fn last_rescan_span(&self) -> ByteSpan {
        self.last_rescan
    }
    pub fn checkpoints(&self) -> &[usize] {
        &self.checkpoints
    }
    pub fn safe_checkpoints(&self) -> &[usize] {
        &self.safe_checkpoints
    }

    /// Apply a sorted, non-overlapping batch whose spans refer to the current
    /// document snapshot. Validation is transactional: an invalid batch does
    /// not alter the source, report, checkpoints, or version.
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
            safe_checkpoints.extend(
                self.safe_checkpoints
                    .iter()
                    .copied()
                    .filter(|point| *point > old_convergence)
                    .map(|point| new_convergence + point - old_convergence),
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
    use crate::scanner::scan_with_checkpoints;
    use proptest::prelude::*;

    /// Bytes that exercise every built-in scanner's string, comment, heredoc
    /// and template states rather than only the C-family delimiters.
    fn lexical_byte() -> impl Strategy<Value = u8> {
        prop_oneof![
            4 => any::<u8>(),
            2 => Just(b'\n'),
            1 => Just(b'\r'),
            1 => Just(b'/'),
            1 => Just(b'*'),
            1 => Just(b'\''),
            1 => Just(b'"'),
            1 => Just(b'#'),
            1 => Just(b'`'),
            1 => Just(b'{'),
            1 => Just(b'}'),
            1 => Just(b'<'),
            1 => Just(b'>'),
            1 => Just(b'='),
            1 => Just(b'['),
            1 => Just(b']'),
            1 => Just(b'-'),
            1 => Just(b'|'),
            1 => Just(b'?'),
            1 => Just(b'\\'),
            1 => Just(b'$'),
            1 => Just(b'%'),
            1 => Just(b'('),
            1 => Just(b')'),
            1 => Just(b'@'),
            1 => Just(b':'),
            1 => Just(b'!'),
            1 => Just(b'~'),
        ]
    }

    /// Multi-byte tokens a single-byte alphabet can never synthesise. The
    /// preamble and directive rules only fire on whole words, so without these
    /// the generated sources never reach the code paths that make a checkpoint
    /// depend on the bytes in front of it.
    fn lexical_fragment() -> impl Strategy<Value = Vec<u8>> {
        prop_oneof![
            8 => lexical_byte().prop_map(|byte| vec![byte]),
            1 => Just(b"coding:".to_vec()),
            1 => Just(b"# -*- coding: utf-8 -*-".to_vec()),
            1 => Just(b"# coding: latin-1".to_vec()),
            1 => Just(b"#!".to_vec()),
            1 => Just(b"//go:build".to_vec()),
            1 => Just(b"/*#__PURE__*/".to_vec()),
            1 => Just(b"<!--".to_vec()),
            1 => Just(b"r#\"".to_vec()),
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
        #[test]
        fn safe_checkpoints_restart_every_builtin_scan_exactly(
            source in lexical_source(0..48),
        ) {
            for language in Language::ALL {
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
