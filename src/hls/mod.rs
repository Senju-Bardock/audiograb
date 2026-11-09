//! Native HLS (HTTP Live Streaming) support: the first piece of audiograb that
//! does not depend on yt-dlp.
//!
//! Done so far:
//! * [`playlist`]: parse master and media `.m3u8` playlists.
//! * [`uri`]: resolve relative segment/key URIs against the playlist URL.
//!
//! Next (phase 2): fetch segments concurrently, decrypt AES-128, and
//! concatenate them into a single stream.

pub mod playlist;
pub mod uri;
