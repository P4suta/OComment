mod atomic;
mod cli;
mod config;
mod files;
mod git;
mod lsp;
mod output;
mod plugin;

use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("ocomment: {error:#}");
            ExitCode::from(2)
        }
    }
}
