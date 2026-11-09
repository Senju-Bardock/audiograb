# audiograb

A fast, concurrent command-line tool that turns a video/playlist URL into a
folder of audio files. It starts as a thin, well-structured wrapper around
`yt-dlp` and `ffmpeg`, and is designed so that every external piece can be
replaced, one at a time, by native Rust code.

> **Status:** phase 1 (skeleton). Zero third-party dependencies.

## Why this exists

* **Learn Rust on a real problem:** processes, concurrency, error handling,
  parsing, traits, and eventually networking.
* **Keep something that works at every step.** The external tools are the
  reference implementation; each native replacement can be compared against them.
* **Be fast where Rust actually helps:** orchestrating many downloads at once
  with a hard concurrency limit, never blocking, never leaving half-written files.

Note that raw download speed is bound by the network and the remote server, not
by the language. What Rust gives us here is safe, predictable parallelism and a
tiny, quick-starting binary.

## Requirements

* Rust (stable, 2021 edition or newer)
* [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) on your `PATH`
* [`ffmpeg`](https://ffmpeg.org/) on your `PATH` (with `libmp3lame` for MP3 output)

## Build

```sh
cargo build --release
# binary: target/release/audiograb
```

## Usage

```sh
# Download a playlist as MP3, 4 at a time, into ./downloads
audiograb "https://www.youtube.com/playlist?list=..."

# 8 parallel downloads, Opus output, custom folder, keep the raw files
audiograb "https://..." -j 8 -f opus -o ~/Music/gym --keep-raw

# Inspect a local HLS playlist (resolves relative URLs if --base is given)
audiograb inspect-hls index.m3u8 --base https://cdn.example.com/live/index.m3u8
```

| Option | Meaning | Default |
|---|---|---|
| `-o, --output <DIR>` | Output directory | `downloads` |
| `-j, --jobs <N>` | Items processed in parallel | `4` |
| `-f, --format <FMT>` | `mp3`, `m4a` or `opus` | `mp3` |
| `--keep-raw` | Keep the raw downloaded audio next to the result | off |

Behaviour worth knowing:

* Files are named `Title [id].ext`, so two videos with the same title never collide.
* Re-running the same command **skips files that already exist**, so it doubles as "resume".
* One failing video never aborts the rest; the exit code is non-zero if any item failed.
* Conversions are written to `*.partial.*` first and renamed when complete, so an
  interrupted run never leaves a truncated file that looks finished.
* When the source already has the target codec (e.g. YouTube's AAC in an `.m4a`
  container to `m4a`), the audio stream is copied instead of re-encoded: instant and lossless.

## Architecture

```
                 +-----------+     +------------+     +-----------+
   URL  ───────► | Extractor | ──► | Downloader | ──► | Processor | ──► Title [id].mp3
                 +-----------+     +------------+     +-----------+
                  URL → items       item → raw file    raw → final
                       │                  │                  │
   today:            yt-dlp             yt-dlp             ffmpeg
   tomorrow:      native extractors  native HLS/HTTP   native remux/decode
```

```
src/
├── main.rs            CLI entry point
├── cli.rs             argument parsing (hand-rolled, no clap yet)
├── pipeline.rs        extract → (download → convert) per item, in parallel
├── pool.rs            bounded worker pool on std::thread::scope
├── traits.rs          Extractor / Downloader / Processor
├── model.rs           MediaItem, AudioFormat
├── error.rs           one error type for the whole crate
├── backend/
│   ├── mod.rs         shared "run an external tool" helper
│   ├── ytdlp.rs       Extractor + Downloader via yt-dlp
│   └── ffmpeg.rs      Processor via ffmpeg
└── hls/
    ├── playlist.rs    native .m3u8 parser (master + media, AES-128 keys, fMP4 init)
    └── uri.rs         relative URI resolution
```

The three traits in `traits.rs` are the contract. The pipeline only talks to
`&dyn Extractor`, `&dyn Downloader` and `&dyn Processor`, which is what makes
each backend swappable and the pipeline testable with mocks.

### Design decisions

* **No dependencies in phase 1.** It builds offline and fast, and every line is
  ours. Concurrency uses plain threads, which is the right tool while the real
  work happens in child processes. `tokio` and an HTTP client arrive with the
  native downloader in phase 2, when there is actual I/O to multiplex.
* **Synchronous traits for now.** Parallelism lives in the pool, not in the
  trait signatures. `Downloader` is the first candidate to become `async`.
* **Per-item error isolation.** Failures are data (`Outcome::Failed`), not control flow.

## Roadmap

- [x] **Phase 1: skeleton.** Traits, yt-dlp/ffmpeg backends, bounded parallel pipeline, CLI, tests
- [x] Native HLS playlist parser and URI resolver
- [ ] **Phase 2: native network layer**
  - [ ] HTTP client (`reqwest` + `rustls`, or `hyper`) and `tokio` runtime
  - [ ] Concurrent HLS segment fetching with retries and backoff
  - [ ] AES-128 segment decryption
  - [ ] Concatenate segments into a single stream
  - [ ] `HlsDownloader` implementing `Downloader`
- [ ] **Phase 3: native extractors**
  - [ ] Twitch (VODs and clips)
  - [ ] Generic "page with an .m3u8 / direct media URL" extractor
  - [ ] YouTube, last and best-effort (it changes constantly)
- [ ] **Phase 4: native audio processing**
  - [ ] Remux to `.m4a` without re-encoding
  - [ ] Decoding (study `symphonia` first)
  - [ ] Optional: progress bars, TUI (`ratatui`), config file, `clap`
- [ ] Spotify: **metadata only** (its audio is DRM-protected). Read a playlist via
      the official API, then search each track on a supported source.

## Testing

```sh
cargo test
cargo clippy --all-targets
```

The unit tests cover the playlist parser, URI resolution, the worker pool
(ordering and the concurrency limit), argument parsing, `yt-dlp` output parsing,
codec selection, and a full pipeline run against mock backends, including
failure isolation and skip-on-rerun.

## Legal note

Downloading from some services may violate their terms of service and, depending
on the content and your jurisdiction, copyright law. Use this tool only for
content you own, that is freely licensed, or that you are otherwise allowed to
download. You are responsible for how you use it.
