//! Remove the comments the policy allows removing, and show what changed.
//!
//! Run it over the built-in sample, or over a file of your own:
//!
//! ```sh
//! cargo run -p ocomment-core --example strip
//! cargo run -p ocomment-core --example strip -- src/lib.rs
//! ```

use ocomment_core::{Language, TransformOptions, detect_language, transform};
use std::{env, fs, path::PathBuf, process::ExitCode};

const SAMPLE: &[u8] = b"// SPDX-License-Identifier: MIT\n\
                        //! A doc comment, which the `safe` policy still removes.\n\
                        fn main() {\n\
                        \x20   // NOTE: kept by the keep_regex below, not by the policy.\n\
                        \x20   let total = 1 + 2; // adds one and two\n\
                        }\n";

fn main() -> ExitCode {
    let path = env::args_os().nth(1).map(PathBuf::from);
    let (source, language) = match path.as_deref() {
        Some(path) => {
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    eprintln!("strip: {}: {error}", path.display());
                    return ExitCode::FAILURE;
                }
            };
            let Some(found) = detect_language(Some(path), &bytes) else {
                eprintln!(
                    "strip: {}: no built-in language for this file",
                    path.display()
                );
                return ExitCode::FAILURE;
            };
            (bytes, found.language)
        }
        None => (SAMPLE.to_vec(), Language::Rust),
    };

    let mut options = TransformOptions::default();
    /* NOTE: A keep_regex override is tested before the policy, so it protects
     * a comment the policy would otherwise remove. */
    options.scan.keep_regex.push(r"^//\s*NOTE\b".into());

    let result = transform(&source, language, options);
    println!("{language}: {} comments", result.report.comments.len());
    for comment in &result.report.comments {
        println!(
            "  {:>4}..{:<4} {:<9} {}",
            comment.span.start, comment.span.end, comment.kind, comment.disposition
        );
    }
    println!("---");
    // NOTE: The output is bytes and need not be UTF-8; printing is the lossy step.
    print!("{}", String::from_utf8_lossy(&result.output));
    ExitCode::SUCCESS
}
