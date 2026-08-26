//! Rescan a document as it is edited, instead of scanning it again.
//!
//! `IncrementalDocument` keeps the previous revision's report and rescans only
//! the stretch an edit disturbed. `last_rescan_span` is what that saved.
//!
//! ```sh
//! cargo run -p ocomment-core --example incremental
//! ```

use ocomment_core::{
    ByteSpan, DocumentChange, IncrementalDocument, IncrementalError, Language, ScanOptions,
};

fn main() {
    let source =
        b"fn main() {\n    let total = 1 + 2; // adds them\n    println!(\"{total}\");\n}\n";
    let mut document =
        IncrementalDocument::new(source.to_vec(), Language::Rust, ScanOptions::default(), 1);
    report(&document);

    /* NOTE: Spans address the document as it stands before the batch, so a
     * client never has to compensate for its own earlier changes. */
    let end = document.source().len() - 2;
    document
        .apply_changes(
            &[DocumentChange {
                span: ByteSpan::new(end, end),
                replacement: b"\n    // and one more\n".to_vec(),
            }],
            2,
        )
        .expect("the span is inside the document and the version advances");
    report(&document);

    let stale = document.apply_changes(&[], 2);
    assert_eq!(
        stale,
        Err(IncrementalError::StaleVersion {
            received: 2,
            current: 2,
        })
    );
    println!(
        "a stale batch changes nothing: still version {}",
        document.version()
    );
}

/// Print the version, the comments, and how much the last edit cost.
fn report(document: &IncrementalDocument) {
    let rescanned = document.last_rescan_span();
    println!(
        "version {}: comments {}, rescanned {} of {} bytes",
        document.version(),
        document.report().comments.len(),
        rescanned.len(),
        document.source().len(),
    );
}
