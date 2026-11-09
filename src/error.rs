use std::fmt;
use std::io;

/// Every error the library can produce.
#[derive(Debug)]
pub enum Error {
    /// A filesystem or process I/O failure.
    Io(io::Error),
    /// An external program (e.g. `yt-dlp`) was not found on `PATH`.
    ToolMissing { program: String },
    /// An external program ran but exited with a failure status.
    CommandFailed {
        program: String,
        code: Option<i32>,
        stderr: String,
    },
    /// Input (tool output, playlist text, ...) could not be understood.
    Parse(String),
    /// Bad command-line usage.
    Usage(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::ToolMissing { program } => {
                write!(f, "`{program}` was not found on PATH; please install it")
            }
            Error::CommandFailed {
                program,
                code,
                stderr,
            } => {
                match code {
                    Some(c) => write!(f, "`{program}` exited with status {c}")?,
                    None => write!(f, "`{program}` was terminated by a signal")?,
                }
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                Ok(())
            }
            Error::Parse(msg) => write!(f, "parse error: {msg}"),
            Error::Usage(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
