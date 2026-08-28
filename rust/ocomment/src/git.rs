use crate::{
    atomic::{WritePlan, apply_transaction},
    config::ResolvedConfig,
    files::SkippedFile,
    output::{
        self, Operation, OutputFormat, Presentation, ProcessedFile, ProcessedResult, RenderOptions,
        Verbosity,
    },
    plugin::PluginHost,
};
use anyhow::{Context, Result, anyhow, bail};
use ocomment_core::{
    ByteSpan, CommentKind, Diagnostic, Edit, PreparedScanner, Severity, TransformPlan, apply_edits,
    detect_language,
};
use std::{
    borrow::Cow,
    collections::{BTreeSet, HashMap},
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};
use tempfile::NamedTempFile;

struct IndexEntry {
    path: PathBuf,
    mode: String,
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
    let (blobs, mut skipped) = configured_paths(&root, staged_paths(&root, paths)?, resolved)?;
    let materialize_output = operation == Operation::Fix
        || (operation == Operation::Diff && format == OutputFormat::Human);
    let materialize_source_map = matches!(format, OutputFormat::Json | OutputFormat::Jsonl);
    let mut scanners = HashMap::new();
    let mut entries = Vec::new();
    for StagedBlob { path, named, mode } in blobs {
        let source = index_blob(&root, &path)?;
        if source.iter().take(8192).any(|byte| *byte == 0) {
            /* NOTE: A walk says why it passed a file over, and so does this: a hook
             * that stages a PNG beside its source has to read as one file
             * scanned and one passed over, not as two files with nothing
             * to say about them. */
            skipped.push(skipped_blob(
                path,
                "binary file (NUL byte)".to_owned(),
                named,
            ));
            continue;
        }
        let detection = forced_language
            .map(|language| ocomment_core::Detection {
                language,
                dialect: forced_dialect.unwrap_or(ocomment_core::Dialect::Standard),
                reason: "command-line",
            })
            .or_else(|| detect_language(Some(&path), &source));
        let detected_language = detection
            .as_ref()
            .map_or(ocomment_core::Language::Unknown, |value| value.language);
        let detected_dialect = detection
            .as_ref()
            .map_or(ocomment_core::Dialect::Standard, |value| value.dialect);
        let (language, options) = resolved.for_path(&path, detected_language, detected_dialect)?;
        if !resolved.language_is_enabled(language) {
            skipped.push(skipped_blob(
                path,
                "language disabled by configuration".to_owned(),
                named,
            ));
            continue;
        }
        let scanner = if let Some(scanner) = scanners.get(&options.scan) {
            Arc::clone(scanner)
        } else {
            let scanner = Arc::new(
                PreparedScanner::new(options.scan.clone())
                    .context("cannot prepare staged comment policy")?,
            );
            scanners.insert(options.scan.clone(), Arc::clone(&scanner));
            scanner
        };
        let profile = (language == ocomment_core::Language::Unknown)
            .then(|| crate::files::profile_for_path(&path, resolved))
            .flatten();
        let routed_plugin = (language == ocomment_core::Language::Unknown && profile.is_none())
            .then(|| crate::files::plugin_for_path(&path, resolved))
            .flatten();
        if language == ocomment_core::Language::Unknown
            && profile.is_none()
            && routed_plugin.is_none()
        {
            skipped.push(skipped_blob(
                path,
                crate::files::NO_LANGUAGE.to_owned(),
                named,
            ));
            continue;
        }
        let full = if let Some(profile) = &profile {
            scanner
                .transform_profile_plan(&source, profile, options.layout)
                .expect("profiles were validated while loading configuration")
        } else if let Some(name) = &routed_plugin {
            let language_name = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown")
                .to_ascii_lowercase();
            plugin_host.transform_plan(name, &source, &language_name, &path, &options, &scanner)?
        } else {
            scanner.transform_plan(&source, language, options.layout)
        };
        let ranges = added_line_ranges(&root, &path)?;
        let lines = LineNumberIndex::new(&source);
        let mut selected_comments = Vec::new();
        let mut conflict = None;
        for comment in &full.report.comments {
            let start_line = lines.line_number(comment.span.start);
            let end_line = lines.line_number(comment.span.end.saturating_sub(1));
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
                let line = lines.line_number(edit.span.start);
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
        let selected = TransformPlan {
            edits: selected_edits,
            report,
        };
        let result = ProcessedResult::plan(
            &source,
            selected,
            materialize_output,
            materialize_source_map,
        );
        let processed = ProcessedFile {
            path: path.clone(),
            source,
            language,
            result,
        };
        entries.push(IndexEntry {
            mode,
            path,
            processed,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    /* NOTE: The size skips were found before the blobs were read and the rest while
     * reading them, so the two arrive interleaved by nothing at all; a
     * machine format publishes this list, which owes its reader one order. */
    skipped.sort_by(|left, right| left.path.cmp(&right.path));
    let invalid = entries
        .iter()
        .any(|entry| !entry.processed.result.report.valid);
    let staged_conflict = entries.iter().any(|entry| {
        entry
            .processed
            .result
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
    let files: Vec<_> = entries.into_iter().map(|entry| entry.processed).collect();
    output::render(
        &files,
        &skipped,
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
            policy: resolved.config.policy.mode,
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
        .filter(|entry| entry.processed.result.changed())
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
        bail!(
            "Git index is locked; no files were modified; another Git process may be \
             running, or remove a stale .git/index.lock"
        );
    }
    let mut temporary_index = NamedTempFile::new_in(index_path.parent().unwrap_or(root))?;
    temporary_index.write_all(&original_index)?;
    temporary_index.flush()?;
    temporary_index.as_file_mut().sync_all()?;
    /* NOTE: Close the file before Git replaces it through `<path>.lock`; retaining an
     * open NamedTempFile handle makes this update fail on Windows. */
    let temporary_path = temporary_index.into_temp_path();

    for entry in &changed {
        let oid = hash_object(root, entry.processed.result.output())?;
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
            let replacement = if working == entry.processed.source {
                Cow::Borrowed(entry.processed.result.output())
            } else {
                Cow::Owned(map_edits_uniquely(&entry.processed.source, &working, &entry.processed.result.edits).with_context(|| {
                    format!("unstaged changes in {} make the staged fix ambiguous; no files were modified (use --index-only)", entry.path.display())
                })?)
            };
            plans.push(WritePlan {
                path: working_path,
                original: Cow::Owned(working),
                replacement,
            });
        }
    }
    /* INVARIANT: Treat the index itself as the last journaled file. The shared transaction
     * rolls working-tree files and index back together on any rename failure. */
    plans.push(WritePlan {
        path: index_path,
        original: Cow::Owned(original_index),
        replacement: Cow::Owned(replacement_index),
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
        /* NOTE: Git's own words follow: they name the directory it searched from,
         * which is the difference between "wrong directory" and "no repository". */
        "--staged needs a Git repository",
    )?;
    trim_line_ending(&mut output);
    Ok(bytes_to_path(&output))
}

/// Every staged path a run has to consider, and which of them the caller
/// named.
struct StagedPaths {
    /// What `git diff --cached` answered: root-relative, sorted, each path
    /// once however many pathspecs covered it.
    paths: Vec<PathBuf>,
    /// The paths a pathspec picked out, which is what lifts the project's own
    /// limits from them.
    named: BTreeSet<PathBuf>,
}

/// A staged path `[files]` let through, and whether the caller named it.
struct StagedBlob {
    path: PathBuf,
    named: bool,
    mode: String,
}

/// Ask `git` what is staged, one question for each pathspec.
///
/// A pathspec is `git`'s to interpret and nobody else's. `.hidden/*.rs` is a
/// wildcard it expands, an absolute path is one it makes root-relative, `.` is
/// a directory it resolves against the directory the command was typed in, and
/// the answer to all of them is a path relative to the repository root. So
/// each pathspec is put to `git` on its own and the answers are unioned, which
/// leaves the run with both of the things it needs — the paths to scan, and
/// the paths a caller asked about — without restating a word of pathspec
/// syntax here, and so without the two readings drifting apart.
///
/// It costs one `git` invocation for each pathspec the caller typed, where the
/// single combined question it replaces cost one for all of them. A hook that
/// passes its staged file names in one by one pays that per name, next to the
/// four this run already spends on every path it keeps, and it buys the only
/// reading of a pathspec that `git` itself would agree with.
fn staged_paths(root: &Path, filters: &[PathBuf]) -> Result<StagedPaths> {
    let base = pathspec_base(root);
    let mut paths = Vec::new();
    let mut named = BTreeSet::new();
    if filters.is_empty() {
        paths = list_staged(&base, None)?;
    } else {
        for pathspec in filters {
            let listed = list_staged(&base, Some(pathspec))?;
            if !names_whole_tree(pathspec, &base, root) {
                named.extend(listed.iter().cloned());
            }
            paths.extend(listed);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(StagedPaths { paths, named })
}

/// The staged paths one pathspec covers, or all of them when there is none.
fn list_staged(base: &Path, pathspec: Option<&Path>) -> Result<Vec<PathBuf>> {
    let mut command = Command::new("git");
    command.current_dir(base).args([
        "diff",
        "--cached",
        "--name-only",
        "-z",
        "--diff-filter=ACMR",
        "--",
    ]);
    if let Some(pathspec) = pathspec {
        command.arg(pathspec);
    }
    let output = command_output(&mut command, "cannot list staged paths")?;
    Ok(output
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(bytes_to_path)
        .collect())
}

/// The directory a relative pathspec is measured from.
///
/// It is the directory the command was typed in, which is what `git` resolves
/// `.` or `../lib` against. `resolved.cwd` cannot answer this question: a
/// staged run points it at the repository root before it begins, because a
/// staged path arrives root-relative and a `[files]` glob is written
/// root-relative too. The root stands in where the working directory cannot be
/// read at all: it is inside the repository by construction, so the worst it
/// can do is read a relative pathspec as the top of the tree would.
fn pathspec_base(root: &Path) -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| root.to_path_buf())
}

/// Whether a pathspec covers the repository, and so picks nothing out of it.
///
/// `ocomment check --staged .` from the top of the repository asks for the run
/// that `ocomment check --staged` already is, and it has to get that run's
/// answer — every `[files]` limit included. Naming a path is what lifts those
/// limits, and the whole tree is not a path anybody picked out: a hook that
/// spells its run with a trailing `.` would otherwise put exactly the hidden
/// or oversized blob through a commit that a bare run passes over. The same
/// `.` typed in `src/` does pick a subtree out, which is why the pathspec is
/// resolved where it was written before it is compared.
///
/// Only a pathspec that is a path is understood here. `git`'s `:(magic)`
/// spellings are read as picking something out, which is the reading that
/// answers about the paths the caller wrote rather than silently dropping
/// them.
fn names_whole_tree(pathspec: &Path, base: &Path, root: &Path) -> bool {
    let joined = base.join(pathspec);
    let absolute = std::path::absolute(&joined).unwrap_or(joined);
    crate::config::lexical(&absolute) == crate::config::lexical(root)
}

/// Drop the staged paths `[files]` puts out of bounds, and say which of them
/// were passed over.
///
/// `git diff --cached` answers with every path the commit carries, which is a
/// different question from the one `[files]` answers: a vendored tree the
/// project excludes is still staged on the commit that updates it. A walk
/// applies `include` and `exclude` in `files::load_one`, so a staged run
/// applies them here, and to the same root-relative spelling — `git` names a
/// staged path relative to the repository root, and `run_target` has already
/// pointed `resolved.cwd` there for exactly this reason.
///
/// A staged path nobody named is a walked path: it never carries the licence
/// an explicit argument does to look past the project's own limits. That is the
/// whole of `[files]` and not just its two glob lists — `hidden` decides
/// whether a dot-directory is looked into at all and `max_size` decides how
/// much of a file is worth reading, and a hook that applied neither would put
/// through a commit exactly what a walk would never have reached.
///
/// A path the caller *did* name is the other case, and
/// [`StagedPaths::named`] is what tells the two apart.
/// `ocomment check --staged .hidden/x.rs` is a request about that file, so
/// answering "0 files" because the project does not walk into dot-directories
/// reads as a clean file rather than as a path out of bounds — which is why a
/// walk lifts both limits for an explicit argument, and why this lifts them
/// for the same argument spelled as a pathspec.
///
/// The two limits answer differently when they do apply, because they mean
/// differently. A hidden path was never a candidate, so it leaves no trace; an
/// oversized blob is a file the run *met* and declined, so it comes back as the
/// same folded "too large" skip a walk reports, counted in the summary rather
/// than annotated once per file.
fn configured_paths(
    root: &Path,
    staged: StagedPaths,
    resolved: &ResolvedConfig,
) -> Result<(Vec<StagedBlob>, Vec<SkippedFile>)> {
    let include = crate::files::compile_globs(&resolved.config.files.include)?;
    let exclude = crate::files::compile_globs(&resolved.config.files.exclude)?;
    let max_size = resolved.config.files.max_size;
    let StagedPaths { paths, named } = staged;
    let mut kept = Vec::new();
    let mut skipped = Vec::new();
    for path in paths {
        let relative = resolved.relative_to_root(&path);
        /* NOTE: The glob lists bound a named path too — a walk asks them about every
         * candidate before it asks anything else, and `load_one` asks them of
         * an explicit argument exactly as it asks them of a walked one. */
        if (!include.is_empty() && !include.is_match(&relative)) || exclude.is_match(&relative) {
            continue;
        }
        let explicit = named.contains(&path);
        if !explicit && !resolved.config.files.hidden && has_hidden_component(&path) {
            continue;
        }
        /* NOTE: Read the index mode before asking Git for blob bytes. A
         * symlink's blob is its target spelling and a gitlink names a commit,
         * neither of which is source text. */
        let mode = index_mode(root, &path)?;
        let special = match mode.as_str() {
            "120000" => Some("symbolic link"),
            "160000" => Some("Git submodule link"),
            _ => None,
        };
        if let Some(reason) = special {
            skipped.push(skipped_blob(path, reason.to_owned(), explicit));
            continue;
        }
        if !explicit && index_blob_size(root, &path)? > max_size {
            skipped.push(SkippedFile {
                path,
                reason: format!("larger than {max_size} bytes"),
                error: false,
                /* NOTE: Nobody typed this path, so its skip is folded into the summary
                 * exactly as a walked one is. */
                explicit: false,
            });
            continue;
        }
        kept.push(StagedBlob {
            path,
            named: explicit,
            mode,
        });
    }
    Ok((kept, skipped))
}

/// A staged blob that was met and declined.
///
/// Neither reason depends on what the caller typed — a PNG is not text and a
/// `.md` file has no scanner however it got into the commit — but who typed
/// the path decides where the skip is reported. One nobody named is counted in
/// the end-of-run summary under the short label [`crate::output::skip_label`]
/// gives it, and listed per file only when `-v` asks for the list; one the
/// caller named is answered on a line of its own, because
/// `ocomment check --staged notes.md` that says only "nothing to check" reads
/// as a clean file rather than as a file nothing could read.
fn skipped_blob(path: PathBuf, reason: String, named: bool) -> SkippedFile {
    SkippedFile {
        path,
        reason,
        error: false,
        explicit: named,
    }
}

/// Whether any component of a staged path is a hidden name.
///
/// `git` names a staged path relative to the repository root, so every
/// component of it is a real directory or file name — there is no walk root in
/// front to leave out, the way `ignore` leaves one out. A leading `.` is the
/// only byte that decides it, so a name that is not UTF-8 is judged on the
/// bytes it actually has rather than on a lossy reading of them.
fn has_hidden_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(component, Component::Normal(name) if name.as_encoded_bytes().starts_with(b"."))
    })
}

/// How `git` is asked for the staged version of a path: `:` names the index.
fn index_specification(path: &Path) -> OsString {
    let mut specification = OsString::from(":");
    specification.push(path_for_git(path));
    specification
}

/// How large the staged blob is, without reading it.
///
/// The size is asked of the index rather than of the working tree, because
/// `--staged` judges the bytes the commit will carry: a file can be a line
/// long on disk and a megabyte in the index, or the other way round.
///
/// Asking costs one `git` invocation for each path that got past the globs,
/// next to the three the run already spends on every path it keeps. It buys
/// the thing `max_size` exists for, which is that an oversized blob is never
/// brought into memory at all — measuring it from `index_blob`'s answer would
/// have read it first.
fn index_blob_size(root: &Path, path: &Path) -> Result<u64> {
    let mut output = command_output(
        Command::new("git")
            .current_dir(root)
            .arg("cat-file")
            .arg("-s")
            .arg(index_specification(path)),
        &format!("cannot measure staged blob {}", path.display()),
    )?;
    trim_line_ending(&mut output);
    std::str::from_utf8(&output)
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .with_context(|| format!("staged blob {} has no size", path.display()))
}

fn index_blob(root: &Path, path: &Path) -> Result<Vec<u8>> {
    command_output(
        Command::new("git")
            .current_dir(root)
            .arg("cat-file")
            .arg("blob")
            .arg(index_specification(path)),
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

struct LineNumberIndex {
    starts: Vec<usize>,
    source_len: usize,
}

impl LineNumberIndex {
    fn new(source: &[u8]) -> Self {
        let starts = source
            .iter()
            .enumerate()
            .filter_map(|(index, byte)| (*byte == b'\n').then_some(index + 1))
            .collect();
        Self {
            starts,
            source_len: source.len(),
        }
    }

    fn line_number(&self, offset: usize) -> usize {
        let offset = offset.min(self.source_len);
        self.starts.partition_point(|start| *start <= offset) + 1
    }
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
    fn staged_line_index_matches_the_previous_walk_at_every_offset() {
        for source in [&b""[..], &b"a\n\nb\r\nc\r"[..], &b"\nfirst\nsecond\n"[..]] {
            let lines = LineNumberIndex::new(source);
            for offset in 0..=source.len() + 2 {
                let bounded = offset.min(source.len());
                let expected = source[..bounded]
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count()
                    + 1;
                assert_eq!(lines.line_number(offset), expected);
            }
        }
    }

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
