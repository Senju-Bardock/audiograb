//! `ffmpeg` backend: implements [`Processor`].

use std::path::Path;

use crate::backend::run_tool;
use crate::error::Result;
use crate::model::AudioFormat;
use crate::traits::Processor;

pub struct Ffmpeg {
    program: String,
}

impl Ffmpeg {
    pub fn new() -> Self {
        Self::with_program("ffmpeg")
    }

    pub fn with_program(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl Default for Ffmpeg {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for Ffmpeg {
    fn name(&self) -> &'static str {
        "ffmpeg"
    }

    fn convert(&self, input: &Path, output: &Path, format: AudioFormat) -> Result<()> {
        let input_ext = input
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();

        let mut args: Vec<&std::ffi::OsStr> = Vec::new();
        let fixed = ["-nostdin", "-y", "-hide_banner", "-loglevel", "error", "-i"];
        args.extend(fixed.iter().map(std::ffi::OsStr::new));
        args.push(input.as_os_str());
        args.extend(
            ["-vn", "-map_metadata", "0"]
                .iter()
                .map(std::ffi::OsStr::new),
        );
        args.extend(
            codec_args(input_ext, format)
                .iter()
                .map(std::ffi::OsStr::new),
        );
        args.push(output.as_os_str());

        run_tool(&self.program, args)?;
        Ok(())
    }
}

/// Picks the codec arguments for a conversion.
///
/// Whenever the source already holds the target codec we copy the stream
/// instead of re-encoding: it is instant and lossless. This assumes `webm`
/// audio is Opus, which is true for YouTube's audio-only streams.
pub(crate) fn codec_args(input_ext: &str, format: AudioFormat) -> &'static [&'static str] {
    let ext = input_ext.to_ascii_lowercase();
    match format {
        AudioFormat::Mp3 => &["-c:a", "libmp3lame", "-q:a", "2"],
        AudioFormat::M4a => {
            if matches!(ext.as_str(), "m4a" | "mp4" | "aac") {
                &["-c:a", "copy"]
            } else {
                &["-c:a", "aac", "-b:a", "192k"]
            }
        }
        AudioFormat::Opus => {
            if matches!(ext.as_str(), "opus" | "webm" | "ogg") {
                &["-c:a", "copy"]
            } else {
                &["-c:a", "libopus", "-b:a", "128k"]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp3_always_reencodes() {
        assert!(codec_args("webm", AudioFormat::Mp3).contains(&"libmp3lame"));
        assert!(codec_args("m4a", AudioFormat::Mp3).contains(&"libmp3lame"));
    }

    #[test]
    fn copies_when_codec_already_matches() {
        assert_eq!(codec_args("m4a", AudioFormat::M4a), &["-c:a", "copy"]);
        assert_eq!(codec_args("WEBM", AudioFormat::Opus), &["-c:a", "copy"]);
    }

    #[test]
    fn reencodes_when_codec_differs() {
        assert!(codec_args("webm", AudioFormat::M4a).contains(&"aac"));
        assert!(codec_args("m4a", AudioFormat::Opus).contains(&"libopus"));
    }
}
