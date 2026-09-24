//! This repository's own task runner.
//!
//! `cargo xtask preflight` runs everything CI checks that a laptop can check,
//! in the order that fails soonest for the least money, and `lefthook install` wires it into `pre-push`.
//! Waiting eight minutes to be told about a stale manual page is not a review cycle.
//!
//! It is Rust rather than a shell script for the reason the workspace is Rust:
//! a task runner is code, and code that decides what a release gate does should be read, typed and tested by the same toolchain as everything else it gates.
//! A shell script is also the one thing here that would not survive the Windows job it is supposed to stand in for.

use anyhow::{Context, Result, bail};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{RED}xtask: {error:#}{RESET}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let task = arguments.next();
    let rest: Vec<String> = arguments.collect();
    match task.as_deref() {
        Some("preflight") => preflight(&rest),
        Some("differential") => differential(&rest),
        Some("release-check") => release_check(),
        Some("package-list") => package_list(&rest),
        Some("help" | "--help" | "-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => bail!("unknown task `{other}`\n\n{USAGE}"),
    }
}

const USAGE: &str = "\
cargo xtask <TASK>

TASKS
  preflight [--quick]   Everything CI checks that a laptop can check.
                        --quick drops the three slowest steps.
  differential [ARGS]   Run the shared fixture corpus through both
                        implementations and compare them.
  release-check         The preflight sweep against a release build, plus the
                        performance gate.
  package-list [ARGS]   The release artefact manifest.
  help                  This message.";

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";

const GIT_REPOSITORY_ENVIRONMENT: [&str; 15] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// Where the repository is, found from this crate rather than from the directory the caller happened to be in.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rust/xtask sits two directories under the repository root")
        .to_path_buf()
}

/// One thing that has to be true before a push.
struct Sweep {
    root: PathBuf,
    /// A Python with `tomllib`, which several of the checks still need.
    python: String,
    number: usize,
    started: Instant,
    skipped: Vec<String>,
}

impl Sweep {
    fn new(root: PathBuf, python: String) -> Self {
        Self {
            root,
            python,
            number: 0,
            started: Instant::now(),
            skipped: Vec::new(),
        }
    }

    /// Run one step, or stop the sweep where it failed.
    fn step<I, S>(&mut self, name: &str, program: &str, arguments: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.step_with(name, program, arguments, &[])
    }

    /// The same, with environment variables this step alone is given.
    fn step_with<I, S>(
        &mut self,
        name: &str,
        program: &str,
        arguments: I,
        environment: &[(&str, &str)],
    ) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.number += 1;
        let started = Instant::now();
        println!("\n{BOLD}[{:02}] {name}{RESET}", self.number);
        let mut command = Command::new(program);
        command.current_dir(&self.root).args(arguments);
        for key in GIT_REPOSITORY_ENVIRONMENT {
            command.env_remove(key);
        }
        for (key, value) in environment {
            command.env(key, value);
        }
        let status = command
            .status()
            .with_context(|| format!("cannot start `{program}` for step {name}"))?;
        if !status.success() {
            bail!(
                "{name} failed ({status}). Fix it and run `cargo xtask preflight` again.\n\
                 {DIM}Steps before it passed; nothing after it ran.{RESET}"
            );
        }
        println!("{DIM}     {:.1}s{RESET}", started.elapsed().as_secs_f64());
        Ok(())
    }

    /// Note a step that could not run here, so that "it passed" never quietly means "it was not asked".
    fn skip(&mut self, name: &str, why: &str) {
        println!("\n{YELLOW}[--] {name}: skipped, {why}{RESET}");
        self.skipped.push(name.to_owned());
    }

    fn finish(self) {
        let seconds = self.started.elapsed().as_secs_f64();
        if self.skipped.is_empty() {
            println!(
                "\n{GREEN}preflight passed: {} steps in {seconds:.0}s.{RESET}",
                self.number
            );
        } else {
            println!(
                "\n{GREEN}preflight passed: {} steps in {seconds:.0}s{RESET}{YELLOW}, {} skipped: {}.{RESET}",
                self.number,
                self.skipped.len(),
                self.skipped.join(", ")
            );
        }
    }
}

/// Whether a program is on the path at all.
fn available(program: &str) -> bool {
    Command::new(program)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .status()
        .is_ok()
}

/// A Python that can read a TOML file, which is 3.11 and up.
///
/// Several checks are still Python and are being ported one at a time.
/// Naming the interpreter here rather than assuming `python3` is what keeps the sweep runnable on a machine whose `python3` is older than the tools need.
fn python(root: &Path) -> Result<String> {
    let mut candidates: Vec<String> = std::env::var("OCOMMENT_PYTHON").ok().into_iter().collect();
    candidates.push("python3".to_owned());
    candidates.extend((11..=20).rev().map(|minor| format!("python3.{minor}")));
    // NOTE: A version manager's interpreter is not on the path under its own name unless it has been activated here, and the one it installed is usually the only one new enough.
    // NOTE: Newest first.
    if let Some(home) = std::env::var_os("HOME") {
        let installs = Path::new(&home).join(".local/share/mise/installs/python");
        if let Ok(entries) = std::fs::read_dir(&installs) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .collect();
            versions.sort();
            candidates.extend(
                versions
                    .into_iter()
                    .rev()
                    .map(|path| path.join("bin/python3").to_string_lossy().into_owned()),
            );
        }
    }
    for candidate in candidates {
        if candidate.is_empty() {
            continue;
        }
        let usable = Command::new(&candidate)
            .current_dir(root)
            .args(["-c", "import tomllib"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if usable {
            return Ok(candidate);
        }
    }
    bail!("no Python with `tomllib` (3.11+) was found; set OCOMMENT_PYTHON")
}

const MANIFEST: &str = "rust/Cargo.toml";
const BINARY: &str = "rust/target/debug/ocomment";

fn preflight(arguments: &[String]) -> Result<()> {
    let mut quick = false;
    for argument in arguments {
        match argument.as_str() {
            "--quick" => quick = true,
            other => bail!("unknown option `{other}`\n\n{USAGE}"),
        }
    }

    let root = root();
    let python = python(&root)?;
    let mut sweep = Sweep::new(root, python);

    sweep.step(
        "Format",
        "cargo",
        ["fmt", "--all", "--manifest-path", MANIFEST, "--", "--check"],
    )?;
    sweep.step(
        "Build",
        "cargo",
        [
            "build",
            "--manifest-path",
            MANIFEST,
            "--locked",
            "--workspace",
        ],
    )?;
    sweep.step(
        "Clippy",
        "cargo",
        [
            "clippy",
            "--manifest-path",
            MANIFEST,
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    )?;

    // NOTE: The formatter-conformance cases skip where gofmt or rustfmt is missing, and a skip that can become permanent is a test that quietly stopped running -- so they are required where it is there.
    let formatters: &[(&str, &str)] = if available("gofmt") {
        &[("OCOMMENT_REQUIRE_FORMATTERS", "1")]
    } else {
        &[]
    };
    sweep.step_with(
        "Tests",
        "cargo",
        [
            "test",
            "--manifest-path",
            MANIFEST,
            "--workspace",
            "--all-targets",
            "--locked",
        ],
        formatters,
    )?;
    sweep.step(
        "Doctests",
        "cargo",
        [
            "test",
            "--manifest-path",
            MANIFEST,
            "--doc",
            "--workspace",
            "--locked",
        ],
    )?;

    // NOTE: `docs/library.md` is hand-written prose the step above never reads: `--doc` compiles what is in the crate sources and no more.
    sweep.step(
        "Library page",
        "cargo",
        [
            "build",
            "--manifest-path",
            MANIFEST,
            "--locked",
            "-p",
            "ocomment-core",
        ],
    )?;
    sweep.step(
        "Library page examples",
        "rustdoc",
        [
            "--test",
            "docs/library.md",
            "--edition",
            "2024",
            "--extern",
            "ocomment_core=rust/target/debug/libocomment_core.rlib",
            "-L",
            "rust/target/debug/deps",
        ],
    )?;

    let python = sweep.python.clone();
    for (name, script, extra) in PYTHON_CHECKS {
        let mut arguments = vec![(*script).to_owned()];
        arguments.extend(extra.iter().map(|value| (*value).to_owned()));
        sweep.step(name, &python, &arguments)?;
    }
    sweep.step(
        "Tool tests",
        &python,
        [
            "-m",
            "unittest",
            "tools/test_release_metadata.py",
            "tools/test_publish_crates.py",
            "tools/test_sync_release_docs.py",
        ],
    )?;

    sweep.step("Own gate", BINARY, ["--format", "github"])?;
    sweep.step(
        "Own coverage",
        BINARY,
        ["coverage", "--deny-skipped", "--quiet"],
    )?;
    sweep.step("Self-test", BINARY, ["selftest"])?;

    if available("mdbook") {
        sweep.step("Book", "mdbook", ["build", "docs"])?;
    } else {
        sweep.skip("Book", "mdbook is not installed");
    }

    if quick {
        sweep.skip("Differential", "--quick");
        sweep.skip("YAML round trip", "--quick");
        sweep.skip("Documentation links", "--quick");
    } else {
        if available("dune") || available("opam") {
            sweep.step(
                "Differential driver (Rust)",
                "cargo",
                [
                    "build",
                    "--manifest-path",
                    MANIFEST,
                    "-p",
                    "ocomment-core",
                    "--example",
                    "ref_driver",
                    "--locked",
                ],
            )?;
            sweep.step(
                "Differential driver (OCaml)",
                "dune",
                ["build", "--root", "ocaml", "bin/main.exe"],
            )?;
            sweep.step("Differential", &python, ["tools/differential.py"])?;
        } else {
            sweep.skip("Differential", "there is no OCaml toolchain here");
        }
        sweep.step(
            "YAML round trip",
            &python,
            [
                "tools/yaml_roundtrip.py",
                "--binary",
                BINARY,
                "--cases",
                "200",
            ],
        )?;
        sweep.step(
            "Documentation links",
            "cargo",
            [
                "doc",
                "--manifest-path",
                MANIFEST,
                "--no-deps",
                "--workspace",
                "--locked",
            ],
        )?;
    }

    sweep.finish();
    Ok(())
}

/// Build both drivers and run every shared fixture through them.
///
/// Two implementations agreeing is the strongest claim this repository makes,
/// and it takes a built OCaml tree to make it, so the build is part of the task rather than something a reader is expected to remember.
fn differential(arguments: &[String]) -> Result<()> {
    let root = root();
    let python = python(&root)?;
    let mut sweep = Sweep::new(root, python.clone());
    sweep.step(
        "Rust driver",
        "cargo",
        [
            "build",
            "--manifest-path",
            MANIFEST,
            "-p",
            "ocomment-core",
            "--example",
            "ref_driver",
            "--locked",
        ],
    )?;
    sweep.step(
        "OCaml driver",
        "dune",
        ["build", "--root", "ocaml", "bin/main.exe"],
    )?;
    let mut compare = vec!["tools/differential.py".to_owned()];
    compare.extend(arguments.iter().cloned());
    sweep.step("Compare", &python, &compare)?;
    sweep.finish();
    Ok(())
}

/// The sweep a release is cut from: everything `preflight` checks, against a release build, plus the performance gate a debug build cannot answer for.
fn release_check() -> Result<()> {
    preflight(&[])?;
    let root = root();
    let python = python(&root)?;
    let mut sweep = Sweep::new(root, python.clone());
    sweep.step(
        "Release build",
        "cargo",
        [
            "build",
            "--manifest-path",
            MANIFEST,
            "--release",
            "--locked",
            "-p",
            "ocomment",
        ],
    )?;
    sweep.step(
        "Throughput example",
        "cargo",
        [
            "build",
            "--manifest-path",
            MANIFEST,
            "--release",
            "--locked",
            "-p",
            "ocomment-core",
            "--example",
            "throughput",
        ],
    )?;
    const RELEASE_BINARY: &str = "rust/target/release/ocomment";
    sweep.step(
        "Schemas (release)",
        &python,
        ["tools/validate_schemas.py", "--binary", RELEASE_BINARY],
    )?;
    sweep.step(
        "Directives (release)",
        &python,
        ["tools/check_directives.py", "--binary", RELEASE_BINARY],
    )?;
    sweep.step("Performance gate", &python, ["tools/release_gate.py"])?;
    sweep.step(
        "Release metadata (release)",
        &python,
        [
            "tools/release_metadata.py",
            "--workspace",
            "--binary",
            RELEASE_BINARY,
        ],
    )?;
    sweep.step("OCaml tests", "dune", ["runtest", "--root", "ocaml"])?;
    sweep.finish();
    Ok(())
}

/// The release artefact manifest, which is `tools/package_artifacts.py` under a name the workflow can call without knowing that.
fn package_list(arguments: &[String]) -> Result<()> {
    let root = root();
    let python = python(&root)?;
    let mut command = Command::new(&python);
    command
        .current_dir(&root)
        .arg("tools/package_artifacts.py")
        .args(arguments);
    let status = command
        .status()
        .context("cannot start the packaging tool")?;
    if !status.success() {
        bail!("packaging failed ({status})");
    }
    Ok(())
}

/// The checks that are still Python, with the arguments each one takes.
///
/// `tools/check_ci_contracts.py` holds this list against `.github/workflows/ci.yml`, so a gate added to CI and not to this table fails rather than quietly stopping here.
/// They are being ported to tasks one at a time; the table is what says how far that has got.
const PYTHON_CHECKS: &[(&str, &str, &[&str])] = &[
    (
        "Schemas",
        "tools/validate_schemas.py",
        &["--binary", BINARY],
    ),
    ("Embedded specs", "tools/check_embedded_specs.py", &[]),
    (
        "Directives",
        "tools/check_directives.py",
        &["--binary", BINARY],
    ),
    (
        "Generated docs",
        "tools/gen_docs.py",
        &["--binary", BINARY, "--check"],
    ),
    (
        "Self-test corpus",
        "tools/gen_selftest_corpus.py",
        &["--check"],
    ),
    ("Hooks", "tools/check_hooks.py", &[]),
    ("Editor ids", "tools/check_editor_ids.py", &[]),
    ("CI contracts", "tools/check_ci_contracts.py", &[]),
    /* NOTE: The one check here that asks somebody else, so it is also the one a train tunnel or an exhausted rate limit can stop.
     * It says which pin it did not read and passes; CI runs it without the flag, where neither excuse is available and a read it cannot make fails the run. */
    (
        "Action pins",
        "tools/check_action_pins.py",
        &["--best-effort"],
    ),
    /* NOTE: Beside it for the same reason and with the same escape: both ask somebody else, and CI asks without one. */
    (
        "Advisories",
        "tools/check_advisories.py",
        &["--best-effort"],
    ),
    ("Gate symmetry", "tools/check_gate_symmetry.py", &[]),
    (
        "Release metadata",
        "tools/release_metadata.py",
        &["--workspace", "--binary", BINARY],
    ),
    ("Release docs", "tools/sync_release_docs.py", &["--check"]),
];
