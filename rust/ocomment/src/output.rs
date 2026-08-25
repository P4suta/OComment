use crate::files::SkippedFile;
use anyhow::Result;
use clap::ValueEnum;
use ocomment_core::{ByteSpan, Language, TransformResult};
use serde::Serialize;
use serde_json::{Value, json};
use similar::{ChangeTag, TextDiff};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
    Jsonl,
    Sarif,
    Github,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Check,
    Scan,
    Diff,
    Fix,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Presentation {
    pub color: bool,
    pub hyperlinks: bool,
    pub progress: bool,
}

#[derive(Clone, Debug)]
pub struct ProcessedFile {
    pub path: PathBuf,
    pub source: Vec<u8>,
    pub language: Language,
    pub result: TransformResult,
}

#[derive(Serialize)]
struct JsonFile<'a> {
    path: String,
    language: Language,
    changed: bool,
    report: &'a ocomment_core::ScanReport,
    edits: &'a [ocomment_core::Edit],
    source_map: &'a ocomment_core::SourceMap,
}

pub fn render(
    files: &[ProcessedFile],
    skipped: &[SkippedFile],
    format: OutputFormat,
    operation: Operation,
    presentation: Presentation,
) -> Result<()> {
    match format {
        OutputFormat::Human => render_human(files, skipped, operation, presentation),
        OutputFormat::Json => render_json(files, skipped),
        OutputFormat::Jsonl => render_jsonl(files, skipped),
        OutputFormat::Sarif => render_sarif(files, skipped),
        OutputFormat::Github => render_github(files, skipped),
    }
}

fn render_human(
    files: &[ProcessedFile],
    skipped: &[SkippedFile],
    operation: Operation,
    presentation: Presentation,
) -> Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for file in files {
        if operation == Operation::Diff && file.source != file.result.output {
            write!(
                output,
                "{}",
                unified_diff(&file.path, &file.source, &file.result.output)
            )?;
            continue;
        }
        for diagnostic in &file.result.report.diagnostics {
            let (line, column) = line_column(&file.source, diagnostic.span.start);
            writeln!(
                output,
                "{}:{line}:{column}: {}{:?}[{}]{}: {}",
                display_path(&file.path, presentation.hyperlinks),
                color("\x1b[31m", presentation.color),
                diagnostic.severity,
                diagnostic.code,
                color("\x1b[0m", presentation.color),
                diagnostic.message
            )?;
        }
        if operation == Operation::Scan {
            for comment in &file.result.report.comments {
                let (line, column) = line_column(&file.source, comment.span.start);
                writeln!(
                    output,
                    "{}:{line}:{column}: {:?} {:?} {}..{}",
                    display_path(&file.path, presentation.hyperlinks),
                    comment.kind,
                    comment.disposition,
                    comment.span.start,
                    comment.span.end
                )?;
            }
        } else if operation != Operation::Fix {
            for comment in file
                .result
                .report
                .comments
                .iter()
                .filter(|comment| comment.disposition.is_remove())
            {
                let (line, column) = line_column(&file.source, comment.span.start);
                writeln!(
                    output,
                    "{}:{line}:{column}: {}removable {:?} comment{}",
                    display_path(&file.path, presentation.hyperlinks),
                    color("\x1b[33m", presentation.color),
                    comment.kind,
                    color("\x1b[0m", presentation.color)
                )?;
            }
        }
    }
    if operation != Operation::Diff {
        for item in skipped {
            writeln!(
                output,
                "{}: {}: {}",
                display_path(&item.path, presentation.hyperlinks),
                if item.error { "error" } else { "skipped" },
                item.reason
            )?;
        }
    }
    if presentation.progress {
        eprintln!(
            "ocomment: processed {} file(s), skipped {}",
            files.len(),
            skipped.len()
        );
    }
    Ok(())
}

fn color(code: &'static str, enabled: bool) -> &'static str {
    if enabled { code } else { "" }
}

fn display_path(path: &Path, hyperlinks: bool) -> String {
    let display = path.display().to_string();
    if !hyperlinks {
        return display;
    }
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let target = absolute
        .to_string_lossy()
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23");
    format!("\x1b]8;;file://{target}\x1b\\{display}\x1b]8;;\x1b\\")
}

fn render_json(files: &[ProcessedFile], skipped: &[SkippedFile]) -> Result<()> {
    let values: Vec<_> = files.iter().map(json_file).collect();
    let skipped: Vec<_> = skipped
        .iter()
        .map(|item| {
            json!({"path": item.path.to_string_lossy(), "reason": item.reason, "error": item.error})
        })
        .collect();
    serde_json::to_writer_pretty(
        io::stdout().lock(),
        &json!({"version": 1, "files": values, "skipped": skipped}),
    )?;
    println!();
    Ok(())
}

fn render_jsonl(files: &[ProcessedFile], skipped: &[SkippedFile]) -> Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for file in files {
        serde_json::to_writer(&mut output, &json_file(file))?;
        writeln!(output)?;
    }
    for item in skipped {
        serde_json::to_writer(
            &mut output,
            &json!({"type": "skip", "path": item.path.to_string_lossy(), "reason": item.reason, "error": item.error}),
        )?;
        writeln!(output)?;
    }
    Ok(())
}

fn json_file(file: &ProcessedFile) -> JsonFile<'_> {
    JsonFile {
        path: file.path.to_string_lossy().into_owned(),
        language: file.language,
        changed: file.source != file.result.output,
        report: &file.result.report,
        edits: &file.result.edits,
        source_map: &file.result.source_map,
    }
}

fn render_sarif(files: &[ProcessedFile], skipped: &[SkippedFile]) -> Result<()> {
    let mut results = Vec::new();
    for file in files {
        for comment in file
            .result
            .report
            .comments
            .iter()
            .filter(|comment| comment.disposition.is_remove())
        {
            let (line, column) = line_column(&file.source, comment.span.start);
            let (end_line, end_column) = line_column(&file.source, comment.span.end);
            let kind = serde_json::to_value(comment.kind)?
                .as_str()
                .unwrap_or("comment")
                .to_owned();
            results.push(json!({
                "ruleId": format!("removable-{kind}"),
                "level": "note",
                "message": {"text": format!("removable {:?} comment", comment.kind)},
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": file.path.to_string_lossy()},
                    "region": {"startLine": line, "startColumn": column,
                        "endLine": end_line, "endColumn": end_column}
                }}],
                "fixes": [{
                    "description": {"text": "Remove comment with OComment"},
                    "artifactChanges": [{
                        "artifactLocation": {"uri": file.path.to_string_lossy()},
                        "replacements": [{"deletedRegion": {
                            "startLine": line, "startColumn": column,
                            "endLine": end_line, "endColumn": end_column
                        }, "insertedContent": {"text": replacement_for_span(file, comment.span)}}]
                    }]
                }]
            }));
        }
        for diagnostic in &file.result.report.diagnostics {
            let (line, column) = line_column(&file.source, diagnostic.span.start);
            let (end_line, end_column) = line_column(&file.source, diagnostic.span.end);
            let level = match diagnostic.severity {
                ocomment_core::Severity::Error => "error",
                ocomment_core::Severity::Warning => "warning",
                ocomment_core::Severity::Info | ocomment_core::Severity::Hint => "note",
            };
            results.push(json!({
                "ruleId": diagnostic.code,
                "level": level,
                "message": {"text": diagnostic.message},
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": file.path.to_string_lossy()},
                    "region": {"startLine": line, "startColumn": column,
                        "endLine": end_line, "endColumn": end_column}
                }}]
            }));
        }
    }
    for item in skipped {
        results.push(json!({
            "ruleId": if item.error { "io-error" } else { "skipped-file" },
            "level": if item.error { "error" } else { "note" },
            "message": {"text": item.reason},
            "locations": [{"physicalLocation": {
                "artifactLocation": {"uri": item.path.to_string_lossy()}
            }}]
        }));
    }
    let sarif = json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{"tool": {"driver": {"name": "ocomment", "informationUri": "https://github.com/P4suta/OComment"}}, "results": results}]
    });
    serde_json::to_writer_pretty(io::stdout().lock(), &sarif)?;
    println!();
    Ok(())
}

fn replacement_for_span(file: &ProcessedFile, span: ByteSpan) -> String {
    file.result
        .edits
        .iter()
        .find(|edit| edit.span == span)
        .map(|edit| String::from_utf8_lossy(&edit.replacement).into_owned())
        .unwrap_or_default()
}

fn render_github(files: &[ProcessedFile], skipped: &[SkippedFile]) -> Result<()> {
    for file in files {
        for comment in file
            .result
            .report
            .comments
            .iter()
            .filter(|comment| comment.disposition.is_remove())
        {
            let (line, column) = line_column(&file.source, comment.span.start);
            println!(
                "::notice file={},line={line},col={column}::removable {:?} comment",
                github_escape(&file.path.to_string_lossy()),
                comment.kind
            );
        }
        for diagnostic in &file.result.report.diagnostics {
            let (line, column) = line_column(&file.source, diagnostic.span.start);
            println!(
                "::error file={},line={line},col={column},title={}::{}",
                github_escape(&file.path.to_string_lossy()),
                github_escape(&diagnostic.code),
                github_escape(&diagnostic.message)
            );
        }
    }
    for item in skipped {
        println!(
            "::{} file={},title={}::{}",
            if item.error { "error" } else { "notice" },
            github_escape(&item.path.to_string_lossy()),
            if item.error {
                "OComment I/O error"
            } else {
                "OComment skipped file"
            },
            github_escape(&item.reason)
        );
    }
    Ok(())
}

pub fn unified_diff(path: &Path, original: &[u8], transformed: &[u8]) -> String {
    let old = String::from_utf8_lossy(original);
    let new = String::from_utf8_lossy(transformed);
    let display = path.to_string_lossy().replace('\\', "/");
    let mut output = format!("--- a/{display}\n+++ b/{display}\n");
    let diff = TextDiff::from_lines(&old, &new);
    for group in diff.grouped_ops(3) {
        let old_start = group.first().map_or(0, |op| op.old_range().start) + 1;
        let new_start = group.first().map_or(0, |op| op.new_range().start) + 1;
        let old_len: usize = group.iter().map(|op| op.old_range().len()).sum();
        let new_len: usize = group.iter().map(|op| op.new_range().len()).sum();
        output.push_str(&format!(
            "@@ -{old_start},{old_len} +{new_start},{new_len} @@\n"
        ));
        for op in group {
            for change in diff.iter_changes(&op) {
                let prefix = match change.tag() {
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                    ChangeTag::Equal => ' ',
                };
                output.push(prefix);
                output.push_str(change.value());
                if !change.value().ends_with('\n') {
                    output.push_str("\n\\ No newline at end of file\n");
                }
            }
        }
    }
    output
}

fn line_column(source: &[u8], offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let mut line = 1usize;
    let mut start = 0usize;
    let mut index = 0usize;
    while index < offset {
        if source[index] == b'\r' {
            index += if source.get(index + 1) == Some(&b'\n') && index + 1 < offset {
                2
            } else {
                1
            };
            line += 1;
            start = index;
        } else if source[index] == b'\n' {
            index += 1;
            line += 1;
            start = index;
        } else {
            index += 1;
        }
    }
    (line, offset - start + 1)
}

fn github_escape(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

pub fn changed(files: &[ProcessedFile]) -> bool {
    files.iter().any(|file| file.source != file.result.output)
}
pub fn invalid(files: &[ProcessedFile]) -> bool {
    files.iter().any(|file| !file.result.report.valid)
}

#[allow(dead_code)]
fn _span(_: ByteSpan) -> Value {
    Value::Null
}
