use crate::{config::PluginsConfig, output::wrote};
use anyhow::{Context, Result, anyhow, bail, ensure};
use ocomment_core::{
    ByteSpan, CommentKind, Language, TransformOptions, TransformResult, transform_spans,
};
use ocomment_plugin_sdk::{API_VERSION, PluginComment, validate_comments};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Condvar, Mutex},
};
use tempfile::NamedTempFile;
use wasm_component_layer::{
    Component, Engine as ComponentEngine, Linker, List, Record, Store, Value, ValueType,
};

const LOCK_NAME: &str = ".ocomment.lock";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LockFile {
    version: u32,
    plugins: BTreeMap<String, LockedPlugin>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LockedPlugin {
    source: String,
    artifact: String,
    version: String,
    sha256: String,
    signature_identity: Option<String>,
    api: u32,
    capabilities: Vec<String>,
}

struct AcquiredArtifact {
    path: PathBuf,
    _cleanup: Option<TemporaryPath>,
}

struct TemporaryPath(PathBuf);

impl TemporaryPath {
    fn new(directory: &Path, suffix: &str) -> Result<Self> {
        let file = tempfile::Builder::new()
            .prefix(".ocomment-download-")
            .suffix(suffix)
            .tempfile_in(directory)?;
        let path = file.path().to_path_buf();
        file.close()?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Offline, prevalidated set of configured scanner components.
///
/// The engine is only constructed when at least one plugin is enabled. Each
/// scan gets a fresh store so fuel and memory accounting cannot leak between
/// files, while parsed components are shared across worker threads.
pub struct PluginHost {
    runtime: Option<PluginRuntime>,
}

struct PluginRuntime {
    engine: ComponentEngine<wasmi_runtime_layer::Engine>,
    components: BTreeMap<String, Component>,
    memory_bytes: usize,
    max_instances: usize,
    fuel_per_byte: u64,
    instance_gate: InstanceGate,
}

struct InstanceGate {
    active: Mutex<usize>,
    available: Condvar,
    limit: usize,
}

struct InstancePermit<'a>(&'a InstanceGate);

impl InstanceGate {
    fn new(limit: usize) -> Self {
        Self {
            active: Mutex::new(0),
            available: Condvar::new(),
            limit,
        }
    }

    fn enter(&self) -> Result<InstancePermit<'_>> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow!("plugin instance limiter is poisoned"))?;
        while *active >= self.limit {
            active = self
                .available
                .wait(active)
                .map_err(|_| anyhow!("plugin instance limiter is poisoned"))?;
        }
        *active += 1;
        Ok(InstancePermit(self))
    }
}

impl Drop for InstancePermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.0.active.lock() {
            *active = active.saturating_sub(1);
            self.0.available.notify_one();
        }
    }
}

impl PluginHost {
    pub fn load(root: &Path, config: &PluginsConfig) -> Result<Self> {
        validate_routes(config)?;
        if config.enabled.is_empty() {
            return Ok(Self { runtime: None });
        }

        let memory_mib = config.memory_mib.unwrap_or(64);
        let max_instances = config.instances.unwrap_or(4);
        let fuel_per_byte = config.fuel_per_byte.unwrap_or(128);
        ensure!(
            (1..=4096).contains(&memory_mib),
            "plugins.memory_mib must be between 1 and 4096"
        );
        ensure!(
            (1..=1024).contains(&max_instances),
            "plugins.instances must be between 1 and 1024"
        );
        ensure!(
            (1..=1_000_000).contains(&fuel_per_byte),
            "plugins.fuel_per_byte must be between 1 and 1000000"
        );
        let memory_bytes = usize::try_from(memory_mib)
            .ok()
            .and_then(|value| value.checked_mul(1024 * 1024))
            .context("plugin memory limit does not fit this platform")?;

        let lock = load_lock(root)?;
        let engine = component_engine();
        let mut components = BTreeMap::new();
        for name in &config.enabled {
            if components.contains_key(name) {
                bail!("plugin `{name}` is enabled more than once");
            }
            let locked = lock.plugins.get(name).with_context(|| {
                format!("enabled plugin `{name}` is not present in {LOCK_NAME}")
            })?;
            validate_locked_metadata(name, locked)?;
            let path = locked_artifact_path(root, &locked.artifact)?;
            let bytes = fs::read(&path).with_context(|| {
                format!("plugin `{name}` artifact is missing: {}", path.display())
            })?;
            ensure!(
                bytes.len() <= memory_bytes,
                "plugin `{name}` artifact exceeds its configured memory budget"
            );
            let actual = hex_sha256(&bytes);
            ensure!(actual == locked.sha256, "plugin `{name}` digest mismatch");
            let component = parse_component(&engine, &bytes)
                .with_context(|| format!("invalid scanner component for plugin `{name}`"))?;
            components.insert(name.clone(), component);
        }

        Ok(Self {
            runtime: Some(PluginRuntime {
                engine,
                components,
                memory_bytes,
                max_instances: max_instances as usize,
                fuel_per_byte,
                instance_gate: InstanceGate::new(max_instances as usize),
            }),
        })
    }

    pub fn transform(
        &self,
        name: &str,
        source: &[u8],
        language: &str,
        path: &Path,
        options: TransformOptions,
    ) -> Result<TransformResult> {
        let runtime = self
            .runtime
            .as_ref()
            .context("a file is routed to a plugin, but no plugins are enabled")?;
        let component = runtime
            .components
            .get(name)
            .with_context(|| format!("routed plugin `{name}` is not enabled"))?;
        let _permit = runtime.instance_gate.enter()?;

        let metered_bytes = u64::try_from(source.len())
            .unwrap_or(u64::MAX)
            .saturating_add(4096);
        let fuel = runtime.fuel_per_byte.saturating_mul(metered_bytes);
        let mut store = Store::new(&runtime.engine, ());
        store
            .configure_wasmi_resources(runtime.memory_bytes, runtime.max_instances, fuel)
            .with_context(|| format!("cannot configure resource limits for plugin `{name}`"))?;
        let instance = Linker::default()
            .instantiate(&mut store, component)
            .map_err(|error| execution_error(name, error))?;
        let exports = instance.exports().root();
        let api = exports
            .func("api-version")
            .context("scanner component does not export `api-version`")?
            .typed::<(), u32>()
            .context("scanner component has an invalid `api-version` signature")?
            .call(&mut store, ())
            .map_err(|error| execution_error(name, error))?;
        ensure!(
            api == API_VERSION,
            "plugin `{name}` reports API {api}, host supports {API_VERSION}"
        );

        let scan = exports
            .func("scan")
            .context("scanner component does not export `scan`")?;
        let scan_type = scan.ty();
        let option_type = match scan_type.params() {
            [
                ValueType::List(source_type),
                ValueType::Record(options_type),
            ] if source_type.element_ty() == ValueType::U8 => options_type.clone(),
            _ => bail!("scanner component has an invalid `scan` parameter signature"),
        };
        ensure!(
            matches!(scan_type.results(), [ValueType::Result(_)]),
            "scanner component has an invalid `scan` result signature"
        );

        let configuration = serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "path": path.to_string_lossy(),
            "transform": options,
        }))?;
        let dialect_value = serde_json::to_value(options.scan.dialect)?;
        let dialect = dialect_value
            .as_str()
            .context("dialect did not serialize as a string")?;
        let option_value = Record::new(
            option_type,
            [
                ("language", Value::String(language.into())),
                ("dialect", Value::String(dialect.into())),
                (
                    "configuration",
                    Value::List(List::from(configuration.as_slice())),
                ),
            ],
        )
        .context("scanner component's scan-options type does not match API 1")?;
        let arguments = [Value::List(List::from(source)), Value::Record(option_value)];
        let mut results = [Value::Bool(false)];
        scan.call(&mut store, &arguments, &mut results)
            .map_err(|error| execution_error(name, error))?;
        let comments = parse_scan_result(name, &results[0])?;
        validate_comments(source.len(), api, &comments)
            .with_context(|| format!("plugin `{name}` returned unsafe comment spans"))?;
        let spans: Vec<_> = comments
            .into_iter()
            .map(|comment| (comment.span, comment.kind))
            .collect();
        transform_spans(source, Language::Unknown, &spans, options)
            .with_context(|| format!("plugin `{name}` spans failed host validation"))
    }
}

fn validate_routes(config: &PluginsConfig) -> Result<()> {
    for (extension, plugin) in &config.routes {
        ensure!(
            !extension.is_empty()
                && !extension.starts_with('.')
                && extension == &extension.to_ascii_lowercase()
                && extension
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric()
                        || matches!(character, '-' | '_')),
            "plugin route `{extension}` must be a lowercase extension without a leading dot"
        );
        ensure!(
            config.enabled.iter().any(|enabled| enabled == plugin),
            "plugin route `{extension}` refers to disabled plugin `{plugin}`"
        );
    }
    Ok(())
}

fn component_engine() -> ComponentEngine<wasmi_runtime_layer::Engine> {
    let mut config = wasmi::Config::default();
    config.consume_fuel(true);
    let engine = wasmi::Engine::new(&config);
    ComponentEngine::new(wasmi_runtime_layer::Engine::new(engine))
}

fn parse_component(
    engine: &ComponentEngine<wasmi_runtime_layer::Engine>,
    bytes: &[u8],
) -> Result<Component> {
    validate_wasm_header(bytes)?;
    let component = Component::new(engine, bytes)?;
    let imports = component.imports();
    ensure!(
        imports.root().funcs().next().is_none()
            && imports.root().resources().next().is_none()
            && imports.instances().next().is_none(),
        "scanner components may not import functions, resources, WASI, or host capabilities"
    );
    let api = component
        .exports()
        .root()
        .func("api-version")
        .context("component does not export `api-version`")?;
    ensure!(
        api.params().is_empty() && api.results() == [ValueType::U32],
        "`api-version` must have type `func() -> u32`"
    );
    let scan = component
        .exports()
        .root()
        .func("scan")
        .context("component does not export `scan`")?;
    ensure!(scan.params().len() == 2, "`scan` must have two parameters");
    ensure!(
        matches!(scan.params().first(), Some(ValueType::List(ty)) if ty.element_ty() == ValueType::U8),
        "`scan` source must be `list<u8>`"
    );
    ensure!(
        matches!(scan.params().get(1), Some(ValueType::Record(_)))
            && matches!(scan.results(), [ValueType::Result(_)]),
        "`scan` does not match the API 1 component signature"
    );
    Ok(component)
}

fn parse_scan_result(name: &str, value: &Value) -> Result<Vec<PluginComment>> {
    let Value::Result(result) = value else {
        bail!("plugin `{name}` returned a non-result value from `scan`");
    };
    let list = match &**result {
        std::result::Result::Ok(Some(Value::List(list))) => list,
        std::result::Result::Ok(_) => {
            bail!("plugin `{name}` returned an invalid successful scan payload")
        }
        std::result::Result::Err(Some(Value::Variant(error))) => {
            let kind = error
                .ty()
                .cases()
                .get(error.discriminant())
                .map(|case| case.name().to_owned())
                .unwrap_or_else(|| "unknown-error".into());
            let message = match error.value() {
                Some(Value::String(message)) => message.to_string(),
                _ => "no error detail".into(),
            };
            bail!("plugin `{name}` returned {kind}: {message}")
        }
        std::result::Result::Err(_) => {
            bail!("plugin `{name}` returned an invalid scan error payload")
        }
    };

    list.iter().map(parse_comment).collect()
}

fn parse_comment(value: Value) -> Result<PluginComment> {
    let Value::Record(comment) = value else {
        bail!("plugin comment is not a record");
    };
    let Value::Record(location) = comment
        .field("location")
        .context("plugin comment has no `location`")?
    else {
        bail!("plugin comment location is not a record");
    };
    let start = match location.field("start") {
        Some(Value::U64(value)) => usize::try_from(value)
            .map_err(|_| anyhow!("plugin comment start does not fit this platform"))?,
        _ => bail!("plugin comment start is not u64"),
    };
    let end = match location.field("end") {
        Some(Value::U64(value)) => usize::try_from(value)
            .map_err(|_| anyhow!("plugin comment end does not fit this platform"))?,
        _ => bail!("plugin comment end is not u64"),
    };
    let Value::Enum(kind) = comment
        .field("kind")
        .context("plugin comment has no `kind`")?
    else {
        bail!("plugin comment kind is not an enum");
    };
    let kind_type = kind.ty();
    let kind = kind_type
        .cases()
        .nth(kind.discriminant())
        .context("plugin comment kind is outside its enum")?;
    Ok(PluginComment {
        span: ByteSpan::new(start, end),
        kind: parse_comment_kind(kind)?,
    })
}

fn parse_comment_kind(value: &str) -> Result<CommentKind> {
    Ok(match value {
        "line" => CommentKind::Line,
        "block" => CommentKind::Block,
        "doc-line" => CommentKind::DocLine,
        "doc-block" => CommentKind::DocBlock,
        "directive" => CommentKind::Directive,
        "license" => CommentKind::License,
        "html-comment" => CommentKind::HtmlComment,
        "shebang" => CommentKind::Shebang,
        "encoding" => CommentKind::Encoding,
        "optimizer-hint" => CommentKind::OptimizerHint,
        "version-comment" => CommentKind::VersionComment,
        other => bail!("plugin returned unknown comment kind `{other}`"),
    })
}

fn execution_error(name: &str, error: anyhow::Error) -> anyhow::Error {
    let detail = error.to_string();
    if detail.to_ascii_lowercase().contains("fuel") {
        anyhow!("plugin `{name}` exhausted its input-sized fuel budget")
    } else if detail.to_ascii_lowercase().contains("memory")
        || detail.to_ascii_lowercase().contains("instance")
        || detail.to_ascii_lowercase().contains("table")
    {
        anyhow!("plugin `{name}` exceeded a configured resource limit: {detail}")
    } else {
        anyhow!("plugin `{name}` execution failed: {detail}")
    }
}

fn validate_locked_metadata(name: &str, plugin: &LockedPlugin) -> Result<()> {
    ensure!(
        plugin.api == API_VERSION,
        "plugin `{name}` uses unsupported API {}",
        plugin.api
    );
    ensure!(
        plugin.capabilities == ["scan"],
        "plugin `{name}` must be locked with only the `scan` capability"
    );
    ensure!(
        plugin.sha256.len() == 64
            && plugin
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "plugin `{name}` has an invalid SHA-256 digest"
    );
    if is_remote_source(&plugin.source) {
        ensure!(
            plugin
                .signature_identity
                .as_deref()
                .is_some_and(|identity| !identity.trim().is_empty()),
            "remote plugin `{name}` has no Sigstore identity"
        );
    }
    Ok(())
}

fn locked_artifact_path(root: &Path, artifact: &str) -> Result<PathBuf> {
    let candidate = locked_artifact_candidate(root, artifact)?;
    let plugin_root = fs::canonicalize(root.join(".ocomment/plugins")).with_context(|| {
        format!(
            "cannot canonicalize plugin directory below {}",
            root.display()
        )
    })?;
    let path = fs::canonicalize(&candidate)
        .with_context(|| format!("plugin artifact is missing: {artifact}"))?;
    ensure!(
        path.starts_with(&plugin_root),
        "plugin artifact resolves outside .ocomment/plugins"
    );
    ensure!(path.is_file(), "plugin artifact is not a regular file");
    Ok(path)
}

fn locked_artifact_candidate(root: &Path, artifact: &str) -> Result<PathBuf> {
    let relative = Path::new(artifact);
    let plugin_directory = Path::new(".ocomment").join("plugins");
    ensure!(
        !relative.is_absolute()
            && relative.starts_with(&plugin_directory)
            && relative != plugin_directory
            && relative.components().all(|component| matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )),
        "plugin artifact path must remain inside .ocomment/plugins"
    );
    Ok(root.join(relative))
}

pub fn add(
    output: &mut impl Write,
    root: &Path,
    source: &str,
    requested_name: Option<&str>,
    expected: Option<&str>,
    identity: Option<&str>,
) -> Result<()> {
    install(
        output,
        root,
        source,
        requested_name,
        expected,
        identity,
        true,
    )
}

fn install(
    output: &mut impl Write,
    root: &Path,
    source: &str,
    requested_name: Option<&str>,
    expected: Option<&str>,
    identity: Option<&str>,
    require_remote_digest: bool,
) -> Result<()> {
    let remote = is_remote_source(source);
    if remote && ((require_remote_digest && expected.is_none()) || identity.is_none()) {
        bail!("remote plugins require both --sha256 and --identity");
    }
    let directory = root.join(".ocomment/plugins");
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let downloaded = if remote {
        fetch_remote(source, &directory)?
    } else {
        AcquiredArtifact {
            path: PathBuf::from(source),
            _cleanup: None,
        }
    };
    let bytes = fs::read(&downloaded.path)
        .with_context(|| format!("cannot read plugin {}", downloaded.path.display()))?;
    validate_wasm(&bytes)?;
    let digest = hex_sha256(&bytes);
    if let Some(expected) = expected {
        let expected = expected.trim_start_matches("sha256:").to_ascii_lowercase();
        if digest != expected {
            bail!("plugin digest mismatch: expected {expected}, got {digest}");
        }
    }
    if remote {
        verify_sigstore(
            source,
            &downloaded.path,
            identity.expect("checked above"),
            &directory,
        )?;
    }
    let name = requested_name
        .map(str::to_owned)
        .or_else(|| plugin_name(source))
        .context("cannot derive plugin name; pass --name")?;
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        bail!("plugin name may contain only ASCII letters, digits, `-`, and `_`");
    }
    let artifact_name = format!("{name}-{digest}.wasm");
    let artifact = directory.join(&artifact_name);
    if !artifact.exists() {
        fs::write(&artifact, &bytes)?;
    }
    let mut lock = load_lock(root)?;
    lock.version = 1;
    lock.plugins.insert(
        name.clone(),
        LockedPlugin {
            source: source.into(),
            artifact: format!(".ocomment/plugins/{artifact_name}"),
            version: source_version(source),
            sha256: digest,
            signature_identity: identity.map(str::to_owned),
            api: API_VERSION,
            capabilities: vec!["scan".into()],
        },
    );
    save_lock(root, &lock)?;
    wrote(writeln!(output, "added plugin {name}"))?;
    Ok(())
}

pub fn remove(output: &mut impl Write, root: &Path, name: &str) -> Result<()> {
    let mut lock = load_lock(root)?;
    let removed = lock
        .plugins
        .remove(name)
        .with_context(|| format!("plugin `{name}` is not locked"))?;
    let shared = lock
        .plugins
        .values()
        .any(|plugin| plugin.artifact == removed.artifact);
    if !shared {
        let candidate = locked_artifact_candidate(root, &removed.artifact)?;
        match fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => fs::remove_file(&candidate)?,
            Ok(_) => fs::remove_file(locked_artifact_path(root, &removed.artifact)?)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    save_lock(root, &lock)?;
    wrote(writeln!(output, "removed plugin {name}"))?;
    Ok(())
}

pub fn list(output: &mut impl Write, root: &Path) -> Result<()> {
    let lock = load_lock(root)?;
    if lock.plugins.is_empty() {
        wrote(writeln!(output, "no plugins locked"))?;
    }
    for (name, plugin) in lock.plugins {
        wrote(writeln!(
            output,
            "{name}\t{}\tsha256:{}\tAPI {}",
            plugin.version, plugin.sha256, plugin.api
        ))?;
    }
    Ok(())
}

pub fn verify(output: &mut impl Write, root: &Path, selected: Option<&str>) -> Result<()> {
    let lock = load_lock(root)?;
    for (name, plugin) in lock
        .plugins
        .iter()
        .filter(|(name, _)| selected.is_none_or(|selected| selected == name.as_str()))
    {
        validate_locked_metadata(name, plugin)?;
        let artifact = locked_artifact_path(root, &plugin.artifact)?;
        let bytes =
            fs::read(artifact).with_context(|| format!("plugin `{name}` artifact is missing"))?;
        validate_wasm(&bytes)?;
        let actual = hex_sha256(&bytes);
        if actual != plugin.sha256 {
            bail!("plugin `{name}` digest mismatch");
        }
        wrote(writeln!(
            output,
            "plugin {name}: verified sha256:{}",
            plugin.sha256
        ))?;
    }
    if let Some(name) = selected
        && !lock.plugins.contains_key(name)
    {
        bail!("plugin `{name}` is not locked");
    }
    if lock.plugins.is_empty() {
        wrote(writeln!(output, "plugins: none (offline lock is valid)"))?;
    }
    Ok(())
}

pub fn update(output: &mut impl Write, root: &Path, selected: Option<&str>) -> Result<()> {
    let lock = load_lock(root)?;
    let entries: Vec<_> = lock
        .plugins
        .iter()
        .filter(|(name, _)| selected.is_none_or(|selected| selected == name.as_str()))
        .map(|(name, plugin)| (name.clone(), plugin.clone()))
        .collect();
    if entries.is_empty() {
        bail!("no matching plugin to update");
    }
    for (name, plugin) in entries {
        if !is_remote_source(&plugin.source) {
            add(output, root, &plugin.source, Some(&name), None, None)?;
        } else {
            // The existing signature identity authorizes a freshly fetched
            // artifact. Its new digest is then written to the lockfile.
            install(
                output,
                root,
                &plugin.source,
                Some(&name),
                None,
                plugin.signature_identity.as_deref(),
                false,
            )?;
        }
    }
    Ok(())
}

fn is_remote_source(source: &str) -> bool {
    source.starts_with("https://") || source.starts_with("gh:") || source.starts_with("oci:")
}

pub fn new_plugin(output: &mut impl Write, path: &Path) -> Result<()> {
    fs::create_dir(path)
        .with_context(|| format!("refusing to overwrite plugin directory {}", path.display()))?;
    fs::create_dir(path.join("src"))?;
    fs::write(
        path.join("ocomment-scanner.wit"),
        include_str!("../assets/ocomment-scanner.wit"),
    )?;
    fs::write(
        path.join("Cargo.toml"),
        r#"[package]
name = "ocomment-scanner-plugin"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.57.1"
"#,
    )?;
    fs::write(
        path.join("src/lib.rs"),
        r#"wit_bindgen::generate!({
    path: "ocomment-scanner.wit",
    world: "scanner",
});

use ocomment::scanner::types::{CommentKind, Span};

struct Plugin;

impl Guest for Plugin {
    fn api_version() -> u32 {
        1
    }

    fn scan(source: Vec<u8>, _options: ScanOptions) -> Result<Vec<Comment>, ScanError> {
        // Minimal example: replace this with a byte-oriented scanner for your language.
        let mut comments = Vec::new();
        let mut index = 0;
        while index + 1 < source.len() {
            if &source[index..index + 2] == b"//" {
                let mut end = index + 2;
                while end < source.len() && !matches!(source[end], b'\r' | b'\n') {
                    end += 1;
                }
                comments.push(Comment {
                    location: Span { start: index as u64, end: end as u64 },
                    kind: CommentKind::Line,
                });
                index = end;
            } else {
                index += 1;
            }
        }
        Ok(comments)
    }
}

export!(Plugin);
"#,
    )?;
    fs::write(
        path.join("README.md"),
        r#"# OComment scanner plugin

This crate implements the versioned `ocomment:scanner@1.0.0` component world.
The generated scanner is intentionally tiny; replace its byte loop with the lexical rules for
your language. Plugins return spans only. OComment validates them and owns every edit.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-tools
cargo build --release --target wasm32-unknown-unknown
wasm-tools component new \
  target/wasm32-unknown-unknown/release/ocomment_scanner_plugin.wasm \
  -o scanner.component.wasm
ocomment plugin add scanner.component.wasm --name example
```

The host provides no WASI, filesystem, network, clock, or random imports. Keep the component
self-contained and return sorted, non-overlapping, non-empty byte spans.
"#,
    )?;
    wrote(writeln!(
        output,
        "created plugin scaffold {}",
        path.display()
    ))?;
    Ok(())
}

fn fetch_remote(source: &str, directory: &Path) -> Result<AcquiredArtifact> {
    let temporary = TemporaryPath::new(directory, ".wasm")?;
    if source.starts_with("https://") {
        let status = Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(temporary.path())
            .arg(source)
            .status()
            .context("cannot launch HTTPS plugin retrieval")?;
        ensure!(status.success(), "plugin retrieval failed with {status}");
    } else if let Some(spec) = source.strip_prefix("gh:") {
        let (repository, tag, asset) = parse_github_source(spec)?;
        let status = Command::new("gh")
            .args([
                "release",
                "download",
                tag,
                "--repo",
                repository,
                "--pattern",
                asset,
                "--output",
            ])
            .arg(temporary.path())
            .status()
            .context("cannot launch GitHub plugin retrieval")?;
        ensure!(status.success(), "plugin retrieval failed with {status}");
    } else {
        let specification = source.strip_prefix("oci:").expect("remote kind checked");
        let (reference, artifact_path) = specification
            .split_once('#')
            .map_or((specification, None), |(reference, path)| {
                (reference, Some(path))
            });
        ensure!(!reference.is_empty(), "OCI plugin reference is empty");
        let pulled = tempfile::tempdir_in(directory)?;
        let status = Command::new("oras")
            .args(["pull", reference, "--output"])
            .arg(pulled.path())
            .status()
            .context("cannot launch OCI plugin retrieval")?;
        ensure!(status.success(), "plugin retrieval failed with {status}");
        let artifact = if let Some(relative) = artifact_path {
            let relative = Path::new(relative);
            ensure!(
                !relative.is_absolute()
                    && relative.components().all(|component| matches!(
                        component,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    )),
                "OCI artifact path must remain inside the pulled artifact"
            );
            pulled.path().join(relative)
        } else {
            let mut candidates = Vec::new();
            collect_wasm_files(pulled.path(), &mut candidates)?;
            ensure!(
                candidates.len() == 1,
                "OCI artifact must contain exactly one .wasm component, or use `oci:reference#path`"
            );
            candidates.remove(0)
        };
        let pulled_root = fs::canonicalize(pulled.path())?;
        let artifact = fs::canonicalize(&artifact).with_context(|| {
            format!("cannot resolve OCI plugin component {}", artifact.display())
        })?;
        ensure!(
            artifact.starts_with(&pulled_root) && artifact.is_file(),
            "OCI plugin component resolves outside the pulled artifact"
        );
        fs::copy(&artifact, temporary.path()).with_context(|| {
            format!("cannot extract OCI plugin component {}", artifact.display())
        })?;
    }
    Ok(AcquiredArtifact {
        path: temporary.path().to_path_buf(),
        _cleanup: Some(temporary),
    })
}

fn verify_sigstore(source: &str, artifact: &Path, identity: &str, directory: &Path) -> Result<()> {
    if let Some(specification) = source.strip_prefix("oci:") {
        let reference = specification.split('#').next().unwrap_or(specification);
        let status = Command::new("cosign")
            .args([
                "verify",
                "--certificate-identity",
                identity,
                "--certificate-oidc-issuer",
                "https://token.actions.githubusercontent.com",
            ])
            .arg(reference)
            .status()
            .context("cannot launch cosign")?;
        ensure!(status.success(), "Sigstore verification failed");
        return Ok(());
    }
    let bundle = TemporaryPath::new(directory, ".sigstore.json")?;
    let download = if let Some(spec) = source.strip_prefix("gh:") {
        let (repository, tag, asset) = parse_github_source(spec)?;
        Command::new("gh")
            .args([
                "release",
                "download",
                tag,
                "--repo",
                repository,
                "--pattern",
                &format!("{asset}.sigstore.json"),
                "--output",
            ])
            .arg(bundle.path())
            .status()
            .context("cannot retrieve GitHub Sigstore bundle")?
    } else {
        let bundle_url = format!("{source}.sigstore.json");
        Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(bundle.path())
            .arg(&bundle_url)
            .status()
            .with_context(|| format!("cannot retrieve Sigstore bundle {bundle_url}"))?
    };
    ensure!(download.success(), "cannot retrieve Sigstore bundle");
    let status = Command::new("cosign")
        .args(["verify-blob", "--bundle"])
        .arg(bundle.path())
        .args([
            "--certificate-identity",
            identity,
            "--certificate-oidc-issuer",
            "https://token.actions.githubusercontent.com",
        ])
        .arg(artifact)
        .status()
        .context("cannot launch cosign")?;
    if !status.success() {
        bail!("Sigstore verification failed");
    }
    Ok(())
}

fn parse_github_source(specification: &str) -> Result<(&str, &str, &str)> {
    let (repository_tag, asset) = specification
        .split_once('#')
        .context("GitHub shorthand must be `gh:owner/repo@tag#asset.wasm`")?;
    let (repository, tag) = repository_tag
        .rsplit_once('@')
        .context("GitHub shorthand needs an explicit tag")?;
    ensure!(
        !repository.is_empty() && !tag.is_empty() && !asset.is_empty(),
        "GitHub plugin source contains an empty repository, tag, or asset"
    );
    Ok((repository, tag, asset))
}

fn collect_wasm_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_wasm_files(&path, output)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("wasm"))
        {
            output.push(path);
        }
    }
    output.sort();
    Ok(())
}

fn validate_wasm(bytes: &[u8]) -> Result<()> {
    let engine = component_engine();
    let component = parse_component(&engine, bytes)?;
    let mut store = Store::new(&engine, ());
    store.configure_wasmi_resources(64 * 1024 * 1024, 4, 1_000_000)?;
    let instance = Linker::default()
        .instantiate(&mut store, &component)
        .context("cannot instantiate scanner component")?;
    let api = instance
        .exports()
        .root()
        .func("api-version")
        .context("scanner component does not export `api-version`")?
        .typed::<(), u32>()?
        .call(&mut store, ())?;
    ensure!(
        api == API_VERSION,
        "scanner component reports API {api}, host supports {API_VERSION}"
    );
    Ok(())
}

fn validate_wasm_header(bytes: &[u8]) -> Result<()> {
    if bytes.len() < 8 || &bytes[..4] != b"\0asm" {
        bail!("plugin is not a WebAssembly binary/component");
    }
    Ok(())
}

fn load_lock(root: &Path) -> Result<LockFile> {
    let path = root.join(LOCK_NAME);
    if !path.exists() {
        return Ok(LockFile {
            version: 1,
            ..Default::default()
        });
    }
    let text = fs::read_to_string(&path)?;
    let lock: LockFile =
        toml::from_str(&text).with_context(|| format!("invalid lockfile {}", path.display()))?;
    if lock.version != 1 {
        bail!("unsupported lockfile version {}", lock.version);
    }
    Ok(lock)
}

fn save_lock(root: &Path, lock: &LockFile) -> Result<()> {
    let text = toml::to_string_pretty(lock)?;
    let mut file = NamedTempFile::new_in(root)?;
    file.write_all(text.as_bytes())?;
    file.as_file_mut().sync_all()?;
    file.persist(root.join(LOCK_NAME))
        .map_err(|error| error.error)?;
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn plugin_name(source: &str) -> Option<String> {
    let stripped = source
        .split('#')
        .next()
        .unwrap_or(source)
        .trim_end_matches('/');
    Path::new(stripped)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::to_owned)
}

fn source_version(source: &str) -> String {
    source.rsplit_once('@').map_or_else(
        || "local".into(),
        |(_, version)| version.split('#').next().unwrap_or(version).into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use wit_component::{ComponentEncoder, StringEncoding, embed_component_metadata};
    use wit_parser::{Resolve, UnresolvedPackage};

    #[test]
    fn locked_artifacts_are_confined_to_the_plugin_cache() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".ocomment/plugins")).unwrap();
        fs::write(root.path().join("outside.wasm"), b"not wasm").unwrap();
        assert!(locked_artifact_candidate(root.path(), "outside.wasm").is_err());
        assert!(locked_artifact_candidate(root.path(), "../outside.wasm").is_err());

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                root.path().join("outside.wasm"),
                root.path().join(".ocomment/plugins/link.wasm"),
            )
            .unwrap();
            assert!(locked_artifact_path(root.path(), ".ocomment/plugins/link.wasm").is_err());
        }
    }

    fn scanner_component(scan_body: &str, memory_pages: u32) -> Vec<u8> {
        let module_text = r#"(module
                (memory (export "memory") MEMORY_PAGES)
                (global $heap (mut i32) (i32.const 1024))
                (func (export "cabi_realloc")
                    (param $old i32) (param $old-size i32) (param $align i32) (param $size i32)
                    (result i32)
                    (local $result i32)
                    global.get $heap
                    local.tee $result
                    local.get $size
                    i32.add
                    global.set $heap
                    local.get $result)
                (func (export "api-version") (result i32)
                    i32.const 1)
                (func (export "scan")
                    (param i32 i32 i32 i32 i32 i32 i32 i32) (result i32)
                    SCAN_BODY)
                (func (export "cabi_post_scan") (param i32))
            )"#
        .replace("SCAN_BODY", scan_body)
        .replace("MEMORY_PAGES", &memory_pages.to_string());
        let mut module = wat::parse_str(module_text).unwrap();
        let mut resolve = Resolve::default();
        let package = resolve
            .push(
                UnresolvedPackage::parse(
                    Path::new("ocomment-scanner.wit"),
                    include_str!("../assets/ocomment-scanner.wit"),
                )
                .unwrap(),
            )
            .unwrap();
        let world = resolve.select_world(package, Some("scanner")).unwrap();
        embed_component_metadata(&mut module, &resolve, world, StringEncoding::UTF8).unwrap();
        ComponentEncoder::default()
            .module(&module)
            .unwrap()
            .validate(true)
            .encode()
            .unwrap()
    }

    fn empty_scanner_component() -> Vec<u8> {
        scanner_component(
            r#"i32.const 512
                i32.const 0
                i32.store8
                i32.const 516
                i32.const 0
                i32.store
                i32.const 520
                i32.const 0
                i32.store
                i32.const 512"#,
            1,
        )
    }

    fn one_comment_component() -> Vec<u8> {
        scanner_component(
            r#"i32.const 512
                i32.const 0
                i32.store8
                i32.const 516
                i32.const 600
                i32.store
                i32.const 520
                i32.const 1
                i32.store
                i32.const 600
                i64.const 1
                i64.store
                i32.const 608
                i64.const 3
                i64.store
                i32.const 616
                i32.const 0
                i32.store8
                i32.const 512"#,
            1,
        )
    }

    #[test]
    fn executes_a_component_with_wasmi_and_host_limits() {
        let bytes = empty_scanner_component();
        let engine = component_engine();
        let component = parse_component(&engine, &bytes).unwrap();
        let mut store = Store::new(&engine, ());
        store
            .configure_wasmi_resources(2 * 1024 * 1024, 4, 1_000_000)
            .unwrap();
        let instance = Linker::default()
            .instantiate(&mut store, &component)
            .unwrap();
        let api = instance
            .exports()
            .root()
            .func("api-version")
            .unwrap()
            .typed::<(), u32>()
            .unwrap()
            .call(&mut store, ())
            .unwrap();
        assert_eq!(api, API_VERSION);
        assert!(store.wasmi_fuel_consumed().is_some_and(|fuel| fuel > 0));

        let temporary = tempfile::tempdir().unwrap();
        let artifact_dir = temporary.path().join(".ocomment/plugins");
        fs::create_dir_all(&artifact_dir).unwrap();
        fs::write(artifact_dir.join("empty.wasm"), &bytes).unwrap();
        let mut lock = LockFile {
            version: 1,
            ..Default::default()
        };
        lock.plugins.insert(
            "empty".into(),
            LockedPlugin {
                source: "fixture".into(),
                artifact: ".ocomment/plugins/empty.wasm".into(),
                version: "1.0.0".into(),
                sha256: hex_sha256(&bytes),
                signature_identity: None,
                api: API_VERSION,
                capabilities: vec!["scan".into()],
            },
        );
        save_lock(temporary.path(), &lock).unwrap();
        let config = PluginsConfig {
            enabled: vec!["empty".into()],
            routes: BTreeMap::from([("fixture".into(), "empty".into())]),
            memory_mib: Some(2),
            instances: Some(4),
            fuel_per_byte: Some(128),
        };
        let host = PluginHost::load(temporary.path(), &config).unwrap();
        let source = b"opaque source";
        let result = host
            .transform(
                "empty",
                source,
                "fixture",
                Path::new("sample.fixture"),
                TransformOptions::default(),
            )
            .unwrap();
        assert_eq!(result.output, source);
        assert!(result.report.comments.is_empty());
    }

    #[test]
    fn rejects_core_modules_and_component_imports() {
        let module = wat::parse_str("(module)").unwrap();
        assert!(validate_wasm(&module).is_err());
        assert!(PluginHost::load(Path::new("."), &PluginsConfig::default()).is_ok());
    }

    #[test]
    fn component_spans_flow_through_the_core_transformer() {
        let bytes = one_comment_component();
        let temporary = tempfile::tempdir().unwrap();
        let artifact_dir = temporary.path().join(".ocomment/plugins");
        fs::create_dir_all(&artifact_dir).unwrap();
        fs::write(artifact_dir.join("one.wasm"), &bytes).unwrap();
        let lock = LockFile {
            version: 1,
            plugins: BTreeMap::from([(
                "one".into(),
                LockedPlugin {
                    source: "fixture".into(),
                    artifact: ".ocomment/plugins/one.wasm".into(),
                    version: "1.0.0".into(),
                    sha256: hex_sha256(&bytes),
                    signature_identity: None,
                    api: API_VERSION,
                    capabilities: vec!["scan".into()],
                },
            )]),
        };
        save_lock(temporary.path(), &lock).unwrap();
        let config = PluginsConfig {
            enabled: vec!["one".into()],
            routes: BTreeMap::from([("fixture".into(), "one".into())]),
            memory_mib: Some(2),
            instances: Some(4),
            fuel_per_byte: Some(128),
        };
        let host = PluginHost::load(temporary.path(), &config).unwrap();
        let result = host
            .transform(
                "one",
                b"a##b",
                "fixture",
                Path::new("sample.fixture"),
                TransformOptions::default(),
            )
            .unwrap();
        assert_eq!(result.output, b"a b");
        assert_eq!(result.report.comments[0].kind, CommentKind::Line);
        assert_eq!(result.report.comments[0].span, ByteSpan::new(1, 3));
        let error = host
            .transform(
                "one",
                b"x",
                "fixture",
                Path::new("sample.fixture"),
                TransformOptions::default(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("unsafe comment spans"));
    }

    fn host_for_component(bytes: &[u8], memory_mib: u32, fuel_per_byte: u64) -> PluginHost {
        let temporary = tempfile::tempdir().unwrap();
        let artifact_dir = temporary.path().join(".ocomment/plugins");
        fs::create_dir_all(&artifact_dir).unwrap();
        fs::write(artifact_dir.join("limit.wasm"), bytes).unwrap();
        let lock = LockFile {
            version: 1,
            plugins: BTreeMap::from([(
                "limit".into(),
                LockedPlugin {
                    source: "fixture".into(),
                    artifact: ".ocomment/plugins/limit.wasm".into(),
                    version: "1.0.0".into(),
                    sha256: hex_sha256(bytes),
                    signature_identity: None,
                    api: API_VERSION,
                    capabilities: vec!["scan".into()],
                },
            )]),
        };
        save_lock(temporary.path(), &lock).unwrap();
        PluginHost::load(
            temporary.path(),
            &PluginsConfig {
                enabled: vec!["limit".into()],
                routes: BTreeMap::from([("fixture".into(), "limit".into())]),
                memory_mib: Some(memory_mib),
                instances: Some(4),
                fuel_per_byte: Some(fuel_per_byte),
            },
        )
        .unwrap()
    }

    #[test]
    fn fuel_and_memory_limits_stop_hostile_components() {
        let looping = scanner_component(r#"(loop $forever (br $forever)) unreachable"#, 1);
        let host = host_for_component(&looping, 2, 1);
        let error = host
            .transform(
                "limit",
                b"x",
                "fixture",
                Path::new("x.fixture"),
                TransformOptions::default(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("fuel budget"));

        let oversized_memory = scanner_component(
            r#"i32.const 512
                i32.const 0
                i32.store8
                i32.const 516
                i32.const 0
                i32.store
                i32.const 520
                i32.const 0
                i32.store
                i32.const 512"#,
            64,
        );
        let host = host_for_component(&oversized_memory, 1, 128);
        let error = host
            .transform(
                "limit",
                b"x",
                "fixture",
                Path::new("x.fixture"),
                TransformOptions::default(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("resource limit"));
    }
}
