//! The download pipeline: extract -> (download -> convert) per item, in parallel.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::Result;
use crate::model::{AudioFormat, MediaItem};
use crate::pool::run_bounded;
use crate::traits::{Downloader, Extractor, Processor};

/// Settings for one pipeline run.
#[derive(Debug, Clone)]
pub struct Config {
    /// Where finished audio files are written.
    pub output_dir: PathBuf,
    /// Number of items processed at the same time.
    pub jobs: usize,
    pub format: AudioFormat,
    /// Keep the raw downloaded file next to the converted one.
    pub keep_raw: bool,
}

/// What happened to a single item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Downloaded and converted; holds the final path.
    Done(PathBuf),
    /// The final file already existed, so nothing was done.
    Skipped(PathBuf),
    /// Something went wrong; holds a human-readable reason.
    Failed(String),
}

#[derive(Debug)]
pub struct Summary {
    pub results: Vec<(MediaItem, Outcome)>,
}

impl Summary {
    fn count(&self, pred: impl Fn(&Outcome) -> bool) -> usize {
        self.results.iter().filter(|(_, o)| pred(o)).count()
    }
    pub fn done(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Done(_)))
    }
    pub fn skipped(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Skipped(_)))
    }
    pub fn failed(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Failed(_)))
    }
}

/// Called after each item finishes: `(finished_so_far, total, item, outcome)`.
pub type ProgressFn<'a> = &'a (dyn Fn(usize, usize, &MediaItem, &Outcome) + Sync);

/// Runs the whole pipeline for `url`.
///
/// Extraction errors abort the run. Per-item errors do not: they are recorded
/// as [`Outcome::Failed`] so one bad video never ruins a whole playlist.
pub fn run(
    extractor: &dyn Extractor,
    downloader: &dyn Downloader,
    processor: &dyn Processor,
    url: &str,
    config: &Config,
    on_progress: ProgressFn<'_>,
) -> Result<Summary> {
    let items = extractor.extract(url)?;
    let total = items.len();

    let work_dir = config.output_dir.join(".work");
    fs::create_dir_all(&work_dir)?;

    let finished = AtomicUsize::new(0);
    let outcomes = run_bounded(items.clone(), config.jobs, |_, item| {
        let outcome = process_item(downloader, processor, &item, config, &work_dir)
            .unwrap_or_else(|e| Outcome::Failed(e.to_string()));
        let n = finished.fetch_add(1, Ordering::SeqCst) + 1;
        on_progress(n, total, &item, &outcome);
        outcome
    });

    // Best effort: only succeeds when the scratch directory is empty.
    let _ = fs::remove_dir(&work_dir);

    Ok(Summary {
        results: items.into_iter().zip(outcomes).collect(),
    })
}

fn process_item(
    downloader: &dyn Downloader,
    processor: &dyn Processor,
    item: &MediaItem,
    config: &Config,
    work_dir: &Path,
) -> Result<Outcome> {
    let ext = config.format.ext();
    let stem = output_stem(item);
    let final_path = config.output_dir.join(format!("{stem}.{ext}"));
    if final_path.exists() {
        return Ok(Outcome::Skipped(final_path));
    }

    let raw = downloader.download(item, work_dir)?;

    // Convert into a `.partial` file first so an interrupted run never leaves
    // a truncated file that looks finished (and would be skipped next time).
    let partial = config.output_dir.join(format!("{stem}.partial.{ext}"));
    let converted = processor.convert(&raw, &partial, config.format);
    if let Err(e) = converted {
        let _ = fs::remove_file(&partial);
        let _ = fs::remove_file(&raw);
        return Err(e);
    }
    fs::rename(&partial, &final_path)?;

    if config.keep_raw {
        // Move the raw download out of the scratch dir, next to the result.
        let raw_ext = raw.extension().and_then(|e| e.to_str()).unwrap_or("bin");
        let kept = config.output_dir.join(format!("{stem}.raw.{raw_ext}"));
        if fs::rename(&raw, &kept).is_err() {
            let _ = fs::remove_file(&raw);
        }
    } else {
        let _ = fs::remove_file(&raw);
    }
    Ok(Outcome::Done(final_path))
}

/// `Title [id]`: readable, and unique even when two items share a title.
pub fn output_stem(item: &MediaItem) -> String {
    format!("{} [{}]", sanitize_filename(&item.title), item.id)
}

/// Makes a title safe to use as a file name on Linux, macOS and Windows.
pub fn sanitize_filename(name: &str) -> String {
    const MAX_CHARS: usize = 120;
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .take(MAX_CHARS)
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use std::sync::Mutex;

    struct MockExtractor(Vec<MediaItem>);
    impl Extractor for MockExtractor {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn extract(&self, _url: &str) -> Result<Vec<MediaItem>> {
            Ok(self.0.clone())
        }
    }

    /// Writes `<id>.raw`; fails for the id `bad`.
    struct MockDownloader;
    impl Downloader for MockDownloader {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn download(&self, item: &MediaItem, work_dir: &Path) -> Result<PathBuf> {
            if item.id == "bad" {
                return Err(Error::Parse("video unavailable".into()));
            }
            let path = work_dir.join(format!("{}.raw", item.id));
            fs::write(&path, b"raw audio")?;
            Ok(path)
        }
    }

    struct MockProcessor;
    impl Processor for MockProcessor {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn convert(&self, input: &Path, output: &Path, _f: AudioFormat) -> Result<()> {
            fs::copy(input, output)?;
            Ok(())
        }
    }

    fn item(id: &str, title: &str) -> MediaItem {
        MediaItem {
            id: id.into(),
            title: title.into(),
            url: format!("https://example.com/{id}"),
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("audiograb-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn runs_end_to_end_and_skips_on_second_run() {
        let dir = temp_dir("pipeline");
        let config = Config {
            output_dir: dir.clone(),
            jobs: 3,
            format: AudioFormat::Mp3,
            keep_raw: false,
        };
        let extractor = MockExtractor(vec![
            item("a1", "Song A"),
            item("bad", "Broken"),
            item("c3", "Song/C: live"),
        ]);

        let events = Mutex::new(Vec::new());
        let progress = |n: usize, total: usize, it: &MediaItem, _o: &Outcome| {
            events.lock().unwrap().push((n, total, it.id.clone()));
        };

        let summary = run(
            &extractor,
            &MockDownloader,
            &MockProcessor,
            "https://example.com/list",
            &config,
            &progress,
        )
        .unwrap();

        assert_eq!(
            (summary.done(), summary.skipped(), summary.failed()),
            (2, 0, 1)
        );
        assert!(dir.join("Song A [a1].mp3").exists());
        assert!(dir.join("Song_C_ live [c3].mp3").exists());
        // Order of results follows the playlist order.
        assert_eq!(summary.results[1].0.id, "bad");
        assert!(matches!(summary.results[1].1, Outcome::Failed(_)));
        // Progress fired once per item with a consistent total.
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(events.iter().all(|(_, total, _)| *total == 3));
        // No scratch or partial files left behind.
        assert!(!dir.join(".work").exists());
        assert!(!dir.join("Song A [a1].partial.mp3").exists());

        // Second run: finished files are skipped, only the failure is retried.
        let again = run(
            &extractor,
            &MockDownloader,
            &MockProcessor,
            "https://example.com/list",
            &config,
            &|_, _, _, _| {},
        )
        .unwrap();
        assert_eq!((again.done(), again.skipped(), again.failed()), (0, 2, 1));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sanitizes_awkward_titles() {
        assert_eq!(
            sanitize_filename("a/b\\c:d*e?f\"g<h>i|j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(sanitize_filename("  ..hidden.. "), "hidden");
        assert_eq!(sanitize_filename("///"), "___");
        assert_eq!(sanitize_filename("   "), "untitled");
        assert_eq!(sanitize_filename(&"x".repeat(500)).chars().count(), 120);
    }

    #[test]
    fn stem_includes_id_so_duplicate_titles_do_not_collide() {
        assert_ne!(
            output_stem(&item("one", "Same title")),
            output_stem(&item("two", "Same title"))
        );
    }
}
