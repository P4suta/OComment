use crate::{
    atomic::{WritePlan, apply_transaction},
    config, files, git, lsp,
    output::{self, Operation, OutputFormat, Presentation, ProcessedFile},
    plugin,
};
use anyhow::{Context, Result};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use ocomment_core::{CommentKind, Dialect, Language, Layout, Policy, transform};
use rayon::prelude::*;
use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    name = "ocomment",
    version,
    about = "Check and remove source-code comments safely"
)]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(flatten)]
    common: CommonArgs,
    #[command(subcommand)]
    command: Option<Command>,
    /// Paths for the implicit `check` command.
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Args)]
struct CommonArgs {
    /// Explicit configuration file.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Output encoding.
    #[arg(long, global = true, value_enum, default_value_t)]
    format: OutputFormat,
    #[arg(long, global = true)]
    policy: Option<Policy>,
    #[arg(long, global = true)]
    layout: Option<Layout>,
    #[arg(long, global = true)]
    language: Option<Language>,
    #[arg(long, global = true)]
    dialect: Option<Dialect>,
    #[arg(long = "keep-kind", global = true, value_delimiter = ',')]
    keep_kind: Vec<CommentKind>,
    #[arg(long = "remove-kind", global = true, value_delimiter = ',')]
    remove_kind: Vec<CommentKind>,
    #[arg(long, global = true)]
    force_invalid: bool,
    #[arg(long, global = true)]
    force_protected: bool,
    #[arg(long, global = true, value_enum, default_value_t)]
    color: ColorChoice,
    #[arg(long, global = true, value_enum, default_value_t)]
    hyperlinks: AutoChoice,
    #[arg(long, global = true, value_enum, default_value_t)]
    progress: AutoChoice,
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
    Check(TargetArgs),
    Fix(TargetArgs),
    Diff(TargetArgs),
    Scan(TargetArgs),
    Strip,
    Lsp,
    Init(InitArgs),
    Config(ConfigArgs),
    Languages,
    Plugin(PluginArgs),
    Completions { shell: Shell },
    Doctor,
}

#[derive(Clone, Debug, Default, Args)]
struct TargetArgs {
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
    #[arg(value_enum, default_value_t)]
    kind: InitKind,
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
    #[command(subcommand)]
    command: PluginCommand,
}

#[derive(Subcommand)]
enum PluginCommand {
    Add {
        source: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        sha256: Option<String>,
        #[arg(long)]
        identity: Option<String>,
    },
    Remove {
        name: String,
    },
    List,
    Update {
        name: Option<String>,
    },
    Verify {
        name: Option<String>,
    },
    New {
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
    }
}

fn run_target(operation: Operation, args: TargetArgs, common: &CommonArgs) -> Result<u8> {
    let mut resolved = config::load(common.config.as_deref())?;
    apply_cli_overrides(&mut resolved.config, common);
    let plugin_host = plugin::PluginHost::load(&resolved.root, &resolved.config.plugins)?;
    let presentation = presentation(common);
    let staged = args.staged || resolved.config.git.staged;
    if staged {
        return git::run_staged(git::StagedRequest {
            operation,
            paths: &args.paths,
            resolved: &resolved,
            format: common.format,
            index_only: args.index_only || resolved.config.git.index_only,
            plugin_host: &plugin_host,
            forced_language: common.language,
            forced_dialect: common.dialect,
            presentation,
        });
    }
    let discovery = files::discover(&args.paths, &resolved, common.language, common.dialect)?;
    let files: Vec<_> = discovery
        .files
        .par_iter()
        .map(|file| {
            let (mut language, mut options) =
                resolved.for_path(&file.path, file.language, file.dialect);
            if let Some(value) = common.language {
                language = value;
            }
            if let Some(value) = common.dialect {
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
    if operation == Operation::Fix && may_fix {
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
        common.format,
        operation,
        presentation,
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
        .language
        .map(|language| (language, common.dialect.unwrap_or(Dialect::Standard)))
        .or_else(|| {
            ocomment_core::detect_language(None, &source)
                .map(|value| (value.language, value.dialect))
        })
        .context("cannot detect stdin language; pass --language")?;
    let (language, mut options) =
        resolved.for_path(std::path::Path::new("<stdin>"), detection.0, detection.1);
    if let Some(value) = common.dialect {
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
    if let Some(value) = common.policy {
        config.policy.mode = value;
    }
    if let Some(value) = common.layout {
        config.policy.layout = value;
    }
    if !common.keep_kind.is_empty() {
        config
            .policy
            .keep_kind
            .extend(common.keep_kind.iter().copied());
    }
    if !common.remove_kind.is_empty() {
        config
            .policy
            .remove_kind
            .extend(common.remove_kind.iter().copied());
    }
    if common.force_invalid {
        config.policy.force_invalid = true;
    }
    if common.force_protected {
        config.policy.force_protected = true;
    }
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
                        "policy: {:?}; layout: {:?}",
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
    println!("languages: {} built in", Language::BUILT_INS.len());
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
    let stderr_tty = io::stderr().is_terminal();
    let no_color = std::env::var_os("NO_COLOR").is_some();
    Presentation {
        color: !no_color
            && match common.color {
                ColorChoice::Auto => stdout_tty,
                ColorChoice::Always => true,
                ColorChoice::Never => false,
            },
        hyperlinks: match common.hyperlinks {
            AutoChoice::Auto => stdout_tty,
            AutoChoice::Always => true,
            AutoChoice::Never => false,
        },
        progress: match common.progress {
            AutoChoice::Auto => stderr_tty,
            AutoChoice::Always => true,
            AutoChoice::Never => false,
        },
    }
}
