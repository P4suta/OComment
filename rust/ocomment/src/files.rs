use crate::{config::ResolvedConfig, output::ReadBy};
use anyhow::{Context, Result, anyhow};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use ocomment_core::{
    DeclarativeProfile, Detection, Dialect, Language, TransformOptions, detect_language,
};
use rayon::prelude::*;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: Vec<u8>,
    pub language: Language,
    pub dialect: Dialect,
    /// The path-resolved policy and layout used for this source.
    /// Resolution is part of discovery so the parallel scan does not repeat it.
    pub options: TransformOptions,
    pub profile: Option<DeclarativeProfile>,
    pub plugin: Option<String>,
    /// What decided the language: `extension`, `reserved-filename`, `shebang`,
    /// `content`, `command-line`, or `configuration-routing`.
    ///
    /// Detection already answers this and the answer was being dropped.
    /// It is the first thing a run that scanned a file as the wrong language needs,
    /// and the only place it can come from is the decision itself.
    pub detection: &'static str,
}

impl SourceFile {
    /// What will read this file: the language that was detected, or the profile or plugin that claimed it when no language did.
    ///
    /// Routing already decided this and every caller was re-deriving it, or -- more often -- dropping it and reporting the `Language::Unknown` that a profile-read file necessarily carries.
    pub fn read_by(&self) -> ReadBy {
        match (&self.profile, &self.plugin) {
            (Some(profile), _) => ReadBy::Profile(profile.name.clone()),
            (None, Some(plugin)) => ReadBy::Plugin(plugin.clone()),
            (None, None) => ReadBy::Language,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SkippedFile {
    pub path: PathBuf,
    pub reason: String,
    pub error: bool,
    /// The path itself was named on the command line.
    /// Such a skip is always reported on its own line; a skip found while walking a directory is folded into the end-of-run summary instead.
    pub explicit: bool,
}

#[derive(Default)]
pub struct Discovery {
    pub files: Vec<SourceFile>,
    pub skipped: Vec<SkippedFile>,
    /// A configuration resolution failure applies to the run rather than to one unreadable path.
    /// It is carried out of the walk and returned after traversal unwinds, instead of being reported as an I/O skip.
    fatal: Option<anyhow::Error>,
}

impl Discovery {
    /// Fold one answer in.
    /// The first configuration failure is the one reported:
    /// they are all the same failure, and a run that listed it once per file would bury it.
    fn absorb(&mut self, looked: Looked) {
        match looked {
            Looked::Nothing => {}
            Looked::Found(file) => self.files.push(*file),
            Looked::Passed(skipped) => self.skipped.push(skipped),
            Looked::Fatal(error) => {
                if self.fatal.is_none() {
                    self.fatal = Some(error);
                }
            }
        }
    }
}

/// The path standard input is reported under.
/// It is not a real file name: the renderers print it, and the configuration override matcher sees it, exactly as it reads here.
pub const STDIN_PATH: &str = "<stdin>";

/// What both `strip` and a `-` target say when the bytes carry no signature to detect a language from.
/// Standard input has no name to fall back on, so the only way forward is for the caller to name the language.
pub const STDIN_LANGUAGE_HELP: &str = "cannot detect the language of standard input; \
pass --language <LANGUAGE> (see `ocomment languages`)";

/// Why a file OComment has no scanner for is passed over, and the two ways out of it: consult the list of what is built in, or name a language anyway.
///
/// The end-of-run summary must not repeat this sentence once per file, so it folds the reason onto a short key of its own; `output::skip_label` is what ties the two together.
pub const NO_LANGUAGE: &str =
    "no built-in language for this file (see `ocomment languages`; use --language to force)";

/// Why a named path was not found.
/// A relative path is resolved against the working directory, which is exactly what a caller who typed it from the wrong place cannot see, so the directory that was searched is named.
fn missing_path_reason() -> String {
    env::current_dir().map_or_else(
        |_| "path does not exist".to_owned(),
        |cwd| {
            format!(
                "path does not exist (checked relative to {})",
                cwd.display()
            )
        },
    )
}

/// Turn the bytes read from standard input into a source file the ordinary pipeline can process, or the skip that says why it cannot.
/// Detection has no path to work with, so it is driven by `--language` or by the contents.
///
/// Declarative profiles and plugins route on a file extension, which standard input does not have; a pipe is therefore always handled by a built-in language or not at all.
pub fn stdin_source(
    bytes: Vec<u8>,
    resolved: &ResolvedConfig,
    forced_language: Option<Language>,
    forced_dialect: Option<Dialect>,
) -> Result<SourceFile, SkippedFile> {
    let skipped = |reason: &str, error: bool| SkippedFile {
        path: PathBuf::from(STDIN_PATH),
        reason: reason.to_owned(),
        error,
        /* NOTE: Standard input was named on the command line, so its skip is always reported on its own line rather than folded into the summary. */
        explicit: true,
    };
    if bytes.iter().take(8192).any(|byte| *byte == 0) {
        return Err(skipped("binary file (NUL byte)", false));
    }
    let detection = forced_language
        .map(|language| Detection {
            language,
            dialect: forced_dialect.unwrap_or(Dialect::Standard),
            reason: "command-line",
        })
        .or_else(|| detect_language(None, &bytes));
    let Some(Detection {
        language,
        dialect,
        reason: detection_reason,
    }) = detection
    else {
        return Err(skipped(STDIN_LANGUAGE_HELP, true));
    };
    let (language, options) = resolved
        .for_path(
            Path::new(STDIN_PATH),
            language,
            forced_dialect.unwrap_or(dialect),
        )
        .map_err(|error| skipped(&error.to_string(), true))?;
    if forced_language.is_none() && !resolved.language_is_enabled(language) {
        return Err(skipped("language disabled by configuration", false));
    }
    Ok(SourceFile {
        path: PathBuf::from(STDIN_PATH),
        source: bytes,
        language,
        dialect: options.scan.dialect,
        options,
        profile: None,
        plugin: None,
        detection: detection_reason,
    })
}

/// What a command with no PATH walks.
///
/// The project root is where the configuration was found, not what the caller is looking at: a command run from a subdirectory checks that subdirectory,
/// the way every other file-walking developer tool does.
/// Reaching back up to the root would put files the caller cannot see — and, with `fix`, files they did not mean to rewrite — into the run.
pub const DEFAULT_TARGET: &str = ".";

/// The one name a walk never offers, whatever else was asked for.
///
/// `.git` is git's own storage rather than source, and `git` itself never treats it as a candidate for anything.
/// Neither may a tool that rewrites files in place: `ocomment fix .` in a fresh repository would otherwise rewrite every sample hook git had just written into `.git/hooks`.
/// Naming a directory lifts the hidden-file rule and so does `files.hidden`, so the exclusion cannot hang off either of them.
///
/// A submodule or a linked worktree keeps its `.git` as a *file* pointing at the storage instead of holding it, which is why the name is matched rather than the file type.
const GIT_DIRECTORY: &str = ".git";

pub fn discover(
    paths: &[PathBuf],
    resolved: &ResolvedConfig,
    forced_language: Option<Language>,
    forced_dialect: Option<Dialect>,
) -> Result<Discovery> {
    let implicit = [PathBuf::from(DEFAULT_TARGET)];
    /* NOTE: The substituted target stands in for an argument nobody typed, so it is walked with the ordinary limits: only a path the caller actually named is a request to look past the hidden-file and size rules. */
    let (paths, explicit) = if paths.is_empty() {
        (&implicit[..], false)
    } else {
        (paths, true)
    };
    discover_with_scope(paths, resolved, forced_language, forced_dialect, explicit)
}

/// Discover workspace roots with normal traversal limits.
/// Unlike explicit CLI paths, an LSP workspace folder must still honor hidden-file and size rules.
pub fn discover_workspace(paths: &[PathBuf], resolved: &ResolvedConfig) -> Result<Discovery> {
    discover_with_scope(paths, resolved, None, None, false)
}

/// The same, with a language and dialect the caller forced.
///
/// `--base` uses this: the paths come from Git rather than from the caller, so they are walked under the ordinary limits, but a `--language` on the same command line still has to reach them.
pub fn discover_workspace_with(
    paths: &[PathBuf],
    resolved: &ResolvedConfig,
    forced_language: Option<Language>,
    forced_dialect: Option<Dialect>,
) -> Result<Discovery> {
    discover_with_scope(paths, resolved, forced_language, forced_dialect, false)
}

/// One file's worth of bytes, judged as though they were the contents of `path`.
///
/// The path decides everything about the judgement — the language, the `[[overrides]]` that apply, whether the file is excluded at all — and the bytes are the ones the caller is proposing to put there.
/// That pair is what a pre-write hook has and what nothing else in this module accepts: a walk reads the bytes off the disk, and `-` has bytes with no name.
///
/// The returned [`Discovery`] holds the one file, or the one skip that says why there is nothing to judge.
/// `path` is never opened.
pub fn proposed_source(
    path: &Path,
    bytes: Vec<u8>,
    resolved: &ResolvedConfig,
    forced_language: Option<Language>,
    forced_dialect: Option<Dialect>,
) -> Result<Discovery> {
    let include = compile_globs(&resolved.config.files.include)?;
    let exclude = compile_globs(&resolved.config.files.exclude)?;
    let generated = crate::generated::Generated::load()?;
    let context = LoadContext {
        resolved,
        forced_language,
        forced_dialect,
        include: &include,
        exclude: &exclude,
        generated: &generated,
    };
    let mut discovery = Discovery::default();
    let path = reported_path(path);
    let relative = resolved.relative_to_root(&path);
    if (!include.is_empty() && !include.is_match(&relative)) || exclude.is_match(&relative) {
        return Ok(discovery);
    }
    match classify(&path, bytes, true, &context) {
        Looked::Fatal(error) => return Err(error),
        looked @ Looked::Nothing | looked @ Looked::Found(_) | looked @ Looked::Passed(_) => {
            discovery.absorb(looked)
        }
    }
    Ok(discovery)
}

fn discover_with_scope(
    paths: &[PathBuf],
    resolved: &ResolvedConfig,
    forced_language: Option<Language>,
    forced_dialect: Option<Dialect>,
    explicit_arguments: bool,
) -> Result<Discovery> {
    let include = compile_globs(&resolved.config.files.include)?;
    let exclude = compile_globs(&resolved.config.files.exclude)?;
    let generated = crate::generated::Generated::load()?;
    let loader = LoadContext {
        resolved,
        forced_language,
        forced_dialect,
        include: &include,
        exclude: &exclude,
        generated: &generated,
    };
    let mut discovery = Discovery::default();
    /* NOTE: Only an editor asking for its workspace arrives here without a target;
     * `discover` gives a command line the current directory instead. */
    let targets: Vec<_> = if paths.is_empty() {
        vec![(resolved.root.clone(), false)]
    } else {
        paths
            .iter()
            .cloned()
            .map(|path| (path, explicit_arguments))
            .collect()
    };
    /* NOTE: Every candidate the walk finds, gathered before any of them is opened.
     * Traversal is one thread's job and reading a thousand files is not, so the two are separated: the walk names them, and `load_one` answers for all of them at once below. */
    let mut candidates: Vec<(PathBuf, bool, bool)> = Vec::new();
    for (path, explicit_scope) in targets {
        if path.is_file()
            || path
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            candidates.push((path, explicit_scope, explicit_scope));
        } else if path.is_dir() {
            let mut builder = WalkBuilder::new(&path);
            let ignore = resolved.config.files.ignore;
            builder
                .follow_links(resolved.config.files.follow_symlinks)
                .standard_filters(ignore)
                /* NOTE: `standard_filters` also resets the hidden-file flag, so this must come afterwards for explicitly named directories. */
                .hidden(!explicit_scope && !resolved.config.files.hidden)
                .git_ignore(ignore)
                .git_global(ignore)
                .git_exclude(ignore)
                .ignore(ignore)
                .parents(ignore);
            if ignore {
                builder.add_custom_ignore_filename(".ocommentignore");
            }
            /* NOTE: The filter is never asked about the walk root, so a caller who names a path inside `.git` — or `.git` itself — is still answered; only what a walk *wanders* into is excluded. */
            builder.filter_entry(|entry| entry.file_name() != GIT_DIRECTORY);
            /* NOTE: `ignore` reads a directory per thread and answers out of order, so the candidates are sorted before they are looked at and the report comes out in the same order on every run.
             * A walk whose output depended on how the scheduler felt would be a walk whose diffs could not be reviewed. */
            builder.threads(rayon::current_num_threads());
            let found = std::sync::Mutex::new(Vec::new());
            let failed = std::sync::Mutex::new(Vec::new());
            builder.build_parallel().run(|| {
                let found = &found;
                let failed = &failed;
                let root = path.clone();
                Box::new(move |entry| {
                    match entry {
                        Ok(entry) if entry.file_type().is_some_and(|kind| kind.is_file()) => {
                            found
                                .lock()
                                .expect("the walk's collector is not poisoned")
                                .push(entry.into_path());
                        }
                        Ok(_) => {}
                        Err(error) => failed
                            .lock()
                            .expect("the walk's collector is not poisoned")
                            .push(SkippedFile {
                                path: root.clone(),
                                reason: error.to_string(),
                                error: true,
                                explicit: explicit_scope,
                            }),
                    }
                    ignore::WalkState::Continue
                })
            });
            let mut found = found.into_inner().expect("the walk finished");
            found.sort_unstable();
            candidates.extend(found.into_iter().map(|path| (path, explicit_scope, false)));
            discovery
                .skipped
                .extend(failed.into_inner().expect("the walk finished"));
        } else {
            discovery.skipped.push(SkippedFile {
                path,
                reason: missing_path_reason(),
                error: true,
                explicit: explicit_scope,
            });
        }
    }
    /* NOTE: Read and classified in parallel, folded in the order the candidates were gathered.
     * Reading is where the time goes -- a walk over a large repository is thousands of `open`, `read`, `close` and a language detection each -- and it is the part that has no reason to happen one file at a time. */
    for looked in candidates
        .par_iter()
        .map(|(path, explicit_scope, explicit_path)| {
            load_one(path, *explicit_scope, *explicit_path, &loader)
        })
        .collect::<Vec<_>>()
    {
        discovery.absorb(looked);
    }
    if let Some(error) = discovery.fatal.take() {
        return Err(error);
    }
    discovery
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    discovery
        .files
        .dedup_by(|left, right| left.path == right.path);
    /* INVARIANT: A path is reached twice whenever it is named beside a directory holding it, and it is one file either way: `files` says so with the sort and the dedup above, and a skip is one file just as much — a report that annotates the same path twice reads as two problems with it.
     * Which of the two entries survives is not arbitrary.
     * An error decides the exit code, and a path the caller actually typed is answered on a line of its own rather than folded into the summary, so the entry that says the most is sorted to the front of its path and is the one the dedup keeps. */
    discovery.skipped.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(right.error.cmp(&left.error))
            .then(right.explicit.cmp(&left.explicit))
    });
    discovery
        .skipped
        .dedup_by(|left, right| left.path == right.path);
    Ok(discovery)
}

/// The name a walked file is reported under.
///
/// The implicit target is `.`, so a walk rooted there hands back every entry as `./name`.
/// `ocomment` and `ocomment check name` report one file, and a reader — or a `git apply` reading the patch — is owed one spelling of it,
/// so the prefix the walk root contributed is dropped.
/// The target itself is left alone: `.` names a directory, and `` names nothing.
fn reported_path(path: &Path) -> PathBuf {
    match path.strip_prefix(DEFAULT_TARGET) {
        Ok(stripped) if !stripped.as_os_str().is_empty() => stripped.to_path_buf(),
        _ => path.to_path_buf(),
    }
}

/// Shared immutable inputs for loading one discovered path.
struct LoadContext<'a> {
    resolved: &'a ResolvedConfig,
    forced_language: Option<Language>,
    forced_dialect: Option<Dialect>,
    include: &'a GlobSet,
    exclude: &'a GlobSet,
    /// The catalogue of files another tool writes, parsed once per walk.
    generated: &'a crate::generated::Generated,
}

/// What looking at one path produced.
///
/// Returned rather than pushed, so that looking at a path is a pure function of the path and the configuration — which is what lets a walk look at a thousand of them at once and fold the answers in one deterministic order.
enum Looked {
    /// Excluded by a glob, or not a file at all.
    Nothing,
    Found(Box<SourceFile>),
    Passed(SkippedFile),
    /// A configuration failure, which applies to the run rather than to this path.
    Fatal(anyhow::Error),
}

fn load_one(
    path: &Path,
    explicit_scope: bool,
    explicit_path: bool,
    context: &LoadContext<'_>,
) -> Looked {
    let LoadContext {
        resolved,
        include,
        exclude,
        ..
    } = context;
    let path = &reported_path(path);
    /* NOTE: The globs are written relative to the root; the path was typed — or walked — relative to the working directory, so it is measured against the root before either set is asked about it. */
    let relative = resolved.relative_to_root(path);
    if (!include.is_empty() && !include.is_match(&relative)) || exclude.is_match(&relative) {
        return Looked::Nothing;
    }
    let link_metadata = match path.symlink_metadata() {
        Ok(value) => value,
        Err(error) => {
            return Looked::Passed(skip(path, explicit_path, error));
        }
    };
    let metadata = if link_metadata.file_type().is_symlink() {
        if !resolved.config.files.follow_symlinks {
            return Looked::Passed(SkippedFile {
                path: path.to_path_buf(),
                reason: "symbolic link".into(),
                error: false,
                explicit: explicit_path,
            });
        }
        match path.metadata() {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return Looked::Nothing,
            Err(error) => {
                return Looked::Passed(skip(path, explicit_path, error));
            }
        }
    } else {
        link_metadata
    };
    /* NOTE: Every path under an explicitly named directory is explicit for hidden and size handling. */
    if !explicit_scope && metadata.len() > resolved.config.files.max_size {
        return Looked::Passed(SkippedFile {
            path: path.to_path_buf(),
            reason: format!("larger than {} bytes", resolved.config.files.max_size),
            error: false,
            explicit: explicit_path,
        });
    }
    let source = match fs::read(path) {
        Ok(value) => value,
        Err(error) => {
            return Looked::Passed(skip(path, explicit_path, error));
        }
    };
    classify(path, source, explicit_path, context)
}

/// Everything deciding one file's fate that does not depend on reading it.
///
/// Split out from [`load_one`] because the bytes and the path are separable questions: [`proposed_source`] has a path that exists and contents that do not, and every rule below — the binary test, the generated catalogue, the language, the overrides, the profile and plugin routing — has to reach the same answer for it that a walk would reach for the file once it is written.
/// Two copies of this would be two answers.
fn classify(
    path: &Path,
    source: Vec<u8>,
    explicit_path: bool,
    context: &LoadContext<'_>,
) -> Looked {
    let LoadContext {
        resolved,
        forced_language,
        forced_dialect,
        generated,
        ..
    } = context;
    if source.iter().take(8192).any(|byte| *byte == 0) {
        return Looked::Passed(SkippedFile {
            path: path.to_path_buf(),
            reason: "binary file (NUL byte)".into(),
            error: false,
            explicit: explicit_path,
        });
    }
    /* NOTE: Before the language is chosen, because this is not a question about what the file is written in.
     * A lock file is perfectly readable TOML and a recorded seed list is perfectly readable prose; what makes them skippable is that the comments in them belong to the tool that will write them again. */
    if !resolved.config.files.include_generated && generated.claims(path, &source) {
        return Looked::Passed(SkippedFile {
            path: path.to_path_buf(),
            reason: crate::generated::REASON.into(),
            error: false,
            explicit: explicit_path,
        });
    }
    let built_in = (*forced_language)
        .map(|language| Detection {
            language,
            dialect: forced_dialect.unwrap_or(Dialect::Standard),
            reason: "command-line",
        })
        .or_else(|| detect_language(Some(path), &source));
    let Detection {
        language: detected_language,
        dialect: detected_dialect,
        reason: detection_reason,
    } = built_in.unwrap_or(Detection {
        language: Language::Unknown,
        dialect: Dialect::Standard,
        reason: "configuration-routing",
    });
    let (language, options) = match resolved.for_path(path, detected_language, detected_dialect) {
        Ok(value) => value,
        Err(error) => return Looked::Fatal(error),
    };
    if !resolved.language_is_enabled(language) {
        return Looked::Passed(SkippedFile {
            path: path.to_path_buf(),
            reason: "language disabled by configuration".into(),
            error: false,
            explicit: explicit_path,
        });
    }
    let profile = if language == Language::Unknown {
        profile_for_path(path, resolved)
    } else {
        None
    };
    let plugin = if language == Language::Unknown && profile.is_none() {
        plugin_for_path(path, resolved)
    } else {
        None
    };
    if language == Language::Unknown && profile.is_none() && plugin.is_none() {
        return Looked::Passed(SkippedFile {
            path: path.to_path_buf(),
            reason: NO_LANGUAGE.into(),
            error: false,
            explicit: explicit_path,
        });
    }
    Looked::Found(Box::new(SourceFile {
        path: path.to_path_buf(),
        source,
        language,
        dialect: options.scan.dialect,
        options,
        profile,
        plugin,
        detection: detection_reason,
    }))
}

pub fn plugin_for_path(path: &Path, resolved: &ResolvedConfig) -> Option<String> {
    let extension = path.extension()?.to_str()?.trim_start_matches('.');
    resolved
        .config
        .plugins
        .routes
        .get(&extension.to_ascii_lowercase())
        .cloned()
}

pub fn profile_for_path(path: &Path, resolved: &ResolvedConfig) -> Option<DeclarativeProfile> {
    let name = path.file_name().and_then(|value| value.to_str());
    let extension = path.extension().and_then(|value| value.to_str());
    resolved
        .config
        .profiles
        .values()
        .find(|profile| {
            /* NOTE: The whole name is tried first, so that a profile claiming `dune-project` wins over one claiming `.project`.
             * A name is the more specific claim of the two. */
            name.is_some_and(|name| profile.filenames.iter().any(|candidate| candidate == name))
                || extension.is_some_and(|extension| {
                    profile.extensions.iter().any(|candidate| {
                        candidate
                            .trim_start_matches('.')
                            .eq_ignore_ascii_case(extension)
                    })
                })
        })
        .cloned()
}

/// Compile one of the `[files]` glob lists.
///
/// A walk asks for these in `discover_with_scope` and a staged run asks for them in `git::run_staged`; both measure a path against the project root first, so both get the same answer for the same path.
pub(crate) fn compile_globs(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            /* INVARIANT: Both halves of this line came out of a file in the project: the pattern the caller wrote, and a `globset` parse error that quotes that same pattern straight back.
             * Neither may reach a terminal verbatim, and the line stays one line.
             * The pattern keeps the spacing it was written with, because a reader who is shown something else cannot find it in the file.
             * It is the same treatment `config::validate_regexes` gives the other pattern a project file carries. */
            anyhow!(
                "invalid file glob `{}`: {}",
                crate::output::sanitize_path(pattern),
                crate::output::sanitize_message(&error.to_string())
            )
        })?;
        builder.add(glob);
    }
    builder.build().context("cannot compile file globs")
}

fn skip(path: &Path, explicit: bool, error: impl std::fmt::Display) -> SkippedFile {
    SkippedFile {
        path: path.to_path_buf(),
        reason: error.to_string(),
        error: true,
        explicit,
    }
}

/// Why a file in the tree was never offered to the walk at all.
///
/// A skip is a file the walk reached and passed over, and it is reported.
/// This is the other thing: a file the walk's own limits kept out, which nothing reported because nothing met it.
/// `ocomment coverage` said `100.0%` over a repository whose every GitHub workflow was under `.github` and therefore hidden -- a true sentence about what was walked and a false assurance about what was checked.
///
/// A file a `.gitignore` excludes is deliberately not here.
/// It is not a gap in the gate: it is build output, and a percentage taken over a hundred thousand object files would mean nothing at all.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NotWalked {
    /// `[files] hidden = false`, and a path component opens with a dot.
    Hidden,
    /// `[files] include` did not name it, or `[files] exclude` did.
    Configured,
    /// `[files] max_size`.
    TooLarge,
}

impl NotWalked {
    /// The setting a reader would change, phrased as the report prints it.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Hidden => "hidden file or directory ([files] hidden = false)",
            Self::Configured => "excluded by configuration ([files] include/exclude)",
            Self::TooLarge => "larger than the size limit ([files] max_size)",
        }
    }
}

/// Every file under `paths` that this configuration's walk would not reach,
/// and the setting that kept each one out.
///
/// Nothing is read.
/// The walk here lifts only the hidden-file rule, so what it finds is the repository as its own ignore files describe it, and each path missing from `reached` is attributed to the first configured limit that would have stopped it -- in the order the walk applies them.
pub fn not_walked(
    paths: &[PathBuf],
    resolved: &ResolvedConfig,
    reached: &[PathBuf],
) -> Result<Vec<(PathBuf, NotWalked)>> {
    let include = compile_globs(&resolved.config.files.include)?;
    let exclude = compile_globs(&resolved.config.files.exclude)?;
    let implicit = [PathBuf::from(DEFAULT_TARGET)];
    let targets = if paths.is_empty() {
        &implicit[..]
    } else {
        paths
    };
    let reached: std::collections::HashSet<&Path> = reached.iter().map(PathBuf::as_path).collect();
    let mut missed = Vec::new();
    for target in targets {
        if !target.is_dir() {
            continue;
        }
        let ignore = resolved.config.files.ignore;
        let mut builder = WalkBuilder::new(target);
        builder
            .standard_filters(ignore)
            .hidden(false)
            .follow_links(resolved.config.files.follow_symlinks);
        if ignore {
            builder.add_custom_ignore_filename(".ocommentignore");
        }
        builder.filter_entry(|entry| entry.file_name() != GIT_DIRECTORY);
        for entry in builder.build().flatten() {
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let path = reported_path(entry.path());
            if reached.contains(path.as_path()) {
                continue;
            }
            if let Some(reason) = kept_out(&path, resolved, &include, &exclude) {
                missed.push((path, reason));
            }
        }
    }
    missed.sort();
    missed.dedup();
    Ok(missed)
}

/// Which of the walk's limits would have stopped `path`, tested in the order the walk applies them.
///
/// `None` cannot happen for a path this function is asked about: the caller has already taken out everything the walk reached, and the walk that found this one lifted exactly one rule.
/// It is returned rather than asserted because a filesystem that changed under the two walks is not a defect worth a panic.
fn kept_out(
    path: &Path,
    resolved: &ResolvedConfig,
    include: &GlobSet,
    exclude: &GlobSet,
) -> Option<NotWalked> {
    if !resolved.config.files.hidden
        && path
            .components()
            .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
    {
        return Some(NotWalked::Hidden);
    }
    let relative = resolved.relative_to_root(path);
    if (!include.is_empty() && !include.is_match(&relative)) || exclude.is_match(&relative) {
        return Some(NotWalked::Configured);
    }
    if path
        .metadata()
        .is_ok_and(|metadata| metadata.len() > resolved.config.files.max_size)
    {
        return Some(NotWalked::TooLarge);
    }
    None
}
