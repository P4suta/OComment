use crate::config::ResolvedConfig;
use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use ocomment_core::{DeclarativeProfile, Detection, Dialect, Language, detect_language};
use std::{
    fs,
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
}

#[derive(Default)]
pub struct Discovery {
    pub files: Vec<SourceFile>,
    pub skipped: Vec<SkippedFile>,
}

pub fn discover(
    paths: &[PathBuf],
    resolved: &ResolvedConfig,
    forced_language: Option<Language>,
    forced_dialect: Option<Dialect>,
) -> Result<Discovery> {
    discover_with_scope(paths, resolved, forced_language, forced_dialect, true)
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
                    }),
                }
            }
        } else {
            discovery.skipped.push(SkippedFile {
                path,
                reason: "path does not exist".into(),
                error: true,
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

#[allow(clippy::too_many_arguments)]
fn load_one(
    path: &Path,
    explicit_scope: bool,
    resolved: &ResolvedConfig,
    forced_language: Option<Language>,
    forced_dialect: Option<Dialect>,
    include: &GlobSet,
    exclude: &GlobSet,
    discovery: &mut Discovery,
) {
    let relative = path.strip_prefix(&resolved.root).unwrap_or(path);
    if (!include.is_empty() && !include.is_match(relative)) || exclude.is_match(relative) {
        return;
    }
    let link_metadata = match path.symlink_metadata() {
        Ok(value) => value,
        Err(error) => {
            discovery.skipped.push(skip(path, error));
            return;
        }
    };
    let metadata = if link_metadata.file_type().is_symlink() {
        if !resolved.config.files.follow_symlinks {
            discovery.skipped.push(SkippedFile {
                path: path.to_path_buf(),
                reason: "symbolic link".into(),
                error: false,
            });
            return;
        }
        match path.metadata() {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return,
            Err(error) => {
                discovery.skipped.push(skip(path, error));
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
        });
        return;
    }
    let source = match fs::read(path) {
        Ok(value) => value,
        Err(error) => {
            discovery.skipped.push(skip(path, error));
            return;
        }
    };
    if source.iter().take(8192).any(|byte| *byte == 0) {
        discovery.skipped.push(SkippedFile {
            path: path.to_path_buf(),
            reason: "binary file (NUL byte)".into(),
            error: false,
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
            reason: "unknown language".into(),
            error: false,
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

fn skip(path: &Path, error: impl std::fmt::Display) -> SkippedFile {
    SkippedFile {
        path: path.to_path_buf(),
        reason: error.to_string(),
        error: true,
    }
}
