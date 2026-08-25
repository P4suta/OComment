use crate::{
    atomic::{WritePlan, apply_transaction},
    config, files, git, lsp,
    output::{
        self, Operation, OutputFormat, Presentation, ProcessedFile, RenderOptions, Verbosity,
    },
    plugin,
    values::{CommentKindArg, DialectArg, LanguageArg, LayoutArg, PolicyArg},
};
use anyhow::{Context, Result, bail};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use ocomment_core::{CommentKind, Dialect, Language, transform};
use rayon::prelude::*;
use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
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

/// The roff sections `clap_mangen` cannot derive, carrying the same content as
/// the `--help` epilogue above. A line that would start with `.` is escaped
/// with `\&` so roff reads a file name as text rather than as a macro.
const MAN_SECTIONS: &str = r#".SH EXIT STATUS
.TP
.B 0
Nothing removable was found and every requested change was applied.
.TP
.B 1
Removable comments were reported, or a diff was printed.
.TP
.B 2
Invalid source, configuration, plugin, or I/O failure.
.SH FILES
.TP
.B \&.ocomment.toml
Project configuration, merged over the user file.
.TP
.B \&.ocommentignore
Extra ignore patterns honoured by repository walks.
.TP
.B \&.ocomment.lock
Pinned digests of the installed WASM scanner plugins.
.TP
.B $XDG_CONFIG_HOME/ocomment/config.toml
User configuration, merged over the built\-in defaults.
.SH EXAMPLES
.TP
.B ocomment
Check the current repository and report removable comments.
.TP
.B ocomment fix \-\-policy all \-\-layout compact src
Remove every comment under src and close the gaps it leaves.
.TP
.B ocomment strip \-\-language rust < before.rs > after.rs
Strip one file from standard input to standard output.
.SH SEE ALSO
The complete schemas and guides are available in the OComment repository.
"#;

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
    /// Files or directories to check; `-` reads standard input (default: current directory).
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
    /// When to draw the live scanning counter on standard error.
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
    Fix(FixArgs),
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
    /// Files or directories to process; `-` reads standard input (default: current directory).
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,
    /// Whether the Git index, rather than the working tree, is the source.
    #[command(flatten)]
    git: GitArgs,
}

/// The `--staged` pair, shared by every command that can read the Git index.
#[derive(Clone, Debug, Default, Args)]
struct GitArgs {
    /// Read and update Git index blobs rather than treating the working tree as the source.
    #[arg(long)]
    staged: bool,
    /// With `--staged`, do not attempt a uniquely mappable working-tree update.
    #[arg(long, requires = "staged")]
    index_only: bool,
}

#[derive(Args)]
struct FixArgs {
    // `fix` rewrites files in place and refuses the `-` that stands for
    // standard input, so its PATH list is not the one every other command
    // takes and does not borrow that command's help line.
    /// Files or directories to rewrite (default: current directory).
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,
    /// Whether the Git index, rather than the working tree, is rewritten.
    #[command(flatten)]
    git: GitArgs,
    /// Print the patch `fix` would apply and write nothing.
    #[arg(long)]
    dry_run: bool,
}

impl FixArgs {
    /// The same targets in the shape every other command hands to the run.
    fn target(self) -> TargetArgs {
        TargetArgs {
            paths: self.paths,
            git: self.git,
        }
    }
}

#[derive(Args)]
struct InitArgs {
    /// Which starter file to write.
    #[arg(value_enum, default_value_t)]
    kind: InitKind,
    /// For the Lefthook hook, run `fix` instead of `check`.
    #[arg(long)]
    fix: bool,
    /// Replace the file if it already exists.
    #[arg(long, conflicts_with = "stdout")]
    force: bool,
    /// Print the template to standard output and write no file.
    #[arg(long)]
    stdout: bool,
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
            false,
        ),
        Some(Command::Check(args)) => run_target(Operation::Check, args, &common, false),
        // `--dry-run` runs the diff and reports it in fix vocabulary: the two
        // commands must agree on the patch, so only the wording differs.
        Some(Command::Fix(args)) if args.dry_run => {
            run_target(Operation::Diff, args.target(), &common, true)
        }
        Some(Command::Fix(args)) => run_target(Operation::Fix, args.target(), &common, false),
        Some(Command::Diff(args)) => run_target(Operation::Diff, args, &common, false),
        Some(Command::Scan(args)) => run_target(Operation::Scan, args, &common, false),
        Some(Command::Strip) => run_strip(&common),
        Some(Command::Lsp) => lsp::run(common.config.as_deref()),
        Some(Command::Init(args)) => run_init(args),
        Some(Command::Config(args)) => run_config(args, &common),
        Some(Command::Languages) => print_languages(),
        Some(Command::Plugin(args)) => run_plugin(args, &common),
        Some(Command::Completions { shell }) => run_completions(shell),
        Some(Command::Doctor) => run_doctor(&common),
        Some(Command::Man) => run_man(),
    }
}

fn run_target(
    operation: Operation,
    args: TargetArgs,
    common: &CommonArgs,
    dry_run: bool,
) -> Result<u8> {
    let mut resolved = config::load(common.config.as_deref())?;
    apply_cli_overrides(&mut resolved.config, common);
    let plugin_host = plugin::PluginHost::load(&resolved.root, &resolved.config.plugins)?;
    let presentation = presentation(common);
    let verbosity = common.verbosity();
    // The trace is part of the human report; a machine format keeps standard
    // error empty however loud the run was asked to be.
    if verbosity == Verbosity::Verbose && common.output.format == OutputFormat::Human {
        trace_run(&resolved, &args.paths)?;
    }
    let progress = progress_enabled(common);
    let staged = args.git.staged || resolved.config.git.staged;
    // `fix --dry-run` writes nothing, but it is still the command whose job is
    // to rewrite files in place, and standard input cannot be rewritten.
    let rewrites = operation == Operation::Fix || dry_run;
    let (paths, stdin) = target_paths(&args.paths, rewrites, staged)?;
    if staged {
        return git::run_staged(git::StagedRequest {
            operation,
            paths: &paths,
            resolved: &resolved,
            format: common.output.format,
            index_only: args.git.index_only || resolved.config.git.index_only,
            plugin_host: &plugin_host,
            forced_language: common.language(),
            forced_dialect: common.dialect(),
            presentation,
            verbosity,
            preview: !common.output.no_preview,
            dry_run,
        });
    }
    let discovery = read_targets(&paths, stdin, &resolved, common)?;
    let total = discovery.files.len();
    let counter = Progress::default();
    let processed = discovery
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
            if progress {
                counter.report(total);
            }
            Ok::<_, anyhow::Error>(ProcessedFile {
                path: file.path.clone(),
                source: file.source.clone(),
                language,
                result,
            })
        })
        .collect::<Result<Vec<_>>>();
    if progress {
        counter.clear();
    }
    let files = processed?;

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

/// How the PATH list names standard input.
const STDIN_ARGUMENT: &str = "-";

/// Split the requested targets into ordinary paths and the `-` that stands for
/// standard input, refusing the combinations that cannot be honoured.
fn target_paths(paths: &[PathBuf], rewrites: bool, staged: bool) -> Result<(Vec<PathBuf>, bool)> {
    let is_stdin = |path: &PathBuf| path.as_os_str() == STDIN_ARGUMENT;
    match paths.iter().filter(|path| is_stdin(path)).count() {
        0 => return Ok((paths.to_vec(), false)),
        1 => {}
        // A pipe is consumed once; a second `-` would silently report the same
        // bytes twice or nothing at all.
        _ => bail!("cannot read standard input twice; `-` may appear only once"),
    }
    if rewrites {
        bail!("cannot rewrite standard input in place; use `ocomment strip`");
    }
    if staged {
        bail!("cannot read standard input with --staged; the index is the source");
    }
    Ok((
        paths
            .iter()
            .filter(|path| !is_stdin(path))
            .cloned()
            .collect(),
        true,
    ))
}

/// Discover the named paths and, when `-` was among them, fold the bytes read
/// from standard input in as one more file so a piped run takes exactly the
/// same reporting path as a walked one.
fn read_targets(
    paths: &[PathBuf],
    stdin: bool,
    resolved: &config::ResolvedConfig,
    common: &CommonArgs,
) -> Result<files::Discovery> {
    if !stdin {
        return files::discover(paths, resolved, common.language(), common.dialect());
    }
    // An empty list means "the whole repository" only when no target was named
    // at all; `-` on its own is a target, and walking would ignore it.
    let mut discovery = if paths.is_empty() {
        files::Discovery::default()
    } else {
        files::discover(paths, resolved, common.language(), common.dialect())?
    };
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .read_to_end(&mut bytes)
        .context("cannot read standard input")?;
    match files::stdin_source(bytes, resolved, common.language(), common.dialect()) {
        Ok(file) => discovery.files.push(file),
        // A skip that cannot be reported per file — nothing was named to skip
        // — is a usage error the run must not swallow.
        Err(skipped) if skipped.error => {
            let reason = skipped.reason;
            bail!("{reason}")
        }
        Err(skipped) => discovery.skipped.push(skipped),
    }
    discovery
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    discovery
        .skipped
        .sort_by(|left, right| left.path.cmp(&right.path));
    Ok(discovery)
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
        .context(files::STDIN_LANGUAGE_HELP)?;
    let (language, mut options) = resolved.for_path(
        std::path::Path::new(files::STDIN_PATH),
        detection.0,
        detection.1,
    );
    if let Some(value) = common.dialect() {
        config::validate_dialect(language, value)?;
        options.scan.dialect = value;
    }
    let result = transform(&source, language, options);
    let stderr = io::stderr();
    let mut report = stderr.lock();
    for diagnostic in &result.report.diagnostics {
        output::note(
            &mut report,
            &format!(
                "stdin:{}..{}: {}: {}",
                diagnostic.span.start, diagnostic.span.end, diagnostic.code, diagnostic.message
            ),
        )?;
    }
    if !result.report.valid && !resolved.config.policy.force_invalid {
        return Ok(2);
    }
    let mut stdout = output::stdout();
    output::wrote(stdout.write_all(&result.output))?;
    output::finish(&mut stdout)?;
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

/// The apostrophe definition `roff` writes at the top of every fragment it
/// renders. A page needs it once, so it is stripped from every fragment after
/// the first.
const ROFF_PREAMBLE: &str = concat!(r".ie \n(.g .ds Aq \(aq", "\n", r".el .ds Aq '", "\n");

/// Append one rendered `roff` fragment to the page under construction.
fn append_fragment(page: &mut String, fragment: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(fragment).context("the manual page is not valid UTF-8")?;
    page.push_str(text.strip_prefix(ROFF_PREAMBLE).unwrap_or(text));
    Ok(())
}

/// Render the arguments that belong to one command alone, as `.SS` subsections.
///
/// `clap_mangen` renders a single page for the root command, so an argument
/// declared on a subcommand — `fix --dry-run`, `init --force`, `plugin add
/// --sha256` — would never reach the manual at all. Every command is walked
/// and the arguments it does not inherit are written under its own heading.
fn command_options(command: &clap::Command, path: &str, page: &mut String) -> Result<()> {
    for subcommand in command.get_subcommands() {
        if subcommand.is_hide_set() || subcommand.get_name() == "help" {
            continue;
        }
        let name = format!("{path} {}", subcommand.get_name());
        // The global arguments already have one entry each under OPTIONS,
        // POLICY, and OUTPUT, and `--help` is on every command by definition.
        // Repeating them here would bury the few arguments this section is
        // for. Hiding is how `clap_mangen` is told to skip an argument.
        let mut own = subcommand.clone();
        let inherited: Vec<clap::Id> = own
            .get_arguments()
            .filter(|argument| {
                argument.is_global_set()
                    || argument.get_id() == "help"
                    || argument.get_id() == "version"
            })
            .map(|argument| argument.get_id().clone())
            .collect();
        for id in inherited {
            own = own.mut_arg(id, |argument| argument.hide(true));
        }
        let mut fragment = Vec::new();
        clap_mangen::Man::new(own)
            .render_options_section(&mut fragment)
            .context("cannot render the manual page")?;
        let mut rendered = String::new();
        append_fragment(&mut rendered, &fragment)?;
        // A command with nothing of its own renders an empty fragment, and an
        // empty heading would claim otherwise.
        if let Some(body) = rendered.strip_prefix(".SH OPTIONS\n")
            && !body.is_empty()
        {
            page.push_str(&format!(".SS {}\n{body}", name.replace('-', "\\-")));
        }
        command_options(subcommand, &name, page)?;
    }
    Ok(())
}

/// Render the roff manual page from the parser definition itself.
fn run_man() -> Result<u8> {
    // `clap_mangen` renders `after_long_help` as one opaque `.SH EXTRA` body,
    // so the page is built without it and the same content is appended below
    // as real roff sections. It is assembled section by section rather than
    // through `render`, because the per-command options belong next to the
    // command list and `render` puts VERSION after it.
    //
    // The `.TH` date is left blank on purpose: stamping the build date would
    // make two reproducible builds of the same source disagree.
    let man = clap_mangen::Man::new(Cli::command().after_long_help(None))
        .title("OCOMMENT")
        .manual("User Commands");
    let mut page = String::new();
    let mut fragment = Vec::new();
    // The title fragment keeps the apostrophe definition the whole page needs.
    man.render_title(&mut fragment)
        .context("cannot render the manual page")?;
    page.push_str(std::str::from_utf8(&fragment).context("the manual page is not valid UTF-8")?);
    type Section = fn(&clap_mangen::Man, &mut dyn Write) -> io::Result<()>;
    for section in [
        clap_mangen::Man::render_name_section as Section,
        clap_mangen::Man::render_synopsis_section,
        clap_mangen::Man::render_description_section,
        clap_mangen::Man::render_options_section,
        clap_mangen::Man::render_subcommands_section,
    ] {
        fragment.clear();
        section(&man, &mut fragment).context("cannot render the manual page")?;
        append_fragment(&mut page, &fragment)?;
    }
    let mut per_command = String::new();
    let mut root = Cli::command();
    // Building propagates the global arguments into every subcommand, which is
    // what makes them recognizable as inherited below.
    root.build();
    command_options(&root, "ocomment", &mut per_command)?;
    if !per_command.is_empty() {
        page.push_str(".SH COMMAND OPTIONS\n");
        page.push_str(&per_command);
    }
    fragment.clear();
    man.render_version_section(&mut fragment)
        .context("cannot render the manual page")?;
    append_fragment(&mut page, &fragment)?;
    if !page.ends_with('\n') {
        page.push('\n');
    }
    page.push_str(MAN_SECTIONS);
    let mut stdout = output::stdout();
    output::wrote(stdout.write_all(page.as_bytes()))?;
    output::finish(&mut stdout)?;
    Ok(0)
}

/// Write the shell completion script.
///
/// `clap_complete` writes straight into the handle it is given and panics if
/// that write fails, so it is given a buffer in memory and the one write that
/// can fail is made here.
fn run_completions(shell: Shell) -> Result<u8> {
    let mut script = Vec::new();
    generate(shell, &mut Cli::command(), "ocomment", &mut script);
    let mut stdout = output::stdout();
    output::wrote(stdout.write_all(&script))?;
    output::finish(&mut stdout)?;
    Ok(0)
}

fn run_init(args: InitArgs) -> Result<u8> {
    // Writing the file is only the first half of the task, so each template
    // carries the step that finishes it.
    let (path, contents, next_step) = match args.kind {
        InitKind::Config => (
            config::CONFIG_FILE,
            include_str!("../assets/default-config.toml").to_owned(),
            "edit [policy] and run `ocomment check`",
        ),
        InitKind::Lefthook => {
            let command = if args.fix {
                "ocomment fix --staged"
            } else {
                "ocomment check --staged"
            };
            (
                "lefthook.yml",
                format!("pre-commit:\n  commands:\n    ocomment:\n      run: {command}\n"),
                "run `lefthook install` to activate the hook",
            )
        }
    };
    let mut stdout = output::stdout();
    if args.stdout {
        // Nothing is created, so nothing is said about creating it: the
        // template alone is on standard output, ready to be redirected.
        output::wrote(write!(stdout, "{contents}"))?;
        output::finish(&mut stdout)?;
        return Ok(0);
    }
    note_inherited_config()?;
    write_template(&mut stdout, path, &contents, args.force, next_step)?;
    output::finish(&mut stdout)?;
    Ok(0)
}

/// Say so when a project configuration from a parent directory already governs
/// this directory.
///
/// The starter file is about to layer over it rather than start from nothing,
/// and the hook a `lefthook` run installs will read it — either way the reader
/// is better off knowing before they start editing. It is a note and not a
/// refusal: a nested per-crate configuration is a normal thing to want.
///
/// The search starts at the parent so that the file this very run is about to
/// write — or the one `--force` is replacing — is never reported as inherited.
fn note_inherited_config() -> Result<()> {
    let Ok(directory) = std::env::current_dir() else {
        return Ok(());
    };
    let Some(inherited) = directory.parent().and_then(config::locate_project) else {
        return Ok(());
    };
    let stderr = io::stderr();
    let mut report = stderr.lock();
    output::note(
        &mut report,
        &format!(
            "note: {} already applies to this directory",
            inherited.display()
        ),
    )
}

/// Write one starter file, refusing an existing one unless `force` says
/// otherwise.
///
/// The refusal is `create_new` rather than a prior `exists()` test: between
/// such a test and the open the file could appear, and never writing over
/// someone's edited configuration is the whole point of the check.
fn write_template(
    output: &mut impl Write,
    path: &str,
    contents: &str,
    force: bool,
    next_step: &str,
) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            anyhow::anyhow!(
                "{path} already exists; use --force to overwrite or --stdout to print the template"
            )
        } else {
            anyhow::Error::new(error).context(format!("cannot write {path}"))
        }
    })?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("cannot write {path}"))?;
    output::wrote(writeln!(output, "created {path} — {next_step}"))?;
    Ok(())
}

fn run_config(args: ConfigArgs, common: &CommonArgs) -> Result<u8> {
    let mut stdout = output::stdout();
    match args.action {
        ConfigAction::Schema => {
            output::wrote(write!(
                stdout,
                "{}",
                include_str!("../assets/config.schema.json")
            ))?;
        }
        action => {
            let mut resolved = config::load(common.config.as_deref())?;
            apply_cli_overrides(&mut resolved.config, common);
            match action {
                ConfigAction::Show => {
                    resolved.config.version = Some(1);
                    output::wrote(write!(
                        stdout,
                        "{}",
                        toml::to_string_pretty(&resolved.config)?
                    ))?;
                }
                ConfigAction::Locate => {
                    if let Some(path) = &resolved.trace.user {
                        output::wrote(writeln!(stdout, "user\t{}", path.display()))?;
                    }
                    if let Some(path) = &resolved.trace.project {
                        output::wrote(writeln!(stdout, "project\t{}", path.display()))?;
                    }
                    if let Some(path) = &resolved.trace.explicit {
                        output::wrote(writeln!(stdout, "explicit\t{}", path.display()))?;
                    }
                    if resolved.trace.user.is_none()
                        && resolved.trace.project.is_none()
                        && resolved.trace.explicit.is_none()
                    {
                        output::wrote(writeln!(stdout, "built-in defaults"))?;
                    }
                }
                ConfigAction::Explain => {
                    output::wrote(writeln!(
                        stdout,
                        "precedence: built-in < XDG user < project < path override < CLI"
                    ))?;
                    output::wrote(writeln!(stdout, "root: {}", resolved.root.display()))?;
                    output::wrote(writeln!(
                        stdout,
                        "policy: {}; layout: {}",
                        resolved.config.policy.mode, resolved.config.policy.layout
                    ))?;
                }
                ConfigAction::Schema => unreachable!(),
            }
        }
    }
    output::finish(&mut stdout)?;
    Ok(0)
}

fn print_languages() -> Result<u8> {
    let mut stdout = output::stdout();
    for line in [
        "language\textensions / guaranteed dialects",
        "rust\trs",
        "ocaml\tml,mli (OCaml 5.5 lexical forms)",
        "c\tc,h / standard, GNU, Objective-C",
        "cpp\tcc,cpp,cxx,hpp / standard, GNU, Objective-C++, CUDA",
        "go\tgo",
        "java\tjava (Unicode escape translation)",
        "javascript\tjs,mjs,cjs,jsx / ECMAScript, JSX",
        "typescript\tts,mts,cts,tsx / TypeScript, TSX",
        "python\tpy,pyw,pyi",
        "shell\tsh,bash,zsh / POSIX sh, Bash 5.3, zsh",
        "html\thtml,htm / recursive script and style",
        "css\tcss",
        "jsonc\tjsonc,json5",
        "sql\tsql / PostgreSQL, MySQL, SQLite, T-SQL, Oracle",
        "kotlin\tkt,kts",
    ] {
        output::wrote(writeln!(stdout, "{line}"))?;
    }
    output::finish(&mut stdout)?;
    Ok(0)
}

fn run_plugin(args: PluginArgs, common: &CommonArgs) -> Result<u8> {
    let resolved = config::load(common.config.as_deref())?;
    let mut stdout = output::stdout();
    match args.command {
        PluginCommand::Add {
            source,
            name,
            sha256,
            identity,
        } => plugin::add(
            &mut stdout,
            &resolved.root,
            &source,
            name.as_deref(),
            sha256.as_deref(),
            identity.as_deref(),
        )?,
        PluginCommand::Remove { name } => plugin::remove(&mut stdout, &resolved.root, &name)?,
        PluginCommand::List => plugin::list(&mut stdout, &resolved.root)?,
        PluginCommand::Update { name } => {
            plugin::update(&mut stdout, &resolved.root, name.as_deref())?;
        }
        PluginCommand::Verify { name } => {
            plugin::verify(&mut stdout, &resolved.root, name.as_deref())?;
        }
        PluginCommand::New { path } => plugin::new_plugin(&mut stdout, &path)?,
    }
    output::finish(&mut stdout)?;
    Ok(0)
}

fn run_doctor(common: &CommonArgs) -> Result<u8> {
    let mut stdout = output::stdout();
    output::wrote(writeln!(stdout, "ocomment {}", env!("CARGO_PKG_VERSION")))?;
    let resolved = config::load(common.config.as_deref())?;
    output::wrote(writeln!(
        stdout,
        "configuration: ok (root {})",
        resolved.root.display()
    ))?;
    output::wrote(writeln!(
        stdout,
        "languages: {} built in",
        Language::ALL.len()
    ))?;
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok()
    {
        output::wrote(writeln!(stdout, "git: available"))?;
    } else {
        output::wrote(writeln!(
            stdout,
            "git: unavailable (only --staged is affected)"
        ))?;
    }
    plugin::verify(&mut stdout, &resolved.root, None)?;
    output::wrote(writeln!(
        stdout,
        "LSP: stdio server available; on-save is opt-in ({})",
        resolved.config.lsp.on_save
    ))?;
    output::finish(&mut stdout)?;
    Ok(0)
}

/// How many files may be processed between two redraws of the counter.
const PROGRESS_STEP: usize = 50;

/// Whether this run draws the live scanning counter. The counter is terminal
/// decoration: it never belongs in a machine format, and `-q` silences it.
fn progress_enabled(common: &CommonArgs) -> bool {
    common.output.format == OutputFormat::Human
        && common.verbosity() != Verbosity::Quiet
        && match common.output.progress {
            AutoChoice::Auto => io::stderr().is_terminal(),
            AutoChoice::Always => true,
            AutoChoice::Never => false,
        }
}

/// The live scanning counter: how many files it has seen, and whether it ever
/// put a line on the screen.
#[derive(Default)]
struct Progress {
    scanned: AtomicUsize,
    drawn: AtomicBool,
}

impl Progress {
    /// Advance the live `n/total` counter, rewriting one line on standard
    /// error rather than scrolling a line for every file.
    fn report(&self, total: usize) {
        let seen = self.scanned.fetch_add(1, Ordering::Relaxed) + 1;
        if !seen.is_multiple_of(PROGRESS_STEP) && seen != total {
            return;
        }
        let mut stderr = io::stderr().lock();
        let _ = write!(stderr, "\rocomment: scanning {seen}/{total} files");
        let _ = stderr.flush();
        self.drawn.store(true, Ordering::Relaxed);
    }

    /// Erase the counter so the report that follows starts on a clean line.
    ///
    /// A run with nothing to scan draws no counter, and erasing a line it
    /// never wrote would put an escape sequence on a standard error whose
    /// reader was promised only the summary.
    fn clear(&self) {
        if !self.drawn.load(Ordering::Relaxed) {
            return;
        }
        let mut stderr = io::stderr().lock();
        let _ = write!(stderr, "\r\x1b[2K");
        let _ = stderr.flush();
    }
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
fn trace_run(resolved: &config::ResolvedConfig, paths: &[PathBuf]) -> Result<()> {
    let stderr = io::stderr();
    let mut report = stderr.lock();
    output::note(&mut report, &format!("root: {}", resolved.root.display()))?;
    let target = if paths.is_empty() {
        ".".to_owned()
    } else {
        paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    output::note(&mut report, &format!("target: {target}"))?;
    let trace = &resolved.trace;
    let sources = [
        ("user", &trace.user),
        ("project", &trace.project),
        ("explicit", &trace.explicit),
    ];
    let mut traced = false;
    for (label, path) in sources {
        if let Some(path) = path {
            output::note(&mut report, &format!("config: {label} {}", path.display()))?;
            traced = true;
        }
    }
    if !traced {
        output::note(&mut report, "config: built-in defaults")?;
    }
    Ok(())
}
