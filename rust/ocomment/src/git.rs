use crate::{
    atomic::{WritePlan, apply_transaction},
    config::ResolvedConfig,
    output::{
        self, Operation, OutputFormat, Presentation, ProcessedFile, RenderOptions, Verbosity,
    },
    plugin::PluginHost,
};
use anyhow::{Context, Result, anyhow, bail};
use ocomment_core::{
    ByteSpan, CommentKind, Diagnostic, Edit, Severity, SourceMap, TransformResult, apply_edits,
    detect_language, transform,
};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile::NamedTempFile;

struct IndexEntry {
    path: PathBuf,
    mode: String,
    source: Vec<u8>,
    processed: ProcessedFile,
}

pub struct StagedRequest<'a> {
    pub operation: Operation,
    pub paths: &'a [PathBuf],
    pub resolved: &'a ResolvedConfig,
    pub format: OutputFormat,
    pub index_only: bool,
    pub plugin_host: &'a PluginHost,
    pub forced_language: Option<ocomment_core::Language>,
    pub forced_dialect: Option<ocomment_core::Dialect>,
    pub presentation: Presentation,
    pub verbosity: Verbosity,
    pub preview: bool,
    /// The run only previews the patch; `fix --dry-run` writes nothing to
    /// the index and reports what a real run would remove.
    pub dry_run: bool,
}

pub fn run_staged(request: StagedRequest<'_>) -> Result<u8> {
    let StagedRequest {
        operation,
        paths,
        resolved,
        format,
        index_only,
        plugin_host,
        forced_language,
        forced_dialect,
        presentation,
        verbosity,
        preview,
        dry_run,
    } = request;
    let root = repository_root()?;
    let names = staged_paths(&root, paths)?;
    let mut entries = Vec::new();
    for path in names {
        let source = index_blob(&root, &path)?;
        if source.iter().take(8192).any(|byte| *byte == 0) {
            continue;
        }
        let detection = forced_language
            .map(|language| ocomment_core::Detection {
                language,
                dialect: forced_dialect.unwrap_or(ocomment_core::Dialect::Standard),
                reason: "command-line",
            })
            .or_else(|| detect_language(Some(&path), &source));
        let profile = detection
            .is_none()
            .then(|| crate::files::profile_for_path(&path, resolved))
            .flatten();
        let routed_plugin = (detection.is_none() && profile.is_none())
            .then(|| crate::files::plugin_for_path(&path, resolved))
            .flatten();
        if detection.is_none() && profile.is_none() && routed_plugin.is_none() {
            continue;
        }
        let language = detection
            .as_ref()
            .map_or(ocomment_core::Language::Unknown, |value| value.language);
        let dialect = detection
            .as_ref()
            .map_or(ocomment_core::Dialect::Standard, |value| value.dialect);
        let (mut language, mut options) = resolved.for_path(&path, language, dialect);
        if let Some(value) = forced_language {
            language = value;
        }
        if let Some(value) = forced_dialect {
            crate::config::validate_dialect(language, value)?;
            options.scan.dialect = value;
        }
        let full = if let Some(profile) = &profile {
            ocomment_core::transform_profile(&source, profile, options)
                .expect("profiles were validated while loading configuration")
        } else if let Some(name) = &routed_plugin {
            let language_name = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown")
                .to_ascii_lowercase();
            plugin_host.transform(name, &source, &language_name, &path, options)?
        } else {
            transform(&source, language, options)
        };
        let ranges = added_line_ranges(&root, &path)?;
        let mut selected_comments = Vec::new();
        let mut conflict = None;
        for comment in &full.report.comments {
            let start_line = line_number(&source, comment.span.start);
            let end_line = line_number(&source, comment.span.end.saturating_sub(1));
            let starts_added = ranges.iter().any(|range| range.contains(&start_line));
            let intersects = ranges
                .iter()
                .any(|range| range.start <= end_line && start_line < range.end);
            if starts_added {
                selected_comments.push(comment.clone());
            } else if intersects
                && comment.disposition.is_remove()
                && matches!(comment.kind, CommentKind::Block | CommentKind::DocBlock)
            {
                conflict = Some(comment.span);
                selected_comments.push(comment.clone());
            }
        }
        let selected_edits: Vec<Edit> = full
            .edits
            .iter()
            .filter(|edit| {
                let line = line_number(&source, edit.span.start);
                ranges.iter().any(|range| range.contains(&line))
            })
            .cloned()
            .collect();
        let mut report = full.report;
        report.comments = selected_comments;
        if let Some(span) = conflict {
            report.valid = false;
            report.diagnostics.push(Diagnostic {
                code: "staged-existing-block-comment".into(),
                message: "added lines modify the interior of an existing block comment; automatic removal would include pre-existing content".into(),
                severity: Severity::Error, span,
            });
        }
        let transformed = apply_edits(&source, &selected_edits);
        let source_map = SourceMap::from_edits(source.len(), &selected_edits);
        let processed = ProcessedFile {
            path: path.clone(),
            source: source.clone(),
            language,
            result: TransformResult {
                output: transformed,
                edits: selected_edits,
                report,
                source_map,
            },
        };
        entries.push(IndexEntry {
            mode: index_mode(&root, &path)?,
            path,
            source,
            processed,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let files: Vec<_> = entries
        .iter()
        .map(|entry| entry.processed.clone())
        .collect();
    let invalid = output::invalid(&files);
    let staged_conflict = files.iter().any(|file| {
        file.result
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "staged-existing-block-comment")
    });
    let applied = operation == Operation::Fix
        && (!invalid || (resolved.config.policy.force_invalid && !staged_conflict));
    if applied {
        fix_index(&root, &entries, index_only)?;
    }
    output::render(
        &files,
        &[],
        &RenderOptions {
            format,
            operation,
            presentation,
            verbosity,
            preview,
            explain: false,
            dry_run,
            force_invalid: resolved.config.policy.force_invalid,
            applied,
        },
    )?;
    if invalid {
        return Ok(2);
    }
    match operation {
        Operation::Check | Operation::Diff if output::changed(&files) => Ok(1),
        _ => Ok(0),
    }
}

fn fix_index(root: &Path, entries: &[IndexEntry], index_only: bool) -> Result<()> {
    let changed: Vec<_> = entries
        .iter()
        .filter(|entry| entry.source != entry.processed.result.output)
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    let index_path = git_path(root, "index")?;
    let index_path = if index_path.is_absolute() {
        index_path
    } else {
        root.join(index_path)
    };
    let original_index = fs::read(&index_path)
        .with_context(|| format!("cannot read Git index {}", index_path.display()))?;
    if index_path.with_file_name("index.lock").exists() {
        bail!("Git index is locked; no files were modified");
    }
    let mut temporary_index = NamedTempFile::new_in(index_path.parent().unwrap_or(root))?;
    temporary_index.write_all(&original_index)?;
    temporary_index.flush()?;
    temporary_index.as_file_mut().sync_all()?;
    // Close the file before Git replaces it through `<path>.lock`; retaining an
    // open NamedTempFile handle makes this update fail on Windows.
    let temporary_path = temporary_index.into_temp_path();

    for entry in &changed {
        let oid = hash_object(root, &entry.processed.result.output)?;
        let path = path_for_git(&entry.path);
        let status = Command::new("git")
            .current_dir(root)
            .env("GIT_INDEX_FILE", &temporary_path)
            .args(["update-index", "--add", "--cacheinfo", &entry.mode, &oid])
            .arg(path)
            .status()
            .context("cannot update temporary Git index")?;
        if !status.success() {
            bail!("git update-index failed; no files were modified");
        }
    }
    let replacement_index = fs::read(&temporary_path)?;
    let mut plans = Vec::new();
    if !index_only {
        for entry in &changed {
            let working_path = root.join(&entry.path);
            let working = fs::read(&working_path).with_context(|| {
                format!(
                    "cannot map staged fix to {}; use --index-only to update only the index",
                    working_path.display()
                )
            })?;
            let replacement = if working == entry.source {
                entry.processed.result.output.clone()
            } else {
                map_edits_uniquely(&entry.source, &working, &entry.processed.result.edits).with_context(|| {
                    format!("unstaged changes in {} make the staged fix ambiguous; no files were modified (use --index-only)", entry.path.display())
                })?
            };
            plans.push(WritePlan {
                path: working_path,
                original: working,
                replacement,
            });
        }
    }
    // Treat the index itself as the last journaled file. The shared transaction
    // rolls working-tree files and index back together on any rename failure.
    plans.push(WritePlan {
        path: index_path,
        original: original_index,
        replacement: replacement_index,
    });
    apply_transaction(plans)
}

fn map_edits_uniquely(index: &[u8], working: &[u8], edits: &[Edit]) -> Result<Vec<u8>> {
    let mut mapped = Vec::with_capacity(edits.len());
    for edit in edits {
        let context_start = edit.span.start.saturating_sub(24);
        let context_end = (edit.span.end + 24).min(index.len());
        let context = &index[context_start..context_end];
        let occurrences: Vec<_> = working
            .windows(context.len())
            .enumerate()
            .filter_map(|(position, window)| (window == context).then_some(position))
            .collect();
        if occurrences.len() != 1 {
            bail!("edit context does not have one unique working-tree mapping");
        }
        let delta = edit.span.start - context_start;
        mapped.push(Edit {
            span: ByteSpan::new(
                occurrences[0] + delta,
                occurrences[0] + delta + edit.span.len(),
            ),
            replacement: edit.replacement.clone(),
        });
    }
    mapped.sort_by_key(|edit| edit.span.start);
    if mapped
        .windows(2)
        .any(|pair| pair[0].span.end > pair[1].span.start)
    {
        bail!("mapped edits overlap");
    }
    Ok(apply_edits(working, &mapped))
}

fn repository_root() -> Result<PathBuf> {
    let mut output = command_output(
        Command::new("git").args(["rev-parse", "--show-toplevel"]),
        "not inside a Git repository",
    )?;
    trim_line_ending(&mut output);
    Ok(bytes_to_path(&output))
}

fn staged_paths(root: &Path, filters: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut command = Command::new("git");
    command.current_dir(root).args([
        "diff",
        "--cached",
        "--name-only",
        "-z",
        "--diff-filter=ACMR",
        "--",
    ]);
    command.args(filters);
    let output = command_output(&mut command, "cannot list staged paths")?;
    let mut paths: Vec<_> = output
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(bytes_to_path)
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn index_blob(root: &Path, path: &Path) -> Result<Vec<u8>> {
    let mut specification = OsString::from(":");
    specification.push(path_for_git(path));
    command_output(
        Command::new("git")
            .current_dir(root)
            .arg("cat-file")
            .arg("blob")
            .arg(specification),
        &format!("cannot read staged blob {}", path.display()),
    )
}

fn index_mode(root: &Path, path: &Path) -> Result<String> {
    let output = command_output(
        Command::new("git")
            .current_dir(root)
            .args(["ls-files", "-s", "--"])
            .arg(path_for_git(path)),
        "cannot read staged file mode",
    )?;
    output
        .split(|byte| byte.is_ascii_whitespace())
        .next()
        .filter(|value| !value.is_empty())
        .map(std::str::from_utf8)
        .transpose()
        .context("Git index mode is not ASCII")?
        .map(str::to_owned)
        .context("staged file has no index mode")
}

fn added_line_ranges(root: &Path, path: &Path) -> Result<Vec<std::ops::Range<usize>>> {
    let output = command_output(
        Command::new("git")
            .current_dir(root)
            .args(["diff", "--cached", "--unified=0", "--no-color", "--"])
            .arg(path_for_git(path)),
        "cannot read staged diff",
    )?;
    let text = String::from_utf8_lossy(&output);
    let mut ranges = Vec::new();
    for line in text.lines().filter(|line| line.starts_with("@@ ")) {
        let Some(plus) = line
            .split_ascii_whitespace()
            .find(|part| part.starts_with('+'))
        else {
            continue;
        };
        let range = plus.trim_start_matches('+');
        let (start, length) = range.split_once(',').unwrap_or((range, "1"));
        let start: usize = start.parse()?;
        let length: usize = length.parse()?;
        if length > 0 {
            ranges.push(start..start + length);
        }
    }
    Ok(ranges)
}

fn line_number(source: &[u8], offset: usize) -> usize {
    source[..offset.min(source.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

fn hash_object(root: &Path, bytes: &[u8]) -> Result<String> {
    let mut child = Command::new("git")
        .current_dir(root)
        .args(["hash-object", "-w", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(bytes)
        .context("cannot write the rewritten blob to git hash-object")?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "git hash-object failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

fn git_path(root: &Path, name: &str) -> Result<PathBuf> {
    let mut output = command_output(
        Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "--git-path", name]),
        "cannot locate Git index",
    )?;
    trim_line_ending(&mut output);
    Ok(bytes_to_path(&output))
}

fn trim_line_ending(bytes: &mut Vec<u8>) {
    while matches!(bytes.last(), Some(b'\r' | b'\n')) {
        bytes.pop();
    }
}

fn command_output(command: &mut Command, context: &str) -> Result<Vec<u8>> {
    let output = command.output().with_context(|| context.to_owned())?;
    if !output.status.success() {
        return Err(anyhow!(
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn path_for_git(path: &Path) -> &OsStr {
    path.as_os_str()
}

#[cfg(unix)]
fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(OsString::from_vec(bytes.to_vec()))
}

#[cfg(not(unix))]
fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unique_context_mapping_preserves_unstaged_prefix() {
        let index = b"a\nlet x; /* remove */\nz\n";
        let working = b"unstaged\na\nlet x; /* remove */\nz\n";
        let start = index.windows(2).position(|window| window == b"/*").unwrap();
        let end = index.windows(2).position(|window| window == b"*/").unwrap() + 2;
        let output = map_edits_uniquely(
            index,
            working,
            &[Edit {
                span: ByteSpan::new(start, end),
                replacement: Vec::new(),
            }],
        )
        .unwrap();
        assert_eq!(output, b"unstaged\na\nlet x; \nz\n");
    }
}
