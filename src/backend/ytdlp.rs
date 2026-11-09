//! `yt-dlp` backend: implements [`Extractor`] and [`Downloader`].

use std::path::{Path, PathBuf};

use crate::backend::{command_failed, run_tool_output};
use crate::error::{Error, Result};
use crate::model::MediaItem;
use crate::traits::{Downloader, Extractor};

/// yt-dlp output template used to list entries: `id<TAB>title<TAB>url`.
///
/// `webpage_url,url` means "use webpage_url, fall back to url": single videos
/// expose `webpage_url`, while flat playlist entries only expose `url`.
const LIST_TEMPLATE: &str = "%(id)s\t%(title)s\t%(webpage_url,url)s";

pub struct YtDlp {
    program: String,
}

impl YtDlp {
    pub fn new() -> Self {
        Self::with_program("yt-dlp")
    }

    /// Use a specific executable (handy for tests and non-standard installs).
    pub fn with_program(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl Default for YtDlp {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for YtDlp {
    fn name(&self) -> &'static str {
        "yt-dlp"
    }

    fn extract(&self, url: &str) -> Result<Vec<MediaItem>> {
        let out = run_tool_output(
            &self.program,
            [
                "--flat-playlist",
                "--ignore-errors",
                "--no-warnings",
                "--print",
                LIST_TEMPLATE,
                url,
            ],
        )?;

        let items = parse_entries(&out.stdout);
        // With --ignore-errors yt-dlp exits non-zero if *any* entry is
        // unavailable. Partial results are still useful, so only fail when we
        // got nothing at all.
        if items.is_empty() {
            return Err(if out.success {
                Error::Parse(format!("no media found at {url}"))
            } else {
                command_failed(&self.program, &out)
            });
        }
        Ok(items)
    }
}

impl Downloader for YtDlp {
    fn name(&self) -> &'static str {
        "yt-dlp"
    }

    fn download(&self, item: &MediaItem, work_dir: &Path) -> Result<PathBuf> {
        let template = work_dir.join("%(id)s.%(ext)s");
        let out = run_tool_output(
            &self.program,
            [
                "--no-playlist",
                "--no-warnings",
                "-f",
                "bestaudio/best",
                "-o",
                &template.to_string_lossy(),
                // Print the final path once the file has been moved into place.
                "--print",
                "after_move:filepath",
                // --print implies --simulate; turn the real download back on.
                "--no-simulate",
                &item.url,
            ],
        )?;
        if !out.success {
            return Err(command_failed(&self.program, &out));
        }

        let path = out
            .stdout
            .lines()
            .map(str::trim)
            .rfind(|l| !l.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| Error::Parse("yt-dlp did not report an output path".into()))?;

        if !path.exists() {
            return Err(Error::Parse(format!(
                "yt-dlp reported `{}` but the file does not exist",
                path.display()
            )));
        }
        Ok(path)
    }
}

/// Parses the tab-separated listing produced with [`LIST_TEMPLATE`].
/// Lines that are malformed or lack an id/url are skipped.
pub(crate) fn parse_entries(stdout: &str) -> Vec<MediaItem> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let id = parts.next()?.trim();
            let title = parts.next()?.trim();
            let url = parts.next()?.trim();
            if is_missing(id) || is_missing(url) {
                return None;
            }
            let title = if is_missing(title) { id } else { title };
            Some(MediaItem {
                id: id.to_string(),
                title: title.to_string(),
                url: url.to_string(),
            })
        })
        .collect()
}

/// yt-dlp prints the literal string `NA` for unavailable fields.
fn is_missing(s: &str) -> bool {
    s.is_empty() || s == "NA"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_playlist_listing() {
        let stdout = "abc123\tFirst song\thttps://www.youtube.com/watch?v=abc123\n\
                      def456\tSecond | song\thttps://www.youtube.com/watch?v=def456\n";
        let items = parse_entries(stdout);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "abc123");
        assert_eq!(items[1].title, "Second | song");
        assert_eq!(items[1].url, "https://www.youtube.com/watch?v=def456");
    }

    #[test]
    fn falls_back_to_id_when_title_missing() {
        let items = parse_entries("xyz\tNA\thttps://example.com/v/xyz\n");
        assert_eq!(items[0].title, "xyz");
    }

    #[test]
    fn skips_malformed_and_unavailable_entries() {
        let stdout = "garbage line\n\
                      NA\tPrivate video\tNA\n\
                      ok1\tGood\thttps://example.com/ok1\n\
                      \n";
        let items = parse_entries(stdout);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "ok1");
    }
}
