//! Backends that implement the pipeline traits by running external programs.

pub mod ffmpeg;
pub mod ytdlp;

use std::ffi::OsStr;
use std::io;
use std::process::Command;

use crate::error::{Error, Result};

/// Captured result of an external program run.
pub(crate) struct ToolOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `program` with `args` and captures its output.
///
/// A missing executable is reported as [`Error::ToolMissing`] so the CLI can
/// give a helpful message instead of a raw "No such file or directory".
pub(crate) fn run_tool_output<I, S>(program: &str, args: I) -> Result<ToolOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => Error::ToolMissing {
                program: program.to_string(),
            },
            _ => Error::Io(e),
        })?;

    Ok(ToolOutput {
        success: output.status.success(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Like [`run_tool_output`], but turns a non-zero exit into an error and
/// returns only stdout.
pub(crate) fn run_tool<I, S>(program: &str, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let out = run_tool_output(program, args)?;
    if out.success {
        Ok(out.stdout)
    } else {
        Err(command_failed(program, &out))
    }
}

pub(crate) fn command_failed(program: &str, out: &ToolOutput) -> Error {
    Error::CommandFailed {
        program: program.to_string(),
        code: out.code,
        stderr: tail_lines(&out.stderr, 5),
    }
}

/// Last `n` non-empty lines, joined with " | " (tool errors are usually at the end).
fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_only_last_lines() {
        assert_eq!(tail_lines("a\n\nb\nc\nd\n", 2), "c | d");
        assert_eq!(tail_lines("", 3), "");
    }

    #[test]
    fn missing_program_is_reported_clearly() {
        let err = run_tool("definitely-not-a-real-program-xyz", ["--version"]).unwrap_err();
        assert!(matches!(err, Error::ToolMissing { .. }));
    }
}
