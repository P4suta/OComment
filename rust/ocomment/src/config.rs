use anyhow::{Context, Result, anyhow, bail, ensure};
use globset::{Glob, GlobMatcher};
use ocomment_core::{
    CommentKind, DeclarativeProfile, Dialect, DispositionExplanation, Language, Layout, Policy,
    ScanOptions, TransformOptions, validate_profile,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Component, Path, PathBuf},
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

/// Where one effective setting came from.
///
/// The layers are the ones [`ResolvedConfig::for_path`] merges, and a source
/// names the layer a value arrived on rather than the value itself, so
/// `--explain` can send a reader to the table they have to edit.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Source {
    /// The `[policy]` table, or the built-in default when no file set it.
    #[default]
    Global,
    /// A `[languages.<name>]` table.
    Language(String),
    /// The `[[overrides]]` table at `index`, whose globs matched the path.
    Override { index: usize, paths: Vec<String> },
    /// A flag on the command line.
    Cli { flag: &'static str },
}

impl Source {
    /// How an explanation names this source, given the file a `Global` value
    /// was written in.
    ///
    /// `#N` counts an `[[overrides]]` table from zero, the way the regex
    /// indices printed beside it count the patterns they address.
    fn describe(&self, origin: Option<&Path>) -> String {
        match self {
            Self::Global => match origin {
                Some(path) => format!("[policy] in {}", path.display()),
                None => "built-in defaults".to_owned(),
            },
            Self::Language(name) => format!("[languages.{name}]"),
            Self::Override { index, paths } => {
                format!("[[overrides]] #{index}, paths = {paths:?}")
            }
            Self::Cli { flag } => format!("{flag} on the command line"),
        }
    }
}

/// The `[policy]` keys a trace can attribute to a file, spelled as the file
/// spells them.
const POLICY_KEYS: [&str; 6] = [
    "mode",
    "layout",
    "keep_kind",
    "remove_kind",
    "keep_regex",
    "remove_regex",
];

/// Which configuration file last set each `[policy]` key. A key no file sets
/// keeps no entry, and an explanation calls it a built-in default rather than
/// sending the reader to a file that never mentions it.
type PolicyOrigins = BTreeMap<&'static str, PathBuf>;

/// What the command line overrode, recorded while it was applied so a trace
/// can say the command line rather than the file the value would otherwise
/// have been written in.
#[derive(Clone, Copy, Debug, Default)]
pub struct CliOverrides {
    pub policy: bool,
    pub layout: bool,
    /// Where the `--keep-kind` values start in `policy.keep_kind`: the command
    /// line appends to the configured list instead of replacing it, so only
    /// the tail of that list belongs to the command line.
    pub keep_kind_from: Option<usize>,
    /// The same boundary for `--remove-kind` in `policy.remove_kind`.
    pub remove_kind_from: Option<usize>,
}

/// Where every effective setting for one path came from.
///
/// The `*_kind` and `*_regex` vectors run parallel to the vectors in the
/// [`ScanOptions`] that [`ResolvedConfig::for_path_traced`] returned beside
/// this: entry `i` says which layer contributed entry `i` of that list.
#[derive(Clone, Debug, Default)]
pub struct PolicyTrace {
    pub policy: Source,
    /// Where the layout came from. No disposition depends on the layout, so no
    /// explanation names it yet; it is recorded because the trace answers the
    /// question for the whole `[policy]` block and `--explain` will not be its
    /// only caller.
    #[allow(dead_code)]
    pub layout: Source,
    pub keep_kind: Vec<Source>,
    pub remove_kind: Vec<Source>,
    pub keep_regex: Vec<Source>,
    pub remove_regex: Vec<Source>,
    origins: PolicyOrigins,
}

impl PolicyTrace {
    /// Where the setting that decided `explanation` came from, worded the way
    /// `--explain` prints it, or `None` when a built-in rule decided it and
    /// there is no table to point at.
    ///
    /// `options` is the one the explanation was produced from: a regex
    /// explanation carries its index into those lists, and a kind explanation
    /// is found by the kind it names.
    pub fn origin_of(
        &self,
        explanation: &DispositionExplanation,
        options: &ScanOptions,
    ) -> Option<String> {
        let position = |kinds: &[CommentKind], kind: &CommentKind| {
            kinds.iter().position(|value| value == kind)
        };
        let (source, key) = match explanation {
            DispositionExplanation::KeptByKind(kind) => (
                self.keep_kind.get(position(&options.keep_kinds, kind)?)?,
                "keep_kind",
            ),
            DispositionExplanation::RemovedByKind(kind) => (
                self.remove_kind
                    .get(position(&options.remove_kinds, kind)?)?,
                "remove_kind",
            ),
            DispositionExplanation::KeptByRegex { index, .. } => {
                (self.keep_regex.get(*index)?, "keep_regex")
            }
            DispositionExplanation::RemovedByRegex { index, .. } => {
                (self.remove_regex.get(*index)?, "remove_regex")
            }
            // Every one of these is the policy having the last word, whether it
            // took the comment out or protected it.
            DispositionExplanation::RemovedByPolicy(_)
            | DispositionExplanation::RemovedByDefault(_)
            | DispositionExplanation::KeptLicense { .. } => (&self.policy, "mode"),
            // A built-in rule, decided by no setting at all.
            DispositionExplanation::ProtectedPreamble
            | DispositionExplanation::KeptHtml
            | DispositionExplanation::KeptDirective { .. } => return None,
        };
        Some(source.describe(self.origins.get(key).map(PathBuf::as_path)))
    }
}

/// One layer of the policy merge, as the trace replays it.
struct TracedLayer<'a> {
    source: Source,
    policy: Option<Policy>,
    layout: Option<Layout>,
    keep_kind: &'a [CommentKind],
    remove_kind: &'a [CommentKind],
    keep_regex: &'a [String],
    remove_regex: &'a [String],
}

/// Attribute every entry of one merged list to the layer that introduced it.
///
/// [`ResolvedConfig::for_path`] starts from the global list verbatim and then
/// appends whatever a later layer adds that is not there yet, so replaying that
/// walk reproduces the merged list position for position. The tail of the
/// global list from `cli_from` on is what `flag` appended to it.
fn attribute<T: Clone + Eq>(
    global: &[T],
    cli_from: Option<usize>,
    flag: &'static str,
    layers: &[(&Source, &[T])],
) -> Vec<Source> {
    let cli_from = cli_from.unwrap_or(usize::MAX);
    let mut sources: Vec<Source> = (0..global.len())
        .map(|index| {
            if index >= cli_from {
                Source::Cli { flag }
            } else {
                Source::Global
            }
        })
        .collect();
    let mut seen = global.to_vec();
    for (source, values) in layers {
        for value in *values {
            if !seen.contains(value) {
                seen.push(value.clone());
                sources.push((*source).clone());
            }
        }
    }
    sources
}

/// Where a setting that holds a single value came from, before any language or
/// path layer has had its say.
fn scalar_source(overridden: bool, flag: &'static str) -> Source {
    if overridden {
        Source::Cli { flag }
    } else {
        Source::Global
    }
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
    /// Where the project starts: the directory `.ocomment.toml` was found in,
    /// the repository above the working directory, or the working directory
    /// itself. It decides where configuration is discovered, what the file and
    /// override globs are written relative to, and where the plugin lock
    /// lives — no longer what a command with no path walks.
    pub root: PathBuf,
    /// The directory the command was run from, which is what a path typed on
    /// the command line is relative to.
    pub cwd: PathBuf,
    /// What the command line overrode, filled in after the files were merged.
    pub cli_overrides: CliOverrides,
    overrides: Vec<CompiledOverride>,
    origins: PolicyOrigins,
}

impl ResolvedConfig {
    /// Where `path` sits under the project root, spelled the way a
    /// configuration glob is written.
    ///
    /// `files.include`, `files.exclude`, and every `[[overrides]].paths`
    /// pattern is relative to the root, while a path named on the command line
    /// is relative to the working directory. The two agree only when the
    /// command is run from the root, so the path is resolved against the
    /// directory it was typed in before it is measured against the root, and
    /// the separators come out as forward slashes so one glob reads the same
    /// on every platform.
    ///
    /// A path outside the root — an explicit target above it, say — has no
    /// root-relative spelling at all, so it keeps its absolute one and only an
    /// absolute glob can match it.
    pub fn relative_to_root(&self, path: &Path) -> String {
        // Standard input has no place on disk. The pseudo-path is what the
        // renderers print, so it is also what the globs are shown.
        if path.as_os_str() == crate::files::STDIN_PATH {
            return crate::files::STDIN_PATH.to_owned();
        }
        let joined = self.cwd.join(path);
        let absolute = lexical(&std::path::absolute(&joined).unwrap_or(joined));
        let relative = absolute.strip_prefix(&self.root).unwrap_or(&absolute);
        relative.to_string_lossy().replace('\\', "/")
    }

    pub fn for_path(
        &self,
        path: &Path,
        language: Language,
        dialect: Dialect,
    ) -> (Language, TransformOptions) {
        let normalized = self.relative_to_root(path);
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

    /// The same answer as [`Self::for_path`], with a record of where each
    /// setting came from.
    ///
    /// The values are [`Self::for_path`]'s own, so what a run does and what
    /// `--explain` says about it cannot disagree; only the attribution is
    /// computed here, by replaying the same merge with the layer names
    /// attached. `--explain` is the only caller, which is why the hot path is
    /// left as it was.
    pub fn for_path_traced(
        &self,
        path: &Path,
        language: Language,
        dialect: Dialect,
    ) -> (Language, TransformOptions, PolicyTrace) {
        let (chosen_language, options) = self.for_path(path, language, dialect);
        let normalized = self.relative_to_root(path);
        let mut layers = Vec::new();
        // `for_path` looks the language table up under the language it was
        // handed, not under the one an override may have changed it to.
        if let Some(config) = self.config.languages.get(language.as_str()) {
            layers.push(TracedLayer {
                source: Source::Language(language.as_str().to_owned()),
                policy: config.policy,
                layout: config.layout,
                keep_kind: &config.keep_kind,
                remove_kind: &config.remove_kind,
                keep_regex: &config.keep_regex,
                remove_regex: &config.remove_regex,
            });
        }
        for (index, compiled) in self.overrides.iter().enumerate() {
            if !compiled
                .matchers
                .iter()
                .any(|matcher| matcher.is_match(&normalized))
            {
                continue;
            }
            let value = &compiled.value;
            layers.push(TracedLayer {
                source: Source::Override {
                    index,
                    paths: value.paths.clone(),
                },
                policy: value.policy,
                layout: value.layout,
                keep_kind: &value.keep_kind,
                remove_kind: &value.remove_kind,
                keep_regex: &value.keep_regex,
                remove_regex: &value.remove_regex,
            });
        }
        let cli = self.cli_overrides;
        let keep_kinds: Vec<_> = layers
            .iter()
            .map(|layer| (&layer.source, layer.keep_kind))
            .collect();
        let remove_kinds: Vec<_> = layers
            .iter()
            .map(|layer| (&layer.source, layer.remove_kind))
            .collect();
        let keep_patterns: Vec<_> = layers
            .iter()
            .map(|layer| (&layer.source, layer.keep_regex))
            .collect();
        let remove_patterns: Vec<_> = layers
            .iter()
            .map(|layer| (&layer.source, layer.remove_regex))
            .collect();
        let mut trace = PolicyTrace {
            policy: scalar_source(cli.policy, "--policy"),
            layout: scalar_source(cli.layout, "--layout"),
            keep_kind: attribute(
                &self.config.policy.keep_kind,
                cli.keep_kind_from,
                "--keep-kind",
                &keep_kinds,
            ),
            remove_kind: attribute(
                &self.config.policy.remove_kind,
                cli.remove_kind_from,
                "--remove-kind",
                &remove_kinds,
            ),
            // No flag supplies a pattern, so no entry of either list can have
            // come from the command line.
            keep_regex: attribute(&self.config.policy.keep_regex, None, "", &keep_patterns),
            remove_regex: attribute(&self.config.policy.remove_regex, None, "", &remove_patterns),
            origins: self.origins.clone(),
        };
        // A single-valued setting is not merged but replaced, so the last layer
        // that names it is the one that decided it.
        for layer in &layers {
            if layer.policy.is_some() {
                trace.policy = layer.source.clone();
            }
            if layer.layout.is_some() {
                trace.layout = layer.source.clone();
            }
        }
        (chosen_language, options, trace)
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
    let mut origins = PolicyOrigins::new();
    if let Some(path) = &user_path {
        merge_layer(
            &mut merged,
            &mut origins,
            parse_layer(path, false)?,
            path,
            &cwd,
        );
        trace.user = Some(path.clone());
    }
    if let Some(path) = &project_path {
        merge_layer(
            &mut merged,
            &mut origins,
            parse_layer(path, true)?,
            path,
            &cwd,
        );
        trace.project = Some(path.clone());
    }
    if let Some(path) = explicit {
        merge_layer(
            &mut merged,
            &mut origins,
            parse_layer(path, true)?,
            path,
            &cwd,
        );
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
        cwd,
        cli_overrides: CliOverrides::default(),
        overrides,
        origins,
    })
}

/// Layer one configuration file over the merged document, noting every
/// `[policy]` key it sets on the way.
///
/// A later layer overwrites an earlier one exactly as `merge_value` does, so
/// what is left is the file whose value survived the merge — the one an
/// explanation is worth sending a reader to.
fn merge_layer(
    merged: &mut toml::Value,
    origins: &mut PolicyOrigins,
    layer: toml::Value,
    path: &Path,
    cwd: &Path,
) {
    if let Some(policy) = layer.get("policy") {
        for key in POLICY_KEYS {
            if policy.get(key).is_some() {
                origins.insert(key, origin_label(path, cwd));
            }
        }
    }
    merge_value(merged, layer);
}

/// How an explanation names a configuration file: relative to the directory
/// the command was run from when it sits there, and absolute otherwise.
///
/// The label is repeated on every explained line, so the short spelling is
/// worth having — but only where it still names the file the reader would open.
/// A file further up the tree, or the user file under `$HOME`, keeps its
/// absolute path.
fn origin_label(path: &Path, cwd: &Path) -> PathBuf {
    path.strip_prefix(cwd).unwrap_or(path).to_path_buf()
}

fn validate_languages(config: &Config) -> Result<()> {
    for (name, language_config) in &config.languages {
        let language: Language = name.parse().map_err(|_| {
            anyhow!("unknown language configuration key `{name}`; see `ocomment languages`")
        })?;
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
        // The path is repeated deliberately: the first half is the verdict on
        // a file the reader may not have opened, the second is the edit that
        // settles it, and an editor is opened on the second one.
        let path = path.display();
        bail!("{path} must contain `version = 1` (add `version = 1` at the top of {path})");
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

/// Resolve `.` and `..` without asking the file system.
///
/// A configuration glob is matched against text, so the text has to be the one
/// the reader would have written: `../sibling/main.rs`, named from `nested/`,
/// is `sibling/main.rs` under the root, and leaving the `..` in place would
/// let it match a `nested/**` override it is not under. The resolution is
/// lexical because the path need not exist and because `canonicalize` would
/// also resolve the symbolic links the root itself may be reached through,
/// which would leave the two ends of the comparison in different spellings.
fn lexical(path: &Path) -> PathBuf {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir
                if matches!(
                    resolved.components().next_back(),
                    Some(Component::Normal(_))
                ) =>
            {
                resolved.pop();
            }
            component => resolved.push(component),
        }
    }
    resolved
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

    /// `for_path_traced` must not become a second copy of the merge that can
    /// drift from it: the values it returns are `for_path`'s own, and the trace
    /// beside them lines up with those values position for position.
    #[test]
    fn a_traced_lookup_returns_the_untraced_answer_and_lines_up_with_it() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_path_buf();
        let mut config = Config::default();
        config.policy.keep_regex = vec!["global".to_owned()];
        config.policy.keep_kind = vec![CommentKind::Line];
        config.languages.insert(
            "rust".to_owned(),
            LanguageConfig {
                keep_regex: vec!["language".to_owned()],
                ..LanguageConfig::default()
            },
        );
        config.overrides = vec![PathOverride {
            paths: vec!["nested/**".to_owned()],
            policy: Some(Policy::All),
            // The duplicate is dropped by the merge, so the trace must not
            // record a source for it either.
            keep_regex: vec!["override".to_owned(), "global".to_owned()],
            ..PathOverride::default()
        }];
        let overrides = compile_overrides(&config.overrides).unwrap();
        let resolved = ResolvedConfig {
            config,
            trace: ConfigTrace::default(),
            root: root.clone(),
            cwd: root.clone(),
            cli_overrides: CliOverrides {
                keep_kind_from: Some(0),
                ..CliOverrides::default()
            },
            overrides,
            origins: PolicyOrigins::new(),
        };
        let path = root.join("nested/a.rs");

        let (language, options) = resolved.for_path(&path, Language::Rust, Dialect::Standard);
        let (traced_language, traced_options, trace) =
            resolved.for_path_traced(&path, Language::Rust, Dialect::Standard);
        assert_eq!(traced_language, language);
        assert_eq!(traced_options, options);

        let override_source = Source::Override {
            index: 0,
            paths: vec!["nested/**".to_owned()],
        };
        assert_eq!(options.scan.keep_regex, ["global", "language", "override"]);
        assert_eq!(
            trace.keep_regex,
            [
                Source::Global,
                Source::Language("rust".to_owned()),
                override_source.clone(),
            ]
        );
        assert_eq!(trace.policy, override_source);
        assert_eq!(
            trace.keep_kind,
            [Source::Cli {
                flag: "--keep-kind"
            }]
        );
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
