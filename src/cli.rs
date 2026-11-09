//! Hand-rolled argument parsing (no `clap` yet: phase 1 has zero dependencies).

use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::model::AudioFormat;

pub const USAGE: &str = "\
audiograb: fast, concurrent audio downloader

USAGE:
    audiograb <URL> [OPTIONS]
    audiograb inspect-hls <FILE> [--base <URL>]

OPTIONS:
    -o, --output <DIR>     Output directory            [default: downloads]
    -j, --jobs <N>         Parallel downloads          [default: 4]
    -f, --format <FMT>     mp3, m4a or opus            [default: mp3]
        --keep-raw         Keep the raw downloaded audio files
    -h, --help             Show this help
    -V, --version          Show the version

SUBCOMMANDS:
    inspect-hls            Parse a local .m3u8 playlist and print what is in it

Requires `yt-dlp` and `ffmpeg` on PATH.";

#[derive(Debug, PartialEq)]
pub enum Command {
    Download(DownloadArgs),
    InspectHls(InspectHlsArgs),
    Help,
    Version,
}

#[derive(Debug, PartialEq)]
pub struct DownloadArgs {
    pub url: String,
    pub output_dir: PathBuf,
    pub jobs: usize,
    pub format: AudioFormat,
    pub keep_raw: bool,
}

#[derive(Debug, PartialEq)]
pub struct InspectHlsArgs {
    pub file: PathBuf,
    /// If given, relative URIs in the playlist are resolved against it.
    pub base_url: Option<String>,
}

pub fn parse_args<I>(args: I) -> Result<Command>
where
    I: IntoIterator<Item = String>,
{
    // Normalise `--key=value` into `--key value`.
    let mut tokens: Vec<String> = Vec::new();
    for arg in args {
        match arg.strip_prefix("--").and_then(|r| r.split_once('=')) {
            Some((key, value)) => {
                tokens.push(format!("--{key}"));
                tokens.push(value.to_string());
            }
            None => tokens.push(arg),
        }
    }

    let mut it = tokens.into_iter().peekable();

    if it.peek().map(String::as_str) == Some("inspect-hls") {
        it.next();
        return parse_inspect(it);
    }

    let mut url: Option<String> = None;
    let mut output_dir = PathBuf::from("downloads");
    let mut jobs = 4usize;
    let mut format = AudioFormat::Mp3;
    let mut keep_raw = false;

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "-o" | "--output" => output_dir = PathBuf::from(value(&mut it, &arg)?),
            "-j" | "--jobs" => {
                let raw = value(&mut it, &arg)?;
                jobs = raw
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| {
                        Error::Usage(format!("{arg} expects a positive number, got `{raw}`"))
                    })?;
            }
            "-f" | "--format" => format = value(&mut it, &arg)?.parse()?,
            "--keep-raw" => keep_raw = true,
            flag if flag.starts_with('-') && flag.len() > 1 => {
                return Err(Error::Usage(format!("unknown option `{flag}`")))
            }
            _ => {
                if url.replace(arg).is_some() {
                    return Err(Error::Usage("only one URL can be given".into()));
                }
            }
        }
    }

    let url = url.ok_or_else(|| Error::Usage("missing URL".into()))?;
    Ok(Command::Download(DownloadArgs {
        url,
        output_dir,
        jobs,
        format,
        keep_raw,
    }))
}

fn parse_inspect(mut it: impl Iterator<Item = String>) -> Result<Command> {
    let mut file: Option<PathBuf> = None;
    let mut base_url = None;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--base" => base_url = Some(value(&mut it, &arg)?),
            flag if flag.starts_with('-') && flag.len() > 1 => {
                return Err(Error::Usage(format!("unknown option `{flag}`")))
            }
            _ => {
                if file.replace(PathBuf::from(&arg)).is_some() {
                    return Err(Error::Usage("only one playlist file can be given".into()));
                }
            }
        }
    }
    let file = file.ok_or_else(|| Error::Usage("inspect-hls needs a playlist file".into()))?;
    Ok(Command::InspectHls(InspectHlsArgs { file, base_url }))
}

fn value(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    it.next()
        .ok_or_else(|| Error::Usage(format!("{flag} requires a value")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn defaults() {
        let Command::Download(a) = parse(&["https://youtu.be/x"]).unwrap() else {
            panic!()
        };
        assert_eq!(a.url, "https://youtu.be/x");
        assert_eq!(a.jobs, 4);
        assert_eq!(a.format, AudioFormat::Mp3);
        assert_eq!(a.output_dir, PathBuf::from("downloads"));
        assert!(!a.keep_raw);
    }

    #[test]
    fn all_options_in_both_styles() {
        let Command::Download(a) =
            parse(&["-o", "music", "--jobs=8", "-f", "opus", "--keep-raw", "URL"]).unwrap()
        else {
            panic!()
        };
        assert_eq!(a.output_dir, PathBuf::from("music"));
        assert_eq!(a.jobs, 8);
        assert_eq!(a.format, AudioFormat::Opus);
        assert!(a.keep_raw);
        assert_eq!(a.url, "URL");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["URL", "OTHER"]).is_err());
        assert!(parse(&["URL", "-j", "0"]).is_err());
        assert!(parse(&["URL", "-j", "many"]).is_err());
        assert!(parse(&["URL", "-f", "wav"]).is_err());
        assert!(parse(&["URL", "-o"]).is_err());
        assert!(parse(&["URL", "--bogus"]).is_err());
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse(&["--help"]).unwrap(), Command::Help);
        assert_eq!(parse(&["URL", "-V"]).unwrap(), Command::Version);
    }

    #[test]
    fn inspect_hls_subcommand() {
        let Command::InspectHls(a) = parse(&[
            "inspect-hls",
            "list.m3u8",
            "--base=https://cdn.test/a/b.m3u8",
        ])
        .unwrap() else {
            panic!()
        };
        assert_eq!(a.file, PathBuf::from("list.m3u8"));
        assert_eq!(a.base_url.as_deref(), Some("https://cdn.test/a/b.m3u8"));
        assert!(parse(&["inspect-hls"]).is_err());
    }
}
