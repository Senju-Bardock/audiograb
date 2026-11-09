//! audiograb: a fast, concurrent audio downloader.
//!
//! The library is organised around three small traits (see [`traits`]):
//!
//! * [`traits::Extractor`]: turns a URL into a list of [`model::MediaItem`]s.
//! * [`traits::Downloader`]: fetches one item's raw audio to disk.
//! * [`traits::Processor`]: converts a raw file into the requested format.
//!
//! Today every trait is implemented by shelling out to `yt-dlp` / `ffmpeg`
//! (see [`backend`]). Each implementation can be replaced independently by a
//! native Rust one without touching the pipeline or the CLI.

pub mod backend;
pub mod cli;
pub mod error;
pub mod hls;
pub mod model;
pub mod pipeline;
pub mod pool;
pub mod traits;

pub use error::{Error, Result};
