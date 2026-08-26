//! Put comment spans found elsewhere through the OComment policy.
//!
//! `transform_spans` is the hand-off point for a scanner this crate does not
//! have — a WebAssembly plugin, or the two-line one below. The policy, the
//! layout, the edit validation, and the source map are the built-in ones.
//!
//! ```sh
//! cargo run -p ocomment-core --example external_spans
//! ```

use ocomment_core::{ByteSpan, CommentKind, Language, TransformOptions, transform_spans};

const SOURCE: &[u8] =
    b"(display x) ;; prints it\n(exit 0) ;; ocomment-keep: the exit code matters\n";

/// Find every `;;` comment, which is all this pretend scanner knows how to do.
fn find_comments(source: &[u8]) -> Vec<(ByteSpan, CommentKind)> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < source.len() {
        if source[index..].starts_with(b";;") {
            let end = index
                + source[index..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .unwrap_or(source.len() - index);
            let text = &source[index..end];
            // NOTE: The kind is the scanner's job; whether it survives is the policy's.
            let kind = if text.windows(14).any(|window| window == b"ocomment-keep:") {
                CommentKind::Directive
            } else {
                CommentKind::Line
            };
            spans.push((ByteSpan::new(index, end), kind));
            index = end;
        } else {
            index += 1;
        }
    }
    spans
}

fn main() {
    let spans = find_comments(SOURCE);
    println!("found {} comments", spans.len());

    let result = transform_spans(
        SOURCE,
        Language::Unknown,
        &spans,
        TransformOptions::default(),
    )
    .expect("the spans are non-empty, sorted, and inside the source");
    for comment in &result.report.comments {
        println!("  {:<9} {}", comment.kind, comment.disposition);
    }
    println!("---");
    print!("{}", String::from_utf8_lossy(&result.output));
}
