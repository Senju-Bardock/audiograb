use std::str::FromStr;

use crate::error::Error;

/// One downloadable item (a video/track) discovered by an extractor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaItem {
    /// Stable identifier from the source site (e.g. a YouTube video id).
    pub id: String,
    /// Human-readable title; falls back to the id when the site gives none.
    pub title: String,
    /// URL that a downloader can fetch this single item from.
    pub url: String,
}

/// Output audio formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Mp3,
    M4a,
    Opus,
}

impl AudioFormat {
    /// File extension without the leading dot.
    pub fn ext(self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::M4a => "m4a",
            AudioFormat::Opus => "opus",
        }
    }
}

impl FromStr for AudioFormat {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "mp3" => Ok(AudioFormat::Mp3),
            "m4a" | "aac" => Ok(AudioFormat::M4a),
            "opus" => Ok(AudioFormat::Opus),
            other => Err(Error::Usage(format!(
                "unknown format `{other}` (expected mp3, m4a or opus)"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_formats_case_insensitively() {
        assert_eq!("MP3".parse::<AudioFormat>().unwrap(), AudioFormat::Mp3);
        assert_eq!("aac".parse::<AudioFormat>().unwrap(), AudioFormat::M4a);
        assert_eq!("opus".parse::<AudioFormat>().unwrap().ext(), "opus");
    }

    #[test]
    fn rejects_unknown_format() {
        assert!("flac".parse::<AudioFormat>().is_err());
    }
}
