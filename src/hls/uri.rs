//! Minimal URI reference resolution (RFC 3986 section 5, simplified).
//!
//! Playlists usually reference segments and keys with relative URIs. This
//! resolves them against the playlist's own URL, including `..` and `.`
//! segments. It is intentionally small and dependency-free.

/// Resolves `reference` against the absolute URL `base`.
///
/// If `base` is not an absolute `scheme://host/...` URL, `reference` is
/// returned unchanged.
pub fn resolve(base: &str, reference: &str) -> String {
    if reference.contains("://") {
        return reference.to_string();
    }
    let Some((scheme, rest)) = base.split_once("://") else {
        return reference.to_string();
    };
    let (host, base_path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    // Protocol-relative: `//cdn.example.com/a.ts`
    if let Some(authority_and_path) = reference.strip_prefix("//") {
        return format!("{scheme}://{authority_and_path}");
    }

    // Separate the path part of the reference from its ?query / #fragment.
    let split_at = reference.find(['?', '#']).unwrap_or(reference.len());
    let (ref_path, suffix) = reference.split_at(split_at);

    let mut segments: Vec<&str> = if ref_path.starts_with('/') {
        Vec::new()
    } else {
        // Directory of the base path, ignoring the base's own query/fragment.
        let base_path = base_path.split(['?', '#']).next().unwrap_or("/");
        let dir_end = base_path.rfind('/').map_or(0, |i| i + 1);
        base_path[..dir_end]
            .split('/')
            .filter(|s| !s.is_empty())
            .collect()
    };

    for part in ref_path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            p => segments.push(p),
        }
    }

    format!("{scheme}://{host}/{}{suffix}", segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://cdn.example.com/live/audio/index.m3u8?token=abc";

    #[test]
    fn absolute_reference_is_unchanged() {
        assert_eq!(
            resolve(BASE, "https://other.com/x.ts"),
            "https://other.com/x.ts"
        );
    }

    #[test]
    fn relative_to_playlist_directory() {
        assert_eq!(
            resolve(BASE, "seg1.ts"),
            "https://cdn.example.com/live/audio/seg1.ts"
        );
        assert_eq!(
            resolve(BASE, "./sub/seg2.ts"),
            "https://cdn.example.com/live/audio/sub/seg2.ts"
        );
    }

    #[test]
    fn parent_directory_segments() {
        assert_eq!(
            resolve(BASE, "../video/seg3.ts"),
            "https://cdn.example.com/live/video/seg3.ts"
        );
        // Cannot climb above the root.
        assert_eq!(
            resolve(BASE, "../../../../x.ts"),
            "https://cdn.example.com/x.ts"
        );
    }

    #[test]
    fn root_relative_and_protocol_relative() {
        assert_eq!(
            resolve(BASE, "/keys/k.bin"),
            "https://cdn.example.com/keys/k.bin"
        );
        assert_eq!(
            resolve(BASE, "//img.example.com/a.ts"),
            "https://img.example.com/a.ts"
        );
    }

    #[test]
    fn keeps_reference_query_string() {
        assert_eq!(
            resolve(BASE, "seg.ts?sig=a/b"),
            "https://cdn.example.com/live/audio/seg.ts?sig=a/b"
        );
    }

    #[test]
    fn base_without_path() {
        assert_eq!(
            resolve("https://example.com", "a.ts"),
            "https://example.com/a.ts"
        );
    }

    #[test]
    fn non_absolute_base_returns_reference() {
        assert_eq!(resolve("local/index.m3u8", "a.ts"), "a.ts");
    }
}
