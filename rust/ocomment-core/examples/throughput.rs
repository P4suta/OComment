use ocomment_core::{Dialect, Language, ScanOptions, scan};
use std::{env, hint::black_box, process::ExitCode, time::Instant};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("throughput: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let language_name = env::args().nth(1).unwrap_or_else(|| "c".into());
    let language: Language = language_name.parse()?;
    let mebibytes: usize = env::args()
        .nth(2)
        .as_deref()
        .unwrap_or("32")
        .parse()
        .map_err(|_| "size must be a positive integer MiB count".to_owned())?;
    let iterations: usize = env::args()
        .nth(3)
        .as_deref()
        .unwrap_or("7")
        .parse()
        .map_err(|_| "iterations must be a positive integer".to_owned())?;
    if mebibytes == 0 || iterations == 0 {
        return Err("size and iterations must be greater than zero".into());
    }
    let target = mebibytes
        .checked_mul(1024 * 1024)
        .ok_or_else(|| "requested input is too large".to_owned())?;
    let (filler, comment, dialect): (&[u8], &[u8], Dialect) = match language {
        Language::JavaScript | Language::TypeScript => (
            b"const text = `opaque // text ${value + 1}`; const re = /[/*]+/g; value += 1;\n",
            b"/* removable */\n",
            Dialect::Standard,
        ),
        Language::Shell => (
            b"value='opaque # text'; printf '%s\\n' \"$value\"; value=${value#prefix}\n",
            b"# removable\n",
            Dialect::Bash53,
        ),
        Language::Rust => (
            b"let text = r#\"opaque /* text */\"#; value = value.wrapping_add(1);\n",
            b"// removable\n",
            Dialect::Standard,
        ),
        _ => (
            b"const char *text = \"opaque /* text */\"; value = value + 1;\n",
            b"/* removable */\n",
            Dialect::Standard,
        ),
    };
    /* PERF: Keep comment allocation realistic: one span per 4 KiB rather than one
     * span per source line. The filler still exercises each language's string
     * and other lexically sensitive states. */
    let mut fragment = Vec::with_capacity(4096 + filler.len());
    while fragment.len() + filler.len() + comment.len() <= 4096 {
        fragment.extend_from_slice(filler);
    }
    fragment.extend_from_slice(comment);
    let mut source = Vec::with_capacity(target + fragment.len());
    while source.len() < target {
        source.extend_from_slice(&fragment);
    }

    let options = ScanOptions {
        dialect,
        ..ScanOptions::default()
    };
    let warm = scan(black_box(&source), language, options.clone());
    if !warm.valid {
        return Err("generated benchmark input is lexically invalid".into());
    }
    black_box(warm.comments.len());

    let mut samples = Vec::with_capacity(iterations);
    let mut comment_count = 0usize;
    for _ in 0..iterations {
        let started = Instant::now();
        let report = scan(black_box(&source), language, options.clone());
        let elapsed = started.elapsed().as_secs_f64();
        if !report.valid {
            return Err("benchmark scan became invalid".into());
        }
        comment_count = black_box(report.comments.len());
        samples.push(elapsed);
    }
    samples.sort_by(f64::total_cmp);
    let seconds = samples[samples.len() / 2];
    let mib_per_second = source.len() as f64 / (1024.0 * 1024.0) / seconds;
    println!(
        "{}",
        serde_json::json!({
            "language": language.as_str(),
            "bytes": source.len(),
            "iterations": iterations,
            "comments": comment_count,
            "median_seconds": seconds,
            "mib_per_second": mib_per_second,
        })
    );
    Ok(())
}
