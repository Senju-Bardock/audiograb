//! Parser for HLS playlists (RFC 8216): master and media playlists.

use crate::error::{Error, Result};

/// A parsed `.m3u8` file.
#[derive(Debug, Clone, PartialEq)]
pub enum Manifest {
    /// Lists alternative renditions ("variants") of the same content.
    Master(Master),
    /// Lists the actual media segments.
    Media(Media),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Master {
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub uri: String,
    /// Peak bits per second (0 if the playlist omits it).
    pub bandwidth: u64,
    pub resolution: Option<(u32, u32)>,
    pub codecs: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeyMethod {
    Aes128,
    SampleAes,
    Other(String),
}

/// Encryption info from `#EXT-X-KEY`.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub method: KeyMethod,
    pub uri: Option<String>,
    /// Explicit IV if the playlist provides one.
    pub iv: Option<[u8; 16]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub uri: String,
    /// Duration in seconds.
    pub duration: f64,
    /// Media sequence number of this segment.
    pub sequence: u64,
    /// Key in effect for this segment, if it is encrypted.
    pub key: Option<Key>,
}

impl Segment {
    /// The AES-128 IV for this segment: the explicit one if given, otherwise
    /// the segment's sequence number as a 128-bit big-endian integer (RFC 8216
    /// section 5.2).
    pub fn iv(&self) -> [u8; 16] {
        self.key
            .as_ref()
            .and_then(|k| k.iv)
            .unwrap_or_else(|| default_iv(self.sequence))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Media {
    pub target_duration: u64,
    pub media_sequence: u64,
    /// Initialization section (`#EXT-X-MAP`), used by fragmented MP4.
    pub init_map: Option<String>,
    pub segments: Vec<Segment>,
    /// `true` once `#EXT-X-ENDLIST` is seen (VOD or finished live stream).
    pub ended: bool,
}

impl Media {
    pub fn total_duration(&self) -> f64 {
        self.segments.iter().map(|s| s.duration).sum()
    }
}

pub fn default_iv(sequence: u64) -> [u8; 16] {
    let mut iv = [0u8; 16];
    iv[8..].copy_from_slice(&sequence.to_be_bytes());
    iv
}

/// Parses playlist text. URIs are kept exactly as written; resolve them with
/// [`crate::hls::uri::resolve`].
pub fn parse(text: &str) -> Result<Manifest> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or("");
    if first.trim_start_matches('\u{feff}') != "#EXTM3U" {
        return Err(Error::Parse("missing #EXTM3U header".into()));
    }

    let mut master = Master::default();
    let mut media = Media::default();
    let mut pending_variant: Option<Variant> = None;
    let mut pending_duration: Option<f64> = None;
    let mut key: Option<Key> = None;

    for line in lines {
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let attrs = parse_attributes(rest);
            pending_variant = Some(Variant {
                uri: String::new(),
                bandwidth: attr(&attrs, "BANDWIDTH")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                resolution: attr(&attrs, "RESOLUTION").and_then(parse_resolution),
                codecs: attr(&attrs, "CODECS").map(str::to_string),
            });
        } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let raw = rest.split(',').next().unwrap_or("").trim();
            let duration = raw
                .parse::<f64>()
                .map_err(|_| Error::Parse(format!("invalid #EXTINF duration `{raw}`")))?;
            pending_duration = Some(duration);
        } else if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            key = parse_key(rest);
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            media.init_map = attr(&parse_attributes(rest), "URI").map(str::to_string);
        } else if let Some(rest) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            media.target_duration = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            media.media_sequence = rest.trim().parse().unwrap_or(0);
        } else if line == "#EXT-X-ENDLIST" {
            media.ended = true;
        } else if line.starts_with('#') {
            // Unknown tag or comment: ignored, as the spec requires.
        } else if let Some(mut variant) = pending_variant.take() {
            variant.uri = line.to_string();
            master.variants.push(variant);
        } else if let Some(duration) = pending_duration.take() {
            media.segments.push(Segment {
                uri: line.to_string(),
                duration,
                sequence: media.media_sequence + media.segments.len() as u64,
                key: key.clone(),
            });
        }
    }

    if !master.variants.is_empty() {
        Ok(Manifest::Master(master))
    } else if !media.segments.is_empty() || media.ended {
        Ok(Manifest::Media(media))
    } else {
        Err(Error::Parse(
            "playlist contains neither variants nor segments".into(),
        ))
    }
}

fn parse_key(rest: &str) -> Option<Key> {
    let attrs = parse_attributes(rest);
    let method = match attr(&attrs, "METHOD")? {
        "NONE" => return None,
        "AES-128" => KeyMethod::Aes128,
        "SAMPLE-AES" => KeyMethod::SampleAes,
        other => KeyMethod::Other(other.to_string()),
    };
    Some(Key {
        method,
        uri: attr(&attrs, "URI").map(str::to_string),
        iv: attr(&attrs, "IV").and_then(parse_iv),
    })
}

/// Parses an `IV` attribute such as `0x000102...0f` (32 hex digits).
fn parse_iv(s: &str) -> Option<[u8; 16]> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    if hex.len() != 32 || !hex.is_ascii() {
        return None;
    }
    let mut iv = [0u8; 16];
    for (i, byte) in iv.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(iv)
}

fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

/// Parses `KEY=value,KEY2="quoted, value"` attribute lists. Keys are
/// upper-cased; surrounding quotes are removed from values.
fn parse_attributes(input: &str) -> Vec<(String, String)> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for c in input.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                current.push(c);
            }
            ',' if !in_quotes => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }

    parts
        .iter()
        .filter_map(|part| {
            let (k, v) = part.split_once('=')?;
            Some((
                k.trim().to_ascii_uppercase(),
                v.trim().trim_matches('"').to_string(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTER: &str = "#EXTM3U\n\
        #EXT-X-VERSION:3\n\
        #EXT-X-STREAM-INF:BANDWIDTH=128000,CODECS=\"mp4a.40.2,avc1.4d401f\",RESOLUTION=1280x720\n\
        720p/index.m3u8\n\
        #EXT-X-STREAM-INF:BANDWIDTH=64000,CODECS=\"mp4a.40.5\"\n\
        audio_only/index.m3u8\n";

    const MEDIA: &str = "#EXTM3U\n\
        #EXT-X-VERSION:3\n\
        #EXT-X-TARGETDURATION:10\n\
        #EXT-X-MEDIA-SEQUENCE:7\n\
        #EXT-X-KEY:METHOD=AES-128,URI=\"keys/k1.bin\",IV=0x000102030405060708090a0b0c0d0e0f\n\
        #EXTINF:9.009,\n\
        seg7.ts\n\
        #EXTINF:9.009,title here\n\
        seg8.ts\n\
        #EXT-X-KEY:METHOD=NONE\n\
        #EXTINF:4.5,\n\
        seg9.ts\n\
        #EXT-X-ENDLIST\n";

    #[test]
    fn parses_master_playlist() {
        let Manifest::Master(m) = parse(MASTER).unwrap() else {
            panic!("expected master playlist");
        };
        assert_eq!(m.variants.len(), 2);
        assert_eq!(m.variants[0].uri, "720p/index.m3u8");
        assert_eq!(m.variants[0].bandwidth, 128000);
        assert_eq!(m.variants[0].resolution, Some((1280, 720)));
        // The comma inside the quoted CODECS value must not split the attribute.
        assert_eq!(
            m.variants[0].codecs.as_deref(),
            Some("mp4a.40.2,avc1.4d401f")
        );
        assert_eq!(m.variants[1].resolution, None);
    }

    #[test]
    fn parses_media_playlist() {
        let Manifest::Media(m) = parse(MEDIA).unwrap() else {
            panic!("expected media playlist");
        };
        assert_eq!(m.target_duration, 10);
        assert_eq!(m.media_sequence, 7);
        assert!(m.ended);
        assert_eq!(m.segments.len(), 3);
        assert_eq!(m.segments[0].uri, "seg7.ts");
        assert_eq!(m.segments[2].sequence, 9);
        assert!((m.total_duration() - 22.518).abs() < 1e-9);
    }

    #[test]
    fn key_applies_until_replaced() {
        let Manifest::Media(m) = parse(MEDIA).unwrap() else {
            panic!()
        };
        let k = m.segments[0].key.as_ref().expect("segment 0 is encrypted");
        assert_eq!(k.method, KeyMethod::Aes128);
        assert_eq!(k.uri.as_deref(), Some("keys/k1.bin"));
        assert!(m.segments[1].key.is_some());
        assert!(m.segments[2].key.is_none(), "METHOD=NONE clears the key");
    }

    #[test]
    fn explicit_iv_wins_over_sequence_default() {
        let Manifest::Media(m) = parse(MEDIA).unwrap() else {
            panic!()
        };
        let iv = m.segments[0].iv();
        assert_eq!(iv[0], 0x00);
        assert_eq!(iv[15], 0x0f);
    }

    #[test]
    fn default_iv_is_big_endian_sequence_number() {
        let iv = default_iv(0x0102);
        assert_eq!(&iv[..14], &[0u8; 14]);
        assert_eq!(iv[14], 0x01);
        assert_eq!(iv[15], 0x02);
    }

    #[test]
    fn parses_fragmented_mp4_init_section() {
        let text = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:2.0,\nseg.m4s\n#EXT-X-ENDLIST\n";
        let Manifest::Media(m) = parse(text).unwrap() else {
            panic!()
        };
        assert_eq!(m.init_map.as_deref(), Some("init.mp4"));
    }

    #[test]
    fn tolerates_bom_crlf_and_unknown_tags() {
        let text = "\u{feff}#EXTM3U\r\n#EXT-X-FUTURE-TAG:whatever\r\n#EXTINF:1.0,\r\na.ts\r\n";
        assert!(matches!(parse(text).unwrap(), Manifest::Media(_)));
    }

    #[test]
    fn rejects_non_playlists() {
        assert!(parse("<html>not a playlist</html>").is_err());
        assert!(parse("#EXTM3U\n").is_err());
        assert!(parse("#EXTM3U\n#EXTINF:abc,\nx.ts\n").is_err());
    }

    #[test]
    fn rejects_malformed_iv() {
        assert_eq!(parse_iv("0x1234"), None);
        assert_eq!(parse_iv("000102030405060708090a0b0c0d0e0f"), None);
        assert!(parse_iv("0x000102030405060708090a0b0c0d0e0f").is_some());
    }
}
