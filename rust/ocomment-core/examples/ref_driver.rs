//! Test tooling: the driver `tools/differential.py` speaks to.
//!
//! It reads one JSON request per line on standard input and answers on
//! standard output, so the OCaml reference implementation and this one can be
//! compared byte for byte. It is not an example of how to use the library;
//! `strip.rs` is.

use ocomment_core::{
    ByteSpan, CommentKind, DeclarativeProfile, Dialect, Edit, Language, Layout, Policy,
    ScanOptions, TransformOptions, apply_edits, scan, scan_profile, transform, transform_profile,
    transform_spans,
};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    str::FromStr,
};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) if !line.trim().is_empty() => line,
            _ => continue,
        };
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                writeln!(
                    stdout,
                    "{}",
                    json!({"id": Value::Null, "error": error.to_string()})
                )
                .ok();
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let response = handle(&request).map_or_else(
            |error| json!({"id": id, "error": error}),
            |ok| json!({"id": id, "ok": ok}),
        );
        serde_json::to_writer(&mut stdout, &response).unwrap();
        writeln!(stdout).unwrap();
        stdout.flush().unwrap();
    }
}

fn handle(request: &Value) -> Result<Value, String> {
    let operation = request
        .get("operation")
        .and_then(Value::as_str)
        .ok_or("missing operation")?;
    let language = Language::from_str(
        request
            .get("language")
            .and_then(Value::as_str)
            .ok_or("missing language")?,
    )?;
    let source = decode(
        request
            .get("source_base64")
            .and_then(Value::as_str)
            .ok_or("missing source_base64")?,
    )?;
    let options_value = request.get("options").unwrap_or(&Value::Null);
    let policy = match options_value
        .get("policy")
        .and_then(Value::as_str)
        .unwrap_or("safe")
    {
        "all" => Policy::All,
        "legal" => Policy::Legal,
        _ => Policy::Safe,
    };
    let layout = match options_value
        .get("layout")
        .and_then(Value::as_str)
        .unwrap_or("lines")
    {
        "columns" => Layout::Columns,
        "compact" => Layout::Compact,
        _ => Layout::Lines,
    };
    let dialect = option_enum::<Dialect>(options_value, "dialect")?.unwrap_or_default();
    let keep_kinds = option_list::<CommentKind>(options_value, "keep_kinds")?;
    let remove_kinds = option_list::<CommentKind>(options_value, "remove_kinds")?;
    let keep_regex = option_strings(options_value, "keep_regex")?;
    let remove_regex = option_strings(options_value, "remove_regex")?;
    let scan_options = ScanOptions {
        policy,
        dialect,
        force_invalid: options_value
            .get("force_invalid")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        force_protected: options_value
            .get("force_protected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        keep_kinds,
        remove_kinds,
        keep_regex,
        remove_regex,
    };
    match operation {
        "apply_edits" => {
            let edits = request
                .get("edits")
                .and_then(Value::as_array)
                .ok_or("missing edits")?
                .iter()
                .map(parse_edit)
                .collect::<Result<Vec<_>, _>>()?;
            validate_edits(source.len(), &edits)?;
            Ok(json!({"output_base64": encode(&apply_edits(&source, &edits))}))
        }
        "scan" => serde_json::to_value(scan(&source, language, scan_options))
            .map_err(|error| error.to_string()),
        "transform" => {
            let result = transform(
                &source,
                language,
                TransformOptions {
                    scan: scan_options,
                    layout,
                },
            );
            Ok(transform_json(&result))
        }
        "transform-spans" => {
            let spans = request
                .get("spans")
                .and_then(Value::as_array)
                .ok_or("missing spans")?
                .iter()
                .map(|item| {
                    let start = item
                        .get("start")
                        .and_then(Value::as_u64)
                        .ok_or("span start")?;
                    let end = item.get("end").and_then(Value::as_u64).ok_or("span end")?;
                    let kind: CommentKind =
                        serde_json::from_value(item.get("kind").cloned().ok_or("span kind")?)
                            .map_err(|error| error.to_string())?;
                    Ok((
                        ByteSpan::new(
                            usize::try_from(start).map_err(|error| error.to_string())?,
                            usize::try_from(end).map_err(|error| error.to_string())?,
                        ),
                        kind,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let result = transform_spans(
                &source,
                language,
                &spans,
                TransformOptions {
                    scan: scan_options,
                    layout,
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(transform_json(&result))
        }
        "scan-profile" => {
            let profile: DeclarativeProfile =
                serde_json::from_value(request.get("profile").cloned().ok_or("missing profile")?)
                    .map_err(|error| error.to_string())?;
            serde_json::to_value(
                scan_profile(&source, &profile, scan_options).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())
        }
        "transform-profile" => {
            let profile: DeclarativeProfile =
                serde_json::from_value(request.get("profile").cloned().ok_or("missing profile")?)
                    .map_err(|error| error.to_string())?;
            let result = transform_profile(
                &source,
                &profile,
                TransformOptions {
                    scan: scan_options,
                    layout,
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(transform_json(&result))
        }
        other => Err(format!("unsupported operation `{other}`")),
    }
}

fn parse_edit(value: &Value) -> Result<Edit, String> {
    let span = value.get("span").ok_or("edit span")?;
    let start = span
        .get("start")
        .and_then(Value::as_u64)
        .ok_or("edit start")?;
    let end = span.get("end").and_then(Value::as_u64).ok_or("edit end")?;
    let replacement = decode(
        value
            .get("replacement_base64")
            .and_then(Value::as_str)
            .ok_or("edit replacement_base64")?,
    )?;
    Ok(Edit {
        span: ByteSpan::new(
            usize::try_from(start).map_err(|error| error.to_string())?,
            usize::try_from(end).map_err(|error| error.to_string())?,
        ),
        replacement,
    })
}

fn validate_edits(source_len: usize, edits: &[Edit]) -> Result<(), String> {
    let mut cursor = 0;
    for (index, edit) in edits.iter().enumerate() {
        if edit.span.start > edit.span.end || edit.span.start < cursor || edit.span.end > source_len
        {
            return Err(format!("invalid edit contract at edit {index}"));
        }
        cursor = edit.span.end;
    }
    Ok(())
}

fn transform_json(result: &ocomment_core::TransformResult) -> Value {
    json!({
        "output_base64": encode(&result.output),
        "edits": result.edits.iter().map(|edit| json!({"span": edit.span, "replacement_base64": encode(&edit.replacement)})).collect::<Vec<_>>(),
        "report": result.report,
        "source_map": result.source_map.segments,
    })
}

fn option_enum<T>(value: &Value, name: &str) -> Result<Option<T>, String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    value
        .get(name)
        .map(|item| serde_json::from_value(item.clone()).map_err(|error| error.to_string()))
        .transpose()
}

fn option_list<T>(value: &Value, name: &str) -> Result<Vec<T>, String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    value.get(name).map_or_else(
        || Ok(Vec::new()),
        |item| serde_json::from_value(item.clone()).map_err(|error| error.to_string()),
    )
}

fn option_strings(value: &Value, name: &str) -> Result<Vec<String>, String> {
    option_list(value, name)
}

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn decode(text: &str) -> Result<Vec<u8>, String> {
    let cleaned: Vec<_> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if cleaned.len() % 4 != 0 {
        return Err("invalid base64 length".into());
    }
    let mut output = Vec::with_capacity(cleaned.len() / 4 * 3);
    for chunk in cleaned.chunks(4) {
        let a = decode_byte(chunk[0])?;
        let b = decode_byte(chunk[1])?;
        output.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            let c = decode_byte(chunk[2])?;
            output.push((b << 4) | (c >> 2));
            if chunk[3] != b'=' {
                let d = decode_byte(chunk[3])?;
                output.push((c << 6) | d);
            }
        }
    }
    Ok(output)
}

fn decode_byte(byte: u8) -> Result<u8, String> {
    TABLE
        .iter()
        .position(|candidate| *candidate == byte)
        .map(|index| index as u8)
        .ok_or_else(|| "invalid base64 byte".into())
}
