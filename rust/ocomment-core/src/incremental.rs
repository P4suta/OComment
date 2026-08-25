use crate::{
    ByteSpan, Language, Layout, ScanOptions, ScanReport, Severity, TransformOptions,
    TransformResult, scanner::scan_with_checkpoints, transform::transform_report,
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
            self.safe_checkpoints
                .iter()
                .copied()
                .take_while(|point| *point <= earliest)
                .last()
                .unwrap_or(0)
        };
        let old_convergence = if can_reuse {
            self.safe_checkpoints
                .iter()
                .copied()
                .find(|point| *point >= old_tail_start.max(safe_start))
        } else {
            None
        };
        let mut reused_tail = None;
        let mut partial = None;
        if let Some(old_convergence) = old_convergence {
            let new_convergence = new_tail_start + old_convergence - old_tail_start;
            let (report, checkpoints) = scan_with_checkpoints(
                &next[safe_start..new_convergence],
                self.language,
                self.options.clone(),
                safe_start,
            );
            if checkpoints.last().copied() == Some(new_convergence) {
                reused_tail = Some((old_convergence, new_convergence));
                partial = Some((report, checkpoints, new_convergence));
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
            .take_while(|comment| comment.span.end <= safe_start)
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
            .take_while(|diagnostic| diagnostic.span.end <= safe_start)
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
