use anyhow::{Context, Result, anyhow, bail, ensure};
use globset::{Glob, GlobMatcher};
use ocomment_core::{
    CommentKind, DeclarativeProfile, Dialect, Language, Layout, Policy, ScanOptions,
    TransformOptions, validate_profile,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

pub const CONFIG_FILE: &str = ".ocomment.toml";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub version: Option<u32>,
    pub files: FilesConfig,
    pub policy: PolicyConfig,
    pub git: GitConfig,
    pub lsp: LspConfig,
    pub languages: BTreeMap<String, LanguageConfig>,
    pub profiles: BTreeMap<String, DeclarativeProfile>,
    pub plugins: PluginsConfig,
    pub overrides: Vec<PathOverride>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FilesConfig {
    pub max_size: u64,
    pub hidden: bool,
    pub follow_symlinks: bool,
    pub ignore: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl Default for FilesConfig {
    fn default() -> Self {
        Self {
            max_size: 32 * 1024 * 1024,
            hidden: false,
            follow_symlinks: false,
            ignore: true,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PolicyConfig {
    pub mode: Policy,
    pub layout: Layout,
    pub keep_kind: Vec<CommentKind>,
    pub remove_kind: Vec<CommentKind>,
    pub keep_regex: Vec<String>,
    pub remove_regex: Vec<String>,
    pub force_invalid: bool,
    pub force_protected: bool,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            mode: Policy::Safe,
            layout: Layout::Lines,
            keep_kind: Vec::new(),
            remove_kind: Vec::new(),
            keep_regex: Vec::new(),
            remove_regex: Vec::new(),
            force_invalid: false,
            force_protected: false,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GitConfig {
    pub staged: bool,
    pub index_only: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LspConfig {
    pub on_save: bool,
    pub diagnostics: bool,
    pub code_lens: bool,
}

impl Default for LspConfig {
    fn default() -> Self {
        Self {
            on_save: false,
            diagnostics: true,
            code_lens: true,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LanguageConfig {
    pub enabled: Option<bool>,
    pub dialect: Option<Dialect>,
    pub policy: Option<Policy>,
    pub layout: Option<Layout>,
    pub keep_kind: Vec<CommentKind>,
    pub remove_kind: Vec<CommentKind>,
    pub keep_regex: Vec<String>,
    pub remove_regex: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PluginsConfig {
    pub enabled: Vec<String>,
    /// Maps a lowercase extension (without `.`) to a locked plugin name.
    pub routes: BTreeMap<String, String>,
    pub memory_mib: Option<u32>,
    pub instances: Option<u32>,
    pub fuel_per_byte: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PathOverride {
    pub paths: Vec<String>,
    pub language: Option<Language>,
    pub dialect: Option<Dialect>,
    pub policy: Option<Policy>,
    pub layout: Option<Layout>,
    pub keep_kind: Vec<CommentKind>,
    pub remove_kind: Vec<CommentKind>,
    pub keep_regex: Vec<String>,
    pub remove_regex: Vec<String>,
}

struct CompiledOverride {
    matchers: Vec<GlobMatcher>,
    value: PathOverride,
}

#[derive(Clone, Debug, Default)]
pub struct ConfigTrace {
    pub user: Option<PathBuf>,
    pub project: Option<PathBuf>,
    pub explicit: Option<PathBuf>,
}

pub struct ResolvedConfig {
    pub config: Config,
    pub trace: ConfigTrace,
    pub root: PathBuf,
    overrides: Vec<CompiledOverride>,
}

impl ResolvedConfig {
    pub fn for_path(
        &self,
        path: &Path,
        language: Language,
        dialect: Dialect,
    ) -> (Language, TransformOptions) {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        let normalized = relative.to_string_lossy().replace('\\', "/");
        let mut chosen_language = language;
        let mut chosen_dialect = dialect;
        let mut policy = self.config.policy.mode;
        let mut layout = self.config.policy.layout;
        let mut keep = self.config.policy.keep_kind.clone();
        let mut remove = self.config.policy.remove_kind.clone();
        let mut keep_regex = self.config.policy.keep_regex.clone();
        let mut remove_regex = self.config.policy.remove_regex.clone();

        if let Some(language_config) = self.config.languages.get(language.as_str()) {
            if let Some(value) = language_config.dialect {
                chosen_dialect = value;
            }
            if let Some(value) = language_config.policy {
                policy = value;
            }
            if let Some(value) = language_config.layout {
                layout = value;
            }
            extend_unique(&mut keep, &language_config.keep_kind);
            extend_unique(&mut remove, &language_config.remove_kind);
            extend_unique(&mut keep_regex, &language_config.keep_regex);
            extend_unique(&mut remove_regex, &language_config.remove_regex);
        }
        for override_ in &self.overrides {
            if override_
                .matchers
                .iter()
                .any(|matcher| matcher.is_match(&normalized))
            {
                if let Some(value) = override_.value.language {
                    chosen_language = value;
                }
                if let Some(value) = override_.value.dialect {
                    chosen_dialect = value;
                }
                if let Some(value) = override_.value.policy {
                    policy = value;
                }
                if let Some(value) = override_.value.layout {
                    layout = value;
                }
                extend_unique(&mut keep, &override_.value.keep_kind);
                extend_unique(&mut remove, &override_.value.remove_kind);
                extend_unique(&mut keep_regex, &override_.value.keep_regex);
                extend_unique(&mut remove_regex, &override_.value.remove_regex);
            }
        }
        let scan = ScanOptions {
            policy,
            dialect: chosen_dialect,
            force_invalid: self.config.policy.force_invalid,
            force_protected: self.config.policy.force_protected,
            keep_kinds: keep,
            remove_kinds: remove,
            keep_regex,
            remove_regex,
        };
        (chosen_language, TransformOptions { scan, layout })
    }
}

pub fn load(explicit: Option<&Path>) -> Result<ResolvedConfig> {
    let cwd = env::current_dir().context("cannot determine current directory")?;
    let project_path = locate_project(&cwd);
    let root = project_path
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .or_else(|| locate_repository(&cwd))
        .unwrap_or_else(|| cwd.clone());
    let user_path = user_config_path().filter(|path| path.is_file());
    let mut merged: toml::Value = toml::from_str(&toml::to_string(&Config::default())?)?;
    let mut trace = ConfigTrace::default();
    if let Some(path) = &user_path {
        merge_value(&mut merged, parse_layer(path, false)?);
        trace.user = Some(path.clone());
    }
    if let Some(path) = &project_path {
        merge_value(&mut merged, parse_layer(path, true)?);
        trace.project = Some(path.clone());
    }
    if let Some(path) = explicit {
        merge_value(&mut merged, parse_layer(path, true)?);
        trace.explicit = Some(path.to_path_buf());
    }
    let mut config: Config = merged
        .try_into()
        .context("cannot resolve merged configuration")?;
    for (name, profile) in &mut config.profiles {
        if profile.name.is_empty() {
            profile.name = name.clone();
        }
        validate_profile(profile).map_err(|error| anyhow!(error))?;
    }
    validate_languages(&config)?;
    validate_policy_regexes(&config)?;
    let overrides = compile_overrides(&config.overrides)?;
    Ok(ResolvedConfig {
        config,
        trace,
        root,
        overrides,
    })
}

fn validate_languages(config: &Config) -> Result<()> {
    for (name, language_config) in &config.languages {
        let language: Language = name
            .parse()
            .map_err(|_| anyhow!("unknown language configuration key `{name}`"))?;
        if let Some(dialect) = language_config.dialect {
            validate_dialect(language, dialect)
                .with_context(|| format!("invalid dialect for [languages.{name}]"))?;
        }
    }
    for item in &config.overrides {
        if let (Some(language), Some(dialect)) = (item.language, item.dialect) {
            validate_dialect(language, dialect)
                .context("invalid language/dialect path override")?;
        }
    }
    Ok(())
}

/// Every dialect the scanner accepts for `language`, in canonical order.
pub fn supported_dialects(language: Language) -> &'static [Dialect] {
    match language {
        Language::JavaScript => &[Dialect::Standard, Dialect::Jsx],
        Language::TypeScript => &[Dialect::Standard, Dialect::Tsx],
        Language::C => &[Dialect::Standard, Dialect::ObjectiveC, Dialect::GnuC],
        Language::Cpp => &[
            Dialect::Standard,
            Dialect::ObjectiveCpp,
            Dialect::GnuCpp,
            Dialect::Cuda,
        ],
        Language::Shell => &[
            Dialect::Standard,
            Dialect::PosixSh,
            Dialect::Bash53,
            Dialect::Zsh,
        ],
        Language::Sql => &[
            Dialect::Standard,
            Dialect::PostgreSql,
            Dialect::MySql,
            Dialect::Sqlite,
            Dialect::TSql,
            Dialect::Oracle,
        ],
        _ => &[Dialect::Standard],
    }
}

pub fn validate_dialect(language: Language, dialect: Dialect) -> Result<()> {
    let supported = supported_dialects(language);
    ensure!(
        supported.contains(&dialect),
        "unsupported dialect `{dialect}` for {language}; supported: {}",
        supported
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

fn validate_policy_regexes(config: &Config) -> Result<()> {
    let patterns = config
        .policy
        .keep_regex
        .iter()
        .chain(&config.policy.remove_regex)
        .chain(
            config
                .languages
                .values()
                .flat_map(|language| language.keep_regex.iter().chain(&language.remove_regex)),
        )
        .chain(
            config
                .overrides
                .iter()
                .flat_map(|item| item.keep_regex.iter().chain(&item.remove_regex)),
        );
    for pattern in patterns {
        regex::bytes::Regex::new(pattern)
            .with_context(|| format!("invalid comment policy regex `{pattern}`"))?;
    }
    Ok(())
}

fn parse_layer(path: &Path, require_version: bool) -> Result<toml::Value> {
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let config: Config = toml::from_str(&text).map_err(|error| {
        let message = error.to_string();
        anyhow!(
            "invalid configuration {}: {}{}",
            path.display(),
            message,
            unknown_key_hint(&message)
        )
    })?;
    if require_version && config.version != Some(1) {
        bail!("{} must contain `version = 1`", path.display());
    }
    toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
}

fn merge_value(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base), toml::Value::Table(overlay)) => {
            for (key, value) in overlay {
                if key == "overrides" {
                    match value {
                        toml::Value::Array(mut incoming) => {
                            if let Some(toml::Value::Array(existing)) = base.get_mut(&key) {
                                existing.append(&mut incoming);
                            } else {
                                base.insert(key, toml::Value::Array(incoming));
                            }
                        }
                        other => {
                            base.insert(key, other);
                        }
                    }
                } else if let Some(existing) = base.get_mut(&key) {
                    merge_value(existing, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

fn compile_overrides(overrides: &[PathOverride]) -> Result<Vec<CompiledOverride>> {
    overrides
        .iter()
        .cloned()
        .map(|value| {
            if value.paths.is_empty() {
                bail!("each [[overrides]] entry needs at least one path glob");
            }
            let matchers = value
                .paths
                .iter()
                .map(|pattern| {
                    Glob::new(pattern)
                        .with_context(|| format!("invalid override glob `{pattern}`"))
                        .map(|glob| glob.compile_matcher())
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(CompiledOverride { matchers, value })
        })
        .collect()
}

pub fn locate_project(start: &Path) -> Option<PathBuf> {
    let mut directory = Some(start);
    while let Some(current) = directory {
        let candidate = current.join(CONFIG_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        directory = current.parent();
    }
    None
}

/// Locate the nearest repository without starting `git`. A `.git` directory
/// and the indirection file used by worktrees are both accepted. This keeps
/// the no-argument command fast while making its scope the current repository.
pub fn locate_repository(start: &Path) -> Option<PathBuf> {
    let start = if start.is_dir() {
        start
    } else {
        start.parent()?
    };
    start
        .ancestors()
        .find(|directory| directory.join(".git").exists())
        .map(Path::to_path_buf)
}

pub fn user_config_path() -> Option<PathBuf> {
    if let Some(base) = env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(base).join("ocomment/config.toml"));
    }
    env::var_os("HOME").map(|base| PathBuf::from(base).join(".config/ocomment/config.toml"))
}

fn extend_unique<T: Clone + Eq>(target: &mut Vec<T>, values: &[T]) {
    for value in values {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn unknown_key_hint(message: &str) -> String {
    let Some(after) = message.split("unknown field `").nth(1) else {
        return String::new();
    };
    let Some((unknown, rest)) = after.split_once('`') else {
        return String::new();
    };
    let candidates = rest
        .split("expected one of ")
        .nth(1)
        .unwrap_or("")
        .split([',', '`', '\'', ' '])
        .filter(|value| {
            !value.is_empty()
                && value
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        });
    let suggestion = candidates.min_by_key(|candidate| edit_distance(unknown, candidate));
    suggestion
        .filter(|candidate| edit_distance(unknown, candidate) <= 3)
        .map_or_else(String::new, |candidate| {
            format!("; did you mean `{candidate}`?")
        })
}

fn edit_distance(left: &str, right: &str) -> usize {
    let right_length = right.len();
    let mut row: Vec<usize> = (0..=right_length).collect();
    for (i, a) in left.bytes().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, b) in right.bytes().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(a != b));
            diagonal = old;
        }
    }
    row[right_length]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typo_hint_uses_nearest_key() {
        let message = "unknown field `layuot`, expected one of `mode`, `layout`, `keep_kind`";
        assert!(unknown_key_hint(message).contains("layout"));
    }

    #[test]
    fn repository_root_accepts_directory_and_worktree_markers() {
        for marker_is_directory in [true, false] {
            let root = tempfile::tempdir().unwrap();
            if marker_is_directory {
                fs::create_dir(root.path().join(".git")).unwrap();
            } else {
                fs::write(root.path().join(".git"), "gitdir: elsewhere\n").unwrap();
            }
            let nested = root.path().join("a/b");
            fs::create_dir_all(&nested).unwrap();
            assert_eq!(locate_repository(&nested), Some(root.path().to_path_buf()));
        }
    }
}
