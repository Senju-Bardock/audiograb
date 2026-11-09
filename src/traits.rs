//! The three seams of the project.
//!
//! Each trait is one stage of the pipeline. The external-tool backends in
//! [`crate::backend`] implement them today; native Rust implementations can
//! replace them one by one later.
//!
//! The traits are synchronous on purpose in phase 1: concurrency is handled by
//! the worker pool in [`crate::pool`]. When a native async HTTP downloader
//! arrives, `Downloader` is the natural place to switch to `async fn`.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::model::{AudioFormat, MediaItem};

/// URL -> list of items. Handles single videos and playlists alike.
pub trait Extractor: Send + Sync {
    fn name(&self) -> &'static str;
    fn extract(&self, url: &str) -> Result<Vec<MediaItem>>;
}

/// Item -> raw audio file inside `work_dir`. Returns the file's path.
///
/// The raw file may be in any container/codec (m4a, webm/opus, ...); turning it
/// into the user's chosen format is the [`Processor`]'s job.
pub trait Downloader: Send + Sync {
    fn name(&self) -> &'static str;
    fn download(&self, item: &MediaItem, work_dir: &Path) -> Result<PathBuf>;
}

/// Raw audio file -> final audio file in the requested format.
pub trait Processor: Send + Sync {
    fn name(&self) -> &'static str;
    fn convert(&self, input: &Path, output: &Path, format: AudioFormat) -> Result<()>;
}
