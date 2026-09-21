//! Describe a syntax this crate has no scanner for, with no code.
//!
//! A declarative profile is literal comment and string delimiters and nothing
//! else, which is exactly what one byte-oriented pass can read. Anything that
//! would make that pass ambiguous is refused up front.
//!
//! ```sh
//! cargo run -p ocomment-core --example profile
//! ```

use ocomment_core::{
    BlockDelimiter, CommentKind, DeclarativeProfile, LineDelimiter, ProtectedPattern,
    ProtectionTier, StringDelimiter, TransformOptions, transform_profile, validate_profile,
};

const SOURCE: &[u8] = b"set greeting \"; not a comment\"  ; a comment\n\
                        { a block\n\
                          over two lines }\n\
                        set port 8080  ; keep: production depends on it\n";

fn ini_like() -> DeclarativeProfile {
    DeclarativeProfile {
        name: "tcl-like".into(),
        extensions: vec!["tcl".into()],
        line_comments: vec![LineDelimiter {
            start: ";".into(),
            requires_boundary: true,
            kind: CommentKind::Line,
            ..Default::default()
        }],
        block_comments: vec![BlockDelimiter {
            start: "{".into(),
            end: "}".into(),
            nested: true,
            kind: CommentKind::Block,
        }],
        strings: vec![StringDelimiter {
            start: "\"".into(),
            end: "\"".into(),
            escape: Some("\\".into()),
            multiline: true,
        }],
        filenames: Vec::new(),
        doc_continuation: false,
        protected_patterns: vec![ProtectedPattern {
            contains: "keep:".into(),
            reason: "marked to keep".into(),
            // NOTE: The weaker tier: addressed to a tool, so `all` may take it.
            tier: ProtectionTier::Tool,
        }],
    }
}

fn main() {
    let profile = ini_like();
    validate_profile(&profile).expect("no delimiter is a prefix of another");

    let result = transform_profile(SOURCE, &profile, TransformOptions::default())
        .expect("the profile is valid");
    for comment in &result.report.comments {
        println!(
            "  {:<9} {:<40} {}",
            comment.kind,
            String::from_utf8_lossy(&SOURCE[comment.span.start..comment.span.end])
                .replace('\n', "\\n"),
            comment.disposition()
        );
    }
    println!("---");
    print!("{}", String::from_utf8_lossy(&result.output));

    /* NOTE: Two delimiters spelled the same way have no single reading, so the
     * profile is refused rather than resolved by an arbitrary rule. A token
     * that is merely the *start* of another is a different matter: that is how
     * a language spells a documentation comment, and the scan takes the
     * longest token that matches. */
    let mut ambiguous = ini_like();
    ambiguous.line_comments.push(LineDelimiter {
        start: ";".into(),
        kind: CommentKind::DocLine,
        ..Default::default()
    });
    println!("---");
    println!("{}", validate_profile(&ambiguous).unwrap_err());
}
