use std::fs;
use std::process::ExitCode;

use audiograb::backend::ffmpeg::Ffmpeg;
use audiograb::backend::ytdlp::YtDlp;
use audiograb::cli::{self, Command, DownloadArgs, InspectHlsArgs};
use audiograb::hls::playlist::{self, KeyMethod, Manifest};
use audiograb::hls::uri;
use audiograb::pipeline::{self, Config, Outcome};
use audiograb::{Error, Result};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            if matches!(e, Error::Usage(_)) {
                eprintln!("Try `audiograb --help`.");
            }
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    match cli::parse_args(std::env::args().skip(1))? {
        Command::Help => {
            println!("{}", cli::USAGE);
            Ok(ExitCode::SUCCESS)
        }
        Command::Version => {
            println!("audiograb {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        Command::Download(args) => download(args),
        Command::InspectHls(args) => inspect_hls(args),
    }
}

fn download(args: DownloadArgs) -> Result<ExitCode> {
    fs::create_dir_all(&args.output_dir)?;

    let config = Config {
        output_dir: args.output_dir,
        jobs: args.jobs,
        format: args.format,
        keep_raw: args.keep_raw,
    };

    // Today both download stages are external tools. Swapping in a native
    // implementation later only changes these constructors.
    let ytdlp = YtDlp::new();
    let ffmpeg = Ffmpeg::new();

    println!(
        "Fetching list with {} (up to {} at a time, output: {})...",
        audiograb::traits::Extractor::name(&ytdlp),
        config.jobs,
        config.output_dir.display()
    );

    let summary = pipeline::run(
        &ytdlp,
        &ytdlp,
        &ffmpeg,
        &args.url,
        &config,
        &|n, total, item, outcome| match outcome {
            Outcome::Done(_) => println!("[{n}/{total}] done     {}", item.title),
            Outcome::Skipped(_) => {
                println!("[{n}/{total}] skipped  {} (already exists)", item.title)
            }
            Outcome::Failed(why) => eprintln!("[{n}/{total}] FAILED   {}: {why}", item.title),
        },
    )?;

    println!(
        "\nFinished: {} downloaded, {} skipped, {} failed.",
        summary.done(),
        summary.skipped(),
        summary.failed()
    );

    Ok(if summary.failed() == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn inspect_hls(args: InspectHlsArgs) -> Result<ExitCode> {
    let text = fs::read_to_string(&args.file)?;
    let resolve = |u: &str| match &args.base_url {
        Some(base) => uri::resolve(base, u),
        None => u.to_string(),
    };

    match playlist::parse(&text)? {
        Manifest::Master(master) => {
            println!("Master playlist with {} variant(s):", master.variants.len());
            for (i, v) in master.variants.iter().enumerate() {
                let res = v
                    .resolution
                    .map(|(w, h)| format!("{w}x{h}"))
                    .unwrap_or_else(|| "audio/unknown".into());
                println!(
                    "  #{i}: {:>6} kbps  {res:<12} {}  -> {}",
                    v.bandwidth / 1000,
                    v.codecs.as_deref().unwrap_or("-"),
                    resolve(&v.uri)
                );
            }
        }
        Manifest::Media(media) => {
            println!("Media playlist");
            println!("  segments:        {}", media.segments.len());
            println!("  total duration:  {:.1} s", media.total_duration());
            println!("  target duration: {} s", media.target_duration);
            println!("  first sequence:  {}", media.media_sequence);
            println!("  complete (VOD):  {}", media.ended);
            if let Some(map) = &media.init_map {
                println!("  init section:    {}", resolve(map));
            }
            let encryption = media
                .segments
                .iter()
                .find_map(|s| s.key.as_ref())
                .map(|k| match &k.method {
                    KeyMethod::Aes128 => "AES-128".to_string(),
                    KeyMethod::SampleAes => "SAMPLE-AES".to_string(),
                    KeyMethod::Other(m) => m.clone(),
                })
                .unwrap_or_else(|| "none".into());
            println!("  encryption:      {encryption}");
            for s in media.segments.iter().take(5) {
                println!(
                    "    seq {:>5}  {:>6.2}s  {}",
                    s.sequence,
                    s.duration,
                    resolve(&s.uri)
                );
            }
            if media.segments.len() > 5 {
                println!("    ... and {} more", media.segments.len() - 5);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}
