mod atomic;
mod cli;
mod config;
mod files;
mod git;
mod interactive;
mod lsp;
mod output;
mod plugin;
mod values;

use std::{
    io::{self, Write},
    process::ExitCode,
};

/// Whether the reader of the program's own output is what ended the run.
///
/// `ocomment … | head` closes the pipe as soon as the reader has what it came
/// for. That is the reader finishing, not the run failing, so — following the
/// convention `rg` and `fd` set — the process ends quietly with status 0
/// rather than reporting an I/O error to a terminal that may itself be gone.
/// The failing write can be several layers down: the serializer wraps it, and
/// the caller adds context on top.
///
/// Only the writers of *our* report may claim this, and they say so by tagging
/// the failure with [`output::OutputPipeClosed`]. A bare `BrokenPipe` from
/// anywhere else — the write that feeds a rewritten blob to `git hash-object`,
/// above all — is a real failure whose silent success would lose data.
fn output_pipe_closed(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<output::OutputPipeClosed>().is_some())
}

fn main() -> ExitCode {
    match cli::run() {
        Ok(code) => ExitCode::from(code),
        Err(error) if output_pipe_closed(&error) => ExitCode::SUCCESS,
        Err(error) => {
            /* NOTE: Nothing is left to try if even the report cannot be written, and
             * `eprintln!` would panic there — an abort under the release
             * profile — so the failure of the last write is dropped. */
            let message = output::sanitize_message(&format!("{error:#}"));
            let _ = writeln!(io::stderr(), "ocomment: {message}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{output::OutputPipeClosed, output_pipe_closed};
    use anyhow::{Context, Result};
    use std::io::{Error, ErrorKind};

    /// The tag survives however many layers of context are added on top of it.
    #[test]
    fn a_tagged_output_pipe_is_recognized_through_its_context() {
        let error = Result::<()>::Err(anyhow::Error::new(OutputPipeClosed))
            .context("cannot write standard output")
            .context("check failed")
            .unwrap_err();
        assert!(output_pipe_closed(&error));
    }

    /// `git hash-object` exiting before it reads the blob raises a bare
    /// `BrokenPipe` that no output writer tagged. Ending quietly there would
    /// report a `fix --staged` that never happened.
    #[test]
    fn an_untagged_broken_pipe_is_not_an_output_pipe_closure() {
        let error = Result::<()>::Err(Error::from(ErrorKind::BrokenPipe).into())
            .context("cannot write the rewritten blob to git hash-object")
            .context("fix failed")
            .unwrap_err();
        assert!(!output_pipe_closed(&error));
    }

    #[test]
    fn another_io_failure_is_not_an_output_pipe_closure() {
        let error = Result::<()>::Err(Error::from(ErrorKind::StorageFull).into())
            .context("cannot write standard output")
            .unwrap_err();
        assert!(!output_pipe_closed(&error));
    }

    #[test]
    fn an_error_carrying_no_io_failure_is_not_an_output_pipe_closure() {
        assert!(!output_pipe_closed(&anyhow::anyhow!(
            "plugin `x` is not locked"
        )));
    }
}
