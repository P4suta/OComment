//! Byte-oriented scanning and transformation for OComment.
//!
//! Byte offsets are the canonical coordinate system. The engine never decodes
//! the complete input as UTF-8, so BOMs and non-UTF-8 source bytes survive a
//! transformation unchanged.

mod detect;
mod incremental;
mod profile;
mod scanner;
mod transform;
mod types;

pub use detect::{Detection, detect_language};
pub use incremental::{DocumentChange, IncrementalDocument, PositionEncoding};
pub use profile::{
    DeclarativeProfile, ProfileError, scan_profile, transform_profile, validate_profile,
};
pub use scanner::{DispositionPatterns, explain_disposition, explain_disposition_with, scan};
pub use transform::{apply_edits, transform, transform_spans};
pub use types::*;
