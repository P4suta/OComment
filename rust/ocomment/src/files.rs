use crate::config::ResolvedConfig;
use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use ocomment_core::{DeclarativeProfile, Detection, Dialect, Language, detect_language};
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
    pub profile: Option<DeclarativeProfile>,
    pub plugin: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SkippedFile {
    pub path: PathBuf,
    pub reason: String,
    pub error: bool,
    /// The path itself was named on the command line. Such a skip is always
    /// reported on its own line; a skip found while walking a directory is
    /// folded into the end-of-run summary instead.
    pub explicit: bool,
}

#[derive(Default)]
pub struct Discovery {
    pub files: Vec<SourceFile>,
    pub skipped: Vec<SkippedFile>,
}

/// The path standard input is reported under. It is not a real file name: the
/// renderers print it, and the configuration override matcher sees it, exactly
/// as it reads here.
pub const STDIN_PATH: &str = "<stdin>";

/// What both `strip` and a `-` target say when the bytes carry no signature to
/// detect a language from. Standard input has no name to fall back on, so the
/// only way forward is for the caller to name the language.
pub const STDIN_LANGUAGE_HELP: &str = "cannot detect the language of standard input; \
pass --language <LANGUAGE> (see `ocomment languages`)";

/// Why a file OComment has no scanner for is passed over, and the two ways out
/// of it: consult the list of what is built in, or name a language anyway.
///
/// The end-of-run summary must not repeat this sentence once per file, so it
/// folds the reason onto a short key of its own; `output::skip_label` is what
/// ties the two together.
pub const NO_LANGUAGE: &str =
    "no built-in language for this file (see `ocomment languages`; use --language to force)";

/// Why a named path was not found. A relative path is resolved against the
/// working directory, which is exactly what a caller who typed it from the
/// wrong place cannot see, so the directory that was searched is named.
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

/// Turn the bytes read from standard input into a source file the ordinary
/// pipeline can process, or the skip that says why it cannot. Detection has no
/// path to work with, so it is driven by `--language` or by the contents.
///
/// Declarative profiles and plugins route on a file extension, which standard
/// input does not have; a pipe is therefore always handled by a built-in
/// language or not at all.
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
        // Standard input was named on the command line, so its skip is always
        // reported on its own line rather than folded into the summary.
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
        language, dialect, ..
    }) = detection
    else {
        return Err(skipped(STDIN_LANGUAGE_HELP, true));
    };
    if forced_language.is_none()
        && resolved
            .config
            .languages
            .get(language.as_str())
            .and_then(|item| item.enabled)
            == Some(false)
    {
        return Err(skipped("language disabled by configuration", false));
    }
    Ok(SourceFile {
        path: PathBuf::from(STDIN_PATH),
        source: bytes,
        language,
        dialect: forced_dialect.unwrap_or(dialect),
        profile: None,
        plugin: None,
    })
}

/// What a command with no PATH walks.
///
/// The project root is where the configuration was found, not what the caller
/// is looking at: a command run from a subdirectory checks that subdirectory,
/// the way every other file-walking developer tool does. Reaching back up to
/// the root would put files the caller cannot see — and, with `fix`, files
/// they did not mean to rewrite — into the run.
pub const DEFAULT_TARGET: &str = ".";

pub fn discover(
    paths: &[PathBuf],
    resolved: &ResolvedConfig,
    forced_language: Option<Language>,
    forced_dialect: Option<Dialect>,
) -> Result<Discovery> {
    let implicit = [PathBuf::from(DEFAULT_TARGET)];
    // The substituted target stands in for an argument nobody typed, so it is
    // walked with the ordinary limits: only a path the caller actually named
    // is a request to look past the hidden-file and size rules.
    let (paths, explicit) = if paths.is_empty() {
        (&implicit[..], false)
    } else {
        (paths, true)
    };
    discover_with_scope(paths, resolved, forced_language, forced_dialect, explicit)
}

/// Discover workspace roots with normal traversal limits. Unlike explicit CLI
/// paths, an LSP workspace folder must still honor hidden-file and size rules.
pub fn discover_workspace(paths: &[PathBuf], resolved: &ResolvedConfig) -> Result<Discovery> {
    discover_with_scope(paths, resolved, None, None, false)
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
    let mut discovery = Discovery::default();
    // Only an editor asking for its workspace arrives here without a target;
    // `discover` gives a command line the current directory instead.
    let targets: Vec<_> = if paths.is_empty() {
        vec![(resolved.root.clone(), false)]
    } else {
        paths
            .iter()
            .cloned()
            .map(|path| (path, explicit_arguments))
            .collect()
    };
    for (path, explicit_scope) in targets {
        if path.is_file()
            || path
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            load_one(
                &path,
                explicit_scope,
                explicit_scope,
                resolved,
                forced_language,
                forced_dialect,
                &include,
                &exclude,
                &mut discovery,
            );
        } else if path.is_dir() {
            let mut builder = WalkBuilder::new(&path);
            let ignore = resolved.config.files.ignore;
            builder
                .follow_links(resolved.config.files.follow_symlinks)
                .standard_filters(ignore)
                // `standard_filters` also resets the hidden-file flag, so this
                // must come afterwards for explicitly named directories.
                .hidden(!explicit_scope && !resolved.config.files.hidden)
                .git_ignore(ignore)
                .git_global(ignore)
                .git_exclude(ignore)
                .ignore(ignore)
                .parents(ignore);
            if ignore {
                builder.add_custom_ignore_filename(".ocommentignore");
            }
            for entry in builder.build() {
                match entry {
                    Ok(entry) if entry.file_type().is_some_and(|kind| kind.is_file()) => {
                        load_one(
                            entry.path(),
                            explicit_scope,
                            false,
                            resolved,
                            forced_language,
                            forced_dialect,
                            &include,
                            &exclude,
                            &mut discovery,
                        );
                    }
                    Ok(_) => {}
                    Err(error) => discovery.skipped.push(SkippedFile {
                        path: path.clone(),
                        reason: error.to_string(),
                        error: true,
                        explicit: explicit_scope,
                    }),
                }
            }
        } else {
            discovery.skipped.push(SkippedFile {
                path,
                reason: missing_path_reason(),
                error: true,
                explicit: explicit_scope,
            });
        }
    }
    discovery
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    discovery
        .files
        .dedup_by(|left, right| left.path == right.path);
    discovery
        .skipped
        .sort_by(|left, right| left.path.cmp(&right.path));
    Ok(discovery)
}

/// The name a walked file is reported under.
///
/// The implicit target is `.`, so a walk rooted there hands back every entry
/// as `./name`. `ocomment` and `ocomment check name` report one file, and a
/// reader — or a `git apply` reading the patch — is owed one spelling of it,
/// so the prefix the walk root contributed is dropped. The target itself is
/// left alone: `.` names a directory, and `` names nothing.
fn reported_path(path: &Path) -> PathBuf {
    match path.strip_prefix(DEFAULT_TARGET) {
        Ok(stripped) if !stripped.as_os_str().is_empty() => stripped.to_path_buf(),
        _ => path.to_path_buf(),
    }
}

#[allow(clippy::too_many_arguments)]
fn load_one(
    path: &Path,
    explicit_scope: bool,
    explicit_path: bool,
    resolved: &ResolvedConfig,
    forced_language: Option<Language>,
    forced_dialect: Option<Dialect>,
    include: &GlobSet,
    exclude: &GlobSet,
    discovery: &mut Discovery,
) {
    let path = &reported_path(path);
    // The globs are written relative to the root; the path was typed — or
    // walked — relative to the working directory, so it is measured against
    // the root before either set is asked about it.
    let relative = resolved.relative_to_root(path);
    if (!include.is_empty() && !include.is_match(&relative)) || exclude.is_match(&relative) {
        return;
    }
    let link_metadata = match path.symlink_metadata() {
        Ok(value) => value,
        Err(error) => {
            discovery.skipped.push(skip(path, explicit_path, error));
            return;
        }
    };
    let metadata = if link_metadata.file_type().is_symlink() {
        if !resolved.config.files.follow_symlinks {
            discovery.skipped.push(SkippedFile {
                path: path.to_path_buf(),
                reason: "symbolic link".into(),
                error: false,
                explicit: explicit_path,
            });
            return;
        }
        match path.metadata() {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return,
            Err(error) => {
                discovery.skipped.push(skip(path, explicit_path, error));
                return;
            }
        }
    } else {
        link_metadata
    };
    // Every path under an explicitly named directory is explicit for hidden and size handling.
    if !explicit_scope && metadata.len() > resolved.config.files.max_size {
        discovery.skipped.push(SkippedFile {
            path: path.to_path_buf(),
            reason: format!("larger than {} bytes", resolved.config.files.max_size),
            error: false,
            explicit: explicit_path,
        });
        return;
    }
    let source = match fs::read(path) {
        Ok(value) => value,
        Err(error) => {
            discovery.skipped.push(skip(path, explicit_path, error));
            return;
        }
    };
    if source.iter().take(8192).any(|byte| *byte == 0) {
        discovery.skipped.push(SkippedFile {
            path: path.to_path_buf(),
            reason: "binary file (NUL byte)".into(),
            error: false,
            explicit: explicit_path,
        });
        return;
    }
    let built_in = forced_language
        .map(|language| Detection {
            language,
            dialect: forced_dialect.unwrap_or(Dialect::Standard),
            reason: "command-line",
        })
        .or_else(|| detect_language(Some(path), &source));
    if forced_language.is_none()
        && built_in.as_ref().is_some_and(|detection| {
            resolved
                .config
                .languages
                .get(detection.language.as_str())
                .and_then(|language| language.enabled)
                == Some(false)
        })
    {
        discovery.skipped.push(SkippedFile {
            path: path.to_path_buf(),
            reason: "language disabled by configuration".into(),
            error: false,
            explicit: explicit_path,
        });
        return;
    }
    let profile = if built_in.is_none() {
        profile_for_path(path, resolved)
    } else {
        None
    };
    let plugin = if built_in.is_none() && profile.is_none() {
        plugin_for_path(path, resolved)
    } else {
        None
    };
    let Detection {
        language, dialect, ..
    } = built_in.unwrap_or(Detection {
        language: Language::Unknown,
        dialect: Dialect::Standard,
        reason: "declarative-profile",
    });
    if language == Language::Unknown && profile.is_none() && plugin.is_none() {
        discovery.skipped.push(SkippedFile {
            path: path.to_path_buf(),
            reason: NO_LANGUAGE.into(),
            error: false,
            explicit: explicit_path,
        });
        return;
    }
    discovery.files.push(SourceFile {
        path: path.to_path_buf(),
        source,
        language,
        dialect: forced_dialect.unwrap_or(dialect),
        profile,
        plugin,
    });
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
    let extension = path.extension()?.to_str()?.trim_start_matches('.');
    resolved
        .config
        .profiles
        .values()
        .find(|profile| {
            profile.extensions.iter().any(|candidate| {
                candidate
                    .trim_start_matches('.')
                    .eq_ignore_ascii_case(extension)
            })
        })
        .cloned()
}

fn compile_globs(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).with_context(|| format!("invalid file glob `{pattern}`"))?);
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
