# audiograb
A fast, concurrent command-line tool that turns a video/playlist URL into a folder of audio files. It starts as a thin, well-structured wrapper around `yt-dlp` and `ffmpeg`, and is designed so that every external piece can be replaced, one at a time, by native Rust code.
