//! Agent editing hooks: the same check, spoken in an agent host's protocol.
//!
//! A hook host hands its hook a description of an edit on standard input and
//! reads a decision back. Nothing in this module decides anything: it works
//! out which bytes are about to become which file, hands that pair to the same
//! machinery `ocomment check` runs, and writes the answer in the shape the
//! host reads. The judgement, the configuration, the policy and the report are
//! the ones every other command uses.
//!
//! This is where the coupling lives, deliberately and in one file — the same
//! arrangement as `editors/` and `action.yml`, which speak an editor's and a
//! CI system's protocols without either reaching into the scanner. Supporting
//! another host is one more [`Surface`] and one more `decide` arm.

use crate::{
    cli::CommonArgs,
    config, deadline, files, output,
    output::{Explanations, FileExplanation, Operation, OutputFormat, RenderOptions},
    plugin,
};
use anyhow::{Context, Result};
use clap::ValueEnum;
use ocomment_core::PreparedScanner;
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// An agent host whose editing hooks OComment can answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Surface {
    /// Claude Code's `PreToolUse` and `PostToolUse` hooks.
    ClaudeCode,
}

/// What the run is being asked about: the bytes, and the path they are for.
///
/// `None` is the ordinary answer. Most hook events are about something that is
/// not a file — a command, a prompt, the end of a session — and a hook with no
/// opinion has to be silent rather than guess.
type Subject = Option<(PathBuf, Vec<u8>)>;

pub fn run(surface: Surface, common: &CommonArgs) -> Result<u8> {
    let mut payload = String::new();
    std::io::stdin()
        .read_to_string(&mut payload)
        .context("cannot read the hook payload from standard input")?;
    match surface {
        Surface::ClaudeCode => claude_code(&payload, common),
    }
}

/// Claude Code's hook payload, cut down to the fields a comment check needs.
///
/// Unknown fields are ignored rather than refused: the payload grows, and a
/// hook that failed on a field it had never heard of would break every editing
/// session the day the host added one.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ClaudeCodeHook {
    hook_event_name: String,
    tool_name: String,
    tool_input: ToolInput,
    /// The directory the session is working in, which is where the
    /// configuration is discovered from.
    cwd: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ToolInput {
    file_path: Option<String>,
    /// `Write`: the whole file, as it is about to be.
    content: Option<String>,
    /// `Edit`: one replacement against the file as it stands.
    old_string: Option<String>,
    new_string: Option<String>,
    replace_all: bool,
    /// `MultiEdit`: several, applied in order.
    edits: Vec<Replacement>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Replacement {
    old_string: String,
    new_string: String,
    replace_all: bool,
}

/// Whether this event is about a file that is about to change, or one that just
/// did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum When {
    /// The edit has not happened. Refusing it keeps the comment out of the file
    /// rather than reporting it once it is in.
    Before,
    /// The edit has happened and the bytes are on the disk.
    After,
}

fn claude_code(payload: &str, common: &CommonArgs) -> Result<u8> {
    /* NOTE: A payload this run cannot parse is the host's business rather than
     * the edit's, so it is reported as a hook failure — exit 1, which Claude
     * Code treats as non-blocking — instead of standing in the way of an edit
     * nothing has actually judged. */
    let hook: ClaudeCodeHook =
        serde_json::from_str(payload).context("cannot read the hook payload as JSON")?;
    let when = match hook.hook_event_name.as_str() {
        "PreToolUse" => When::Before,
        "PostToolUse" => When::After,
        _ => return Ok(0),
    };
    let Some((path, bytes)) = subject(&hook, when)? else {
        return Ok(0);
    };
    let subject = match when {
        When::Before => output::Subject::Proposed,
        When::After => output::Subject::OnDisk,
    };
    let Some(report) = judge(
        &path,
        bytes,
        hook.cwd.as_deref().map(Path::new),
        common,
        subject,
    )?
    else {
        return Ok(0);
    };
    match when {
        /* NOTE: A denial carries its own reason and exits 0, because exit 2
         * would take the reason from standard error instead and the two would
         * have to be kept in step. Nothing here ever answers `allow`: that
         * would wave the edit past the permission rules its user set, and this
         * hook was asked about comments. */
        When::Before => {
            let decision = json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": report,
                }
            });
            let mut stdout = output::stdout();
            output::wrote(writeln!(stdout, "{decision}"))?;
            output::finish(&mut stdout)?;
            Ok(0)
        }
        /* NOTE: The edit already happened, so there is nothing left to refuse
         * and the report is a correction. Exit 2 is how this host puts one in
         * front of the model; the text comes from standard error. */
        When::After => {
            let stderr = std::io::stderr();
            let mut sink = stderr.lock();
            output::wrote(writeln!(sink, "{report}"))?;
            Ok(2)
        }
    }
}

/// The tools whose events are about a file that is about to hold different
/// bytes.
///
/// Named rather than inferred from the payload, because several tools carry a
/// `file_path` and only these put anything in the file. Reading one is not an
/// edit, and a hook that blocked on a file the agent had merely read would be
/// reporting a comment nobody had just written.
const EDITING_TOOLS: [&str; 4] = ["Write", "Edit", "MultiEdit", "NotebookEdit"];

/// The path and the bytes this event is about, or `None` if it is about
/// something else.
fn subject(hook: &ClaudeCodeHook, when: When) -> Result<Subject> {
    if !EDITING_TOOLS.contains(&hook.tool_name.as_str()) {
        return Ok(None);
    }
    let Some(path) = hook.tool_input.file_path.as_deref().map(PathBuf::from) else {
        return Ok(None);
    };
    if when == When::After {
        /* NOTE: Read rather than reconstructed. Whatever the tool reported it
         * would do, the file is the file. */
        return Ok(std::fs::read(&path).ok().map(|bytes| (path, bytes)));
    }
    let input = &hook.tool_input;
    /* NOTE: `Write` carries the whole file; the two edit tools carry
     * replacements against the file as it stands, so the file is read and the
     * replacements applied the way the tool is about to apply them. A
     * replacement that does not match is an edit the tool will refuse on its
     * own, and this hook says nothing about it rather than judging bytes that
     * will never exist. */
    if let Some(content) = &input.content {
        return Ok(Some((path, content.clone().into_bytes())));
    }
    let replacements: Vec<Replacement> = match (&input.old_string, &input.new_string) {
        (Some(old), Some(new)) => vec![Replacement {
            old_string: old.clone(),
            new_string: new.clone(),
            replace_all: input.replace_all,
        }],
        _ if !input.edits.is_empty() => input
            .edits
            .iter()
            .map(|edit| Replacement {
                old_string: edit.old_string.clone(),
                new_string: edit.new_string.clone(),
                replace_all: edit.replace_all,
            })
            .collect(),
        _ => return Ok(None),
    };
    let Ok(current) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let mut proposed = current;
    for replacement in replacements {
        if !proposed.contains(&replacement.old_string) {
            return Ok(None);
        }
        proposed = if replacement.replace_all {
            proposed.replace(&replacement.old_string, &replacement.new_string)
        } else {
            proposed.replacen(&replacement.old_string, &replacement.new_string, 1)
        };
    }
    Ok(Some((path, proposed.into_bytes())))
}

/// The agent report for `bytes` judged as the contents of `path`, or `None`
/// when there is nothing to say.
///
/// Everything below is the same call `ocomment check` makes. A hook that
/// scanned differently from the command would be a second implementation of
/// the project's policy, and the first thing it would disagree with is the
/// gate the project already runs.
fn judge(
    path: &Path,
    bytes: Vec<u8>,
    cwd: Option<&Path>,
    common: &CommonArgs,
    subject: output::Subject,
) -> Result<Option<String>> {
    let mut resolved = match cwd {
        Some(cwd) => config::load_from(cwd, common.config())?,
        None => config::load(common.config())?,
    };
    crate::cli::apply_cli_overrides(&mut resolved, common);
    let plugin_host = plugin::PluginHost::load(&resolved.root, &resolved.config.plugins)?;
    let discovery =
        files::proposed_source(path, bytes, &resolved, common.language(), common.dialect())?;
    let mut processed = Vec::new();
    let mut explanations = Explanations::new();
    for file in discovery.files {
        let scanner = PreparedScanner::new(file.options.scan.clone())
            .context("cannot prepare comment policy")?;
        let mut report = crate::cli::scan_bytes(&file.source, &file, &scanner, &plugin_host)?;
        /* NOTE: The bytes under judgement are not the ones on the disk, and the
         * deadline is read from the history of the file they would become —
         * `git blame --contents` answers for exactly that. Without this an
         * editing hook would be the one surface where a promise never ran
         * out. */
        deadline::apply(
            &resolved.root,
            &file.path,
            &file.source,
            &mut report,
            &file.options.scan.allow,
            std::time::SystemTime::now(),
        )?;
        let changed = (report.valid || scanner.options().force_invalid)
            && report
                .comments
                .iter()
                .any(|comment| comment.disposition().action().changes_bytes());
        let (_, _, trace) = resolved.for_path_traced(&file.path, file.language, file.dialect)?;
        explanations.insert(
            file.path.clone(),
            FileExplanation {
                options: file.options.scan.clone(),
                trace,
            },
        );
        let read_by = file.read_by();
        processed.push(output::ProcessedFile {
            path: file.path,
            source: file.source,
            language: file.language,
            read_by,
            result: output::ProcessedResult::report(report, changed),
        });
    }
    let options = RenderOptions {
        format: OutputFormat::Agent,
        operation: Operation::Check,
        presentation: output::Presentation::default(),
        verbosity: output::Verbosity::default(),
        preview: common.preview(),
        json: common.json_options(),
        explain: false,
        dry_run: false,
        force_invalid: resolved.config.policy.force_invalid,
        applied: false,
        policy: resolved.config.policy.mode,
        annotation_level: None,
    };
    Ok(output::agent_report(
        &processed,
        &discovery.skipped,
        &options,
        &explanations,
        subject,
    ))
}
