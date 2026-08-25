use crate::{
    atomic::{WritePlan, apply_transaction},
    config, files, git, lsp,
    output::{
        self, Operation, OutputFormat, Presentation, ProcessedFile, RenderOptions, Verbosity,
    },
    plugin,
    values::{CommentKindArg, DialectArg, LanguageArg, LayoutArg, PolicyArg},
};
use anyhow::{Context, Result};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use ocomment_core::{CommentKind, Dialect, Language, transform};
use rayon::prelude::*;
use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
};

const LONG_ABOUT: &str = "\
OComment scans source bytes without requiring UTF-8 and reports or removes \
comment tokens. The default policy protects source preambles and tool or \
language directives. Rewrites are prepared and committed as one \
rollback-backed transaction.";

const AFTER_LONG_HELP: &str = "\
EXIT STATUS
  0  Nothing removable was found and every requested change was applied.
  1  Removable comments were reported, or a diff was printed.
  2  Invalid source, configuration, plugin, or I/O failure.

FILES
  .ocomment.toml   Project configuration, merged over the user file.
  .ocommentignore  Extra ignore patterns honoured by repository walks.
  .ocomment.lock   Pinned digests of the installed WASM scanner plugins.
  $XDG_CONFIG_HOME/ocomment/config.toml
                   User configuration, merged over the built-in defaults.

EXAMPLES
  ocomment
      Check the current repository and report removable comments.
  ocomment fix --policy all --layout compact src
      Remove every comment under src and close the gaps it leaves.
  ocomment strip --language rust < before.rs > after.rs
      Strip one file from standard input to standard output.

SEE ALSO
  The complete schemas and guides are available in the OComment repository.";

#[derive(Parser)]
#[command(
    name = "ocomment",
    version,
    about = "Check and remove source-code comments safely"
)]
#[command(long_about = LONG_ABOUT)]
#[command(args_conflicts_with_subcommands = true)]
#[command(after_long_help = AFTER_LONG_HELP)]
struct Cli {
    /// Files or directories to check (default: current directory).
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,
    /// The command to run; `check` runs when none is given.
    #[command(subcommand)]
    command: Option<Command>,
    /// Configuration, policy, and output options shared by every command.
    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Clone, Debug, Args)]
struct CommonArgs {
    /// Read this configuration file instead of discovering `.ocomment.toml`.
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,
    /// What may be removed and how the source is interpreted.
    #[command(flatten)]
    policy: PolicyArgs,
    /// How results are encoded and decorated.
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Clone, Debug, Args)]
#[command(next_help_heading = "Policy")]
struct PolicyArgs {
    /// Which classes of comment the run is allowed to remove.
    #[arg(
        long,
        global = true,
        value_enum,
        ignore_case = true,
        value_name = "POLICY"
    )]
    policy: Option<PolicyArg>,
    /// How the bytes left behind by a removed comment are laid out.
    #[arg(
        long,
        global = true,
        value_enum,
        ignore_case = true,
        value_name = "LAYOUT"
    )]
    layout: Option<LayoutArg>,
    /// Force this language instead of detecting it from path and contents.
    #[arg(
        long,
        global = true,
        value_enum,
        ignore_case = true,
        value_name = "LANGUAGE"
    )]
    language: Option<LanguageArg>,
    /// Force this dialect of the selected language.
    #[arg(
        long,
        global = true,
        value_enum,
        ignore_case = true,
        value_name = "DIALECT"
    )]
    dialect: Option<DialectArg>,
    /// Comma-separated comment kinds to protect on top of the policy.
    #[arg(
        long = "keep-kind",
        global = true,
        value_enum,
        ignore_case = true,
        value_delimiter = ',',
        value_name = "KIND"
    )]
    keep_kind: Vec<CommentKindArg>,
    /// Comma-separated comment kinds to remove regardless of the policy.
    #[arg(
        long = "remove-kind",
        global = true,
        value_enum,
        ignore_case = true,
        value_delimiter = ',',
        value_name = "KIND"
    )]
    remove_kind: Vec<CommentKindArg>,
    /// Apply the edits that are still provably safe when the source fails to scan.
    #[arg(long, global = true)]
    force_invalid: bool,
    /// Remove protected comments such as shebang and encoding preambles.
    #[arg(long, global = true)]
    force_protected: bool,
}

#[derive(Clone, Debug, Args)]
#[command(next_help_heading = "Output")]
struct OutputArgs {
    /// Output encoding.
    #[arg(
        long,
        global = true,
        value_enum,
        default_value_t,
        value_name = "FORMAT"
    )]
    format: OutputFormat,
    /// When to colour terminal output.
    #[arg(long, global = true, value_enum, default_value_t, value_name = "WHEN")]
    color: ColorChoice,
    /// When to emit terminal hyperlinks for reported paths.
    #[arg(long, global = true, value_enum, default_value_t, value_name = "WHEN")]
    hyperlinks: AutoChoice,
    /// Omit the one-line comment text from human `check` and `scan` lines.
    #[arg(long, global = true)]
    no_preview: bool,
    /// Accepted for compatibility; the end-of-run summary replaced this line.
    // Nothing reads the value: the summary is written whether or not standard
    // error is a terminal.
    #[allow(dead_code)]
    #[arg(long, global = true, value_enum, default_value_t, value_name = "WHEN")]
    progress: AutoChoice,
    /// Print nothing but errors and diagnostics.
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    quiet: bool,
    /// Trace what is scanned and summarize every comment kind and skipped file.
    #[arg(short, long, global = true)]
    verbose: bool,
}

impl CommonArgs {
    /// The language forced on the command line, if any.
    fn language(&self) -> Option<Language> {
        self.policy.language.map(Language::from)
    }

    /// The dialect forced on the command line, if any.
    fn dialect(&self) -> Option<Dialect> {
        self.policy.dialect.map(Dialect::from)
    }

    /// How much of the human report this run may write.
    fn verbosity(&self) -> Verbosity {
        match (self.output.quiet, self.output.verbose) {
            (true, _) => Verbosity::Quiet,
            (_, true) => Verbosity::Verbose,
            _ => Verbosity::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum AutoChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Subcommand)]
enum Command {
    /// Report removable comments (default command)
    Check(TargetArgs),
    /// Remove comments in place through an atomic, rollback-backed transaction
    Fix(TargetArgs),
    /// Print a unified diff of the changes fix would make
    Diff(TargetArgs),
    /// List every comment with its kind, disposition and byte span
    Scan(TargetArgs),
    /// Read source on stdin and write the stripped result to stdout
    Strip,
    /// Run the LSP 3.18 server over stdio
    Lsp,
    /// Write a starter .ocomment.toml or Lefthook configuration
    Init(InitArgs),
    /// Show, locate, explain, or export the resolved configuration
    Config(ConfigArgs),
    /// List built-in languages, extensions, and dialects
    Languages,
    /// Manage sandboxed WASM scanner plugins
    Plugin(PluginArgs),
    /// Generate shell completions
    Completions {
        /// Shell whose completion script is written to stdout.
        shell: Shell,
    },
    /// Diagnose the environment (config, git, plugins, tools)
    Doctor,
    /// Render the roff manual page to stdout
    Man,
}

#[derive(Clone, Debug, Default, Args)]
struct TargetArgs {
    /// Files or directories to process (default: current directory).
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,
    /// Read and update Git index blobs rather than treating the working tree as the source.
    #[arg(long)]
    staged: bool,
    /// With `--staged`, do not attempt a uniquely mappable working-tree update.
    #[arg(long, requires = "staged")]
    index_only: bool,
}

#[derive(Args)]
struct InitArgs {
    /// Which starter file to write.
    #[arg(value_enum, default_value_t)]
    kind: InitKind,
    /// For the Lefthook hook, run `fix` instead of `check`.
    #[arg(long)]
    fix: bool,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum InitKind {
    #[default]
    Config,
    Lefthook,
}

#[derive(Args)]
struct ConfigArgs {
    /// Which view of the resolved configuration to print.
    #[arg(value_enum, default_value_t)]
    action: ConfigAction,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum ConfigAction {
    #[default]
    Show,
    Locate,
    Explain,
    Schema,
}

#[derive(Args)]
struct PluginArgs {
    /// The plugin operation to run.
    #[command(subcommand)]
    command: PluginCommand,
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Install a plugin and pin its digest in .ocomment.lock
    Add {
        /// Path or URL of the WASM component to install.
        source: String,
        /// Name to register the plugin under (default: the file stem).
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
        /// Expected SHA-256 digest of the component, verified before install.
        #[arg(long, value_name = "HEX")]
        sha256: Option<String>,
        /// Publisher identity recorded alongside the pinned digest.
        #[arg(long, value_name = "IDENTITY")]
        identity: Option<String>,
    },
    /// Uninstall a plugin and drop its lock entry
    Remove {
        /// Name of the plugin to remove.
        name: String,
    },
    /// List the installed plugins and their pinned digests
    List,
    /// Re-fetch plugins and refresh their pinned digests
    Update {
        /// Name of the plugin to update (default: all of them).
        name: Option<String>,
    },
    /// Check installed plugins against their pinned digests
    Verify {
        /// Name of the plugin to verify (default: all of them).
        name: Option<String>,
    },
    /// Scaffold a new plugin crate from the scanner WIT world
    New {
        /// Directory to create the plugin crate in.
        path: PathBuf,
    },
}

pub fn run() -> Result<u8> {
    let cli = Cli::parse();
    let common = cli.common;
    match cli.command {
        None => run_target(
            Operation::Check,
            TargetArgs {
                paths: cli.paths,
                ..Default::default()
            },
            &common,
        ),
        Some(Command::Check(args)) => run_target(Operation::Check, args, &common),
        Some(Command::Fix(args)) => run_target(Operation::Fix, args, &common),
        Some(Command::Diff(args)) => run_target(Operation::Diff, args, &common),
        Some(Command::Scan(args)) => run_target(Operation::Scan, args, &common),
        Some(Command::Strip) => run_strip(&common),
        Some(Command::Lsp) => lsp::run(common.config.as_deref()),
        Some(Command::Init(args)) => run_init(args),
        Some(Command::Config(args)) => run_config(args, &common),
        Some(Command::Languages) => {
            print_languages();
            Ok(0)
        }
        Some(Command::Plugin(args)) => run_plugin(args, &common),
        Some(Command::Completions { shell }) => {
            generate(shell, &mut Cli::command(), "ocomment", &mut io::stdout());
            Ok(0)
        }
        Some(Command::Doctor) => run_doctor(&common),
        Some(Command::Man) => run_man(),
    }
}

fn run_target(operation: Operation, args: TargetArgs, common: &CommonArgs) -> Result<u8> {
    let mut resolved = config::load(common.config.as_deref())?;
    apply_cli_overrides(&mut resolved.config, common);
    let plugin_host = plugin::PluginHost::load(&resolved.root, &resolved.config.plugins)?;
    let presentation = presentation(common);
    let verbosity = common.verbosity();
    if verbosity == Verbosity::Verbose {
        trace_run(&resolved, &args.paths);
    }
    let staged = args.staged || resolved.config.git.staged;
    if staged {
        return git::run_staged(git::StagedRequest {
            operation,
            paths: &args.paths,
            resolved: &resolved,
            format: common.output.format,
            index_only: args.index_only || resolved.config.git.index_only,
            plugin_host: &plugin_host,
            forced_language: common.language(),
            forced_dialect: common.dialect(),
            presentation,
            verbosity,
            preview: !common.output.no_preview,
        });
    }
    let discovery = files::discover(&args.paths, &resolved, common.language(), common.dialect())?;
    let files: Vec<_> = discovery
        .files
        .par_iter()
        .map(|file| {
            let (mut language, mut options) =
                resolved.for_path(&file.path, file.language, file.dialect);
            if let Some(value) = common.language() {
                language = value;
            }
            if let Some(value) = common.dialect() {
                config::validate_dialect(language, value)?;
                options.scan.dialect = value;
            }
            let result = if let Some(name) = &file.plugin {
                let language_name = file
                    .path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("unknown")
                    .to_ascii_lowercase();
                plugin_host.transform(name, &file.source, &language_name, &file.path, options)?
            } else if let Some(profile) = &file.profile {
                ocomment_core::transform_profile(&file.source, profile, options)
                    .expect("profiles were validated while loading configuration")
            } else {
                transform(&file.source, language, options)
            };
            Ok::<_, anyhow::Error>(ProcessedFile {
                path: file.path.clone(),
                source: file.source.clone(),
                language,
                result,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let report_invalid = output::invalid(&files);
    let io_invalid = discovery.skipped.iter().any(|item| item.error);
    let invalid = report_invalid || io_invalid;
    let may_fix = !io_invalid && (!report_invalid || resolved.config.policy.force_invalid);
    let applied = operation == Operation::Fix && may_fix;
    if applied {
        let plans = files
            .iter()
            .filter(|file| file.source != file.result.output)
            .map(|file| WritePlan {
                path: file.path.clone(),
                original: file.source.clone(),
                replacement: file.result.output.clone(),
            })
            .collect();
        apply_transaction(plans)?;
    }
    output::render(
        &files,
        &discovery.skipped,
        &RenderOptions {
            format: common.output.format,
            operation,
            presentation,
            verbosity,
            preview: !common.output.no_preview,
            explain: false,
            dry_run: false,
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

fn run_strip(common: &CommonArgs) -> Result<u8> {
    let mut source = Vec::new();
    io::stdin()
        .lock()
        .read_to_end(&mut source)
        .context("cannot read standard input")?;
    let mut resolved = config::load(common.config.as_deref())?;
    apply_cli_overrides(&mut resolved.config, common);
    let detection = common
        .language()
        .map(|language| (language, common.dialect().unwrap_or(Dialect::Standard)))
        .or_else(|| {
            ocomment_core::detect_language(None, &source)
                .map(|value| (value.language, value.dialect))
        })
        .context("cannot detect stdin language; pass --language")?;
    let (language, mut options) =
        resolved.for_path(std::path::Path::new("<stdin>"), detection.0, detection.1);
    if let Some(value) = common.dialect() {
        config::validate_dialect(language, value)?;
        options.scan.dialect = value;
    }
    let result = transform(&source, language, options);
    for diagnostic in &result.report.diagnostics {
        eprintln!(
            "stdin:{}..{}: {}: {}",
            diagnostic.span.start, diagnostic.span.end, diagnostic.code, diagnostic.message
        );
    }
    if !result.report.valid && !resolved.config.policy.force_invalid {
        return Ok(2);
    }
    io::stdout()
        .lock()
        .write_all(&result.output)
        .context("cannot write standard output")?;
    Ok(if result.report.valid { 0 } else { 2 })
}

fn apply_cli_overrides(config: &mut config::Config, common: &CommonArgs) {
    let policy = &common.policy;
    if let Some(value) = policy.policy {
        config.policy.mode = *value;
    }
    if let Some(value) = policy.layout {
        config.policy.layout = *value;
    }
    if !policy.keep_kind.is_empty() {
        config
            .policy
            .keep_kind
            .extend(policy.keep_kind.iter().copied().map(CommentKind::from));
    }
    if !policy.remove_kind.is_empty() {
        config
            .policy
            .remove_kind
            .extend(policy.remove_kind.iter().copied().map(CommentKind::from));
    }
    if policy.force_invalid {
        config.policy.force_invalid = true;
    }
    if policy.force_protected {
        config.policy.force_protected = true;
    }
}

/// Render the roff manual page from the parser definition itself.
fn run_man() -> Result<u8> {
    let mut page = Vec::new();
    clap_mangen::Man::new(Cli::command())
        .title("OCOMMENT")
        .manual("User Commands")
        .render(&mut page)
        .context("cannot render the manual page")?;
    io::stdout()
        .lock()
        .write_all(&page)
        .context("cannot write standard output")?;
    Ok(0)
}

fn run_init(args: InitArgs) -> Result<u8> {
    match args.kind {
        InitKind::Config => create_new(
            config::CONFIG_FILE,
            include_str!("../assets/default-config.toml"),
        )?,
        InitKind::Lefthook => {
            let command = if args.fix {
                "ocomment fix --staged"
            } else {
                "ocomment check --staged"
            };
            create_new(
                "lefthook.yml",
                &format!("pre-commit:\n  commands:\n    ocomment:\n      run: {command}\n"),
            )?;
        }
    }
    Ok(0)
}

fn create_new(path: &str, contents: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("refusing to overwrite {path}"))?;
    file.write_all(contents.as_bytes())?;
    println!("created {path}");
    Ok(())
}

fn run_config(args: ConfigArgs, common: &CommonArgs) -> Result<u8> {
    match args.action {
        ConfigAction::Schema => print!("{}", include_str!("../assets/config.schema.json")),
        action => {
            let mut resolved = config::load(common.config.as_deref())?;
            apply_cli_overrides(&mut resolved.config, common);
            match action {
                ConfigAction::Show => {
                    resolved.config.version = Some(1);
                    print!("{}", toml::to_string_pretty(&resolved.config)?);
                }
                ConfigAction::Locate => {
                    if let Some(path) = &resolved.trace.user {
                        println!("user\t{}", path.display());
                    }
                    if let Some(path) = &resolved.trace.project {
                        println!("project\t{}", path.display());
                    }
                    if let Some(path) = &resolved.trace.explicit {
                        println!("explicit\t{}", path.display());
                    }
                    if resolved.trace.user.is_none()
                        && resolved.trace.project.is_none()
                        && resolved.trace.explicit.is_none()
                    {
                        println!("built-in defaults");
                    }
                }
                ConfigAction::Explain => {
                    println!("precedence: built-in < XDG user < project < path override < CLI");
                    println!("root: {}", resolved.root.display());
                    println!(
                        "policy: {}; layout: {}",
                        resolved.config.policy.mode, resolved.config.policy.layout
                    );
                }
                ConfigAction::Schema => unreachable!(),
            }
        }
    }
    Ok(0)
}

fn print_languages() {
    println!("language\textensions / guaranteed dialects");
    println!("rust\trs");
    println!("ocaml\tml,mli (OCaml 5.5 lexical forms)");
    println!("c\tc,h / standard, GNU, Objective-C");
    println!("cpp\tcc,cpp,cxx,hpp / standard, GNU, Objective-C++, CUDA");
    println!("go\tgo");
    println!("java\tjava (Unicode escape translation)");
    println!("javascript\tjs,mjs,cjs,jsx / ECMAScript, JSX");
    println!("typescript\tts,mts,cts,tsx / TypeScript, TSX");
    println!("python\tpy,pyw,pyi");
    println!("shell\tsh,bash,zsh / POSIX sh, Bash 5.3, zsh");
    println!("html\thtml,htm / recursive script and style");
    println!("css\tcss");
    println!("jsonc\tjsonc,json5");
    println!("sql\tsql / PostgreSQL, MySQL, SQLite, T-SQL, Oracle");
    println!("kotlin\tkt,kts");
}

fn run_plugin(args: PluginArgs, common: &CommonArgs) -> Result<u8> {
    let resolved = config::load(common.config.as_deref())?;
    match args.command {
        PluginCommand::Add {
            source,
            name,
            sha256,
            identity,
        } => plugin::add(
            &resolved.root,
            &source,
            name.as_deref(),
            sha256.as_deref(),
            identity.as_deref(),
        )?,
        PluginCommand::Remove { name } => plugin::remove(&resolved.root, &name)?,
        PluginCommand::List => plugin::list(&resolved.root)?,
        PluginCommand::Update { name } => plugin::update(&resolved.root, name.as_deref())?,
        PluginCommand::Verify { name } => plugin::verify(&resolved.root, name.as_deref())?,
        PluginCommand::New { path } => plugin::new_plugin(&path)?,
    }
    Ok(0)
}

fn run_doctor(common: &CommonArgs) -> Result<u8> {
    println!("ocomment {}", env!("CARGO_PKG_VERSION"));
    let resolved = config::load(common.config.as_deref())?;
    println!("configuration: ok (root {})", resolved.root.display());
    println!("languages: {} built in", Language::ALL.len());
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok()
    {
        println!("git: available");
    } else {
        println!("git: unavailable (only --staged is affected)");
    }
    plugin::verify(&resolved.root, None)?;
    println!(
        "LSP: stdio server available; on-save is opt-in ({})",
        resolved.config.lsp.on_save
    );
    Ok(0)
}

fn presentation(common: &CommonArgs) -> Presentation {
    let stdout_tty = io::stdout().is_terminal();
    let no_color = std::env::var_os("NO_COLOR").is_some();
    Presentation {
        color: !no_color
            && match common.output.color {
                ColorChoice::Auto => stdout_tty,
                ColorChoice::Always => true,
                ColorChoice::Never => false,
            },
        hyperlinks: match common.output.hyperlinks {
            AutoChoice::Auto => stdout_tty,
            AutoChoice::Always => true,
            AutoChoice::Never => false,
        },
    }
}

/// The `--verbose` header: where the run is rooted, what it was pointed at,
/// and which configuration files it merged.
fn trace_run(resolved: &config::ResolvedConfig, paths: &[PathBuf]) {
    eprintln!("root: {}", resolved.root.display());
    let target = if paths.is_empty() {
        ".".to_owned()
    } else {
        paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    eprintln!("target: {target}");
    let trace = &resolved.trace;
    let sources = [
        ("user", &trace.user),
        ("project", &trace.project),
        ("explicit", &trace.explicit),
    ];
    let mut traced = false;
    for (label, path) in sources {
        if let Some(path) = path {
            eprintln!("config: {label} {}", path.display());
            traced = true;
        }
    }
    if !traced {
        eprintln!("config: built-in defaults");
    }
}
