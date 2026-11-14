//! How the admin's built files are sent: caching and compression.
//!
//! Trunk names the JS, WASM and CSS it builds with a content hash
//! (`rustyfin-admin-d2cf9e2fd33a5abd.js`), so those can be cached for a year and
//! never asked for again; a new build has new names. `index.html` names them,
//! so it is always revalidated (cheaply, by ETag). Text and WASM go out Brotli-
//! or gzip-compressed, compressed once per file and kept in memory.

/// How a response body is encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Encoding {
    Brotli,
    Gzip,
    Identity,
}

impl Encoding {
    /// The `Content-Encoding` value, if any.
    pub fn header_value(self) -> Option<&'static str> {
        match self {
            Encoding::Brotli => Some("br"),
            Encoding::Gzip => Some("gzip"),
            Encoding::Identity => None,
        }
    }
}

/// `Cache-Control` for an embedded file.
pub fn cache_control(path: &str) -> &'static str {
    if path == "index.html" {
        "no-cache"
    } else if is_fingerprinted(path) {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    }
}

/// Whether trunk named this file with a content hash.
pub fn is_fingerprinted(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.split('.').next().unwrap_or(name);
    let stem = stem.strip_suffix("_bg").unwrap_or(stem);
    stem.rsplit_once('-')
        .is_some_and(|(_, hash)| hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The best encoding the client accepts: Brotli, then gzip, else none.
pub fn choose_encoding(accept_encoding: Option<&str>) -> Encoding {
    let Some(header) = accept_encoding else {
        return Encoding::Identity;
    };
    let accepts = |name: &str| {
        header.split(',').any(|part| {
            let mut bits = part.split(';').map(str::trim);
            let coding = bits.next().unwrap_or_default();
            let refused = bits.any(|p| {
                p.strip_prefix("q=")
                    .and_then(|q| q.parse::<f32>().ok())
                    .is_some_and(|q| q <= 0.0)
            });
            coding.eq_ignore_ascii_case(name) && !refused
        })
    };
    if accepts("br") {
        Encoding::Brotli
    } else if accepts("gzip") {
        Encoding::Gzip
    } else {
        Encoding::Identity
    }
}

/// Whether compressing this type pays off (text and WASM; images do not).
pub fn is_compressible(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/wasm" | "application/javascript" | "application/json" | "image/svg+xml"
        )
}

/// `data` in `encoding`.
pub fn compress(data: &[u8], encoding: Encoding) -> Vec<u8> {
    use std::io::Write;
    match encoding {
        Encoding::Brotli => {
            let mut out = Vec::new();
            {
                // Quality 11 (the best): each file is compressed once.
                let mut w = brotli::CompressorWriter::new(&mut out, 4096, 11, 22);
                w.write_all(data).expect("writing to memory");
            }
            out
        }
        Encoding::Gzip => {
            let mut w = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            w.write_all(data).expect("writing to memory");
            w.finish().expect("writing to memory")
        }
        Encoding::Identity => data.to_vec(),
    }
}

/// A strong ETag from a file's SHA-256.
pub fn etag(sha256: &[u8; 32]) -> String {
    let hex: String = sha256[..12].iter().map(|b| format!("{b:02x}")).collect();
    format!("\"{hex}\"")
}

/// Whether an `If-None-Match` header names `etag` (or `*`).
pub fn none_match(if_none_match: Option<&str>, etag: &str) -> bool {
    let Some(header) = if_none_match else {
        return false;
    };
    header
        .split(',')
        .map(str::trim)
        .any(|c| c == "*" || c.trim_start_matches("W/") == etag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn hashed_build_files_are_cached_for_a_year_and_index_is_revalidated() {
        let year = "public, max-age=31536000, immutable";
        assert_eq!(cache_control("rustyfin-admin-d2cf9e2fd33a5abd.js"), year);
        assert_eq!(
            cache_control("rustyfin-admin-e4c84b564a2415a8_bg.wasm"),
            year
        );
        assert_eq!(cache_control("style-59f8e714ecd7adf6.css"), year);
        assert_eq!(cache_control("index.html"), "no-cache");
        assert_eq!(cache_control("favicon.svg"), "public, max-age=3600");
        assert!(!is_fingerprinted("style.css"));
        assert!(!is_fingerprinted("my-component.js"));
        assert!(!is_fingerprinted("x-d2cf9e2fd33a5abz.js"), "not hex");
    }

    #[test]
    fn brotli_is_preferred_then_gzip() {
        assert_eq!(
            choose_encoding(Some("gzip, deflate, br, zstd")),
            Encoding::Brotli
        );
        assert_eq!(choose_encoding(Some("gzip, deflate")), Encoding::Gzip);
        assert_eq!(choose_encoding(Some("br;q=0, gzip")), Encoding::Gzip);
        assert_eq!(choose_encoding(Some("identity")), Encoding::Identity);
        assert_eq!(choose_encoding(None), Encoding::Identity);
        assert_eq!(Encoding::Brotli.header_value(), Some("br"));
        assert_eq!(Encoding::Gzip.header_value(), Some("gzip"));
        assert_eq!(Encoding::Identity.header_value(), None);
    }

    #[test]
    fn text_and_wasm_are_compressed_images_are_not() {
        for m in [
            "application/wasm",
            "text/javascript",
            "application/javascript",
            "text/css",
            "text/html",
            "image/svg+xml",
        ] {
            assert!(is_compressible(m), "{m}");
        }
        for m in ["image/png", "image/jpeg", "application/octet-stream"] {
            assert!(!is_compressible(m), "{m}");
        }
    }

    #[test]
    fn compressed_bodies_decode_to_the_original() {
        let data = b"function f() { return 42; }\n".repeat(200);
        let gz = compress(&data, Encoding::Gzip);
        assert!(gz.len() < data.len() / 4);
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&gz[..])
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(out, data);

        let br = compress(&data, Encoding::Brotli);
        assert!(br.len() < data.len() / 4);
        let mut out = Vec::new();
        brotli::Decompressor::new(&br[..], 4096)
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(out, data);

        assert_eq!(compress(&data, Encoding::Identity), data);
    }

    #[test]
    fn etags_match_if_none_match() {
        let tag = etag(&[0xab; 32]);
        assert!(tag.starts_with('"') && tag.ends_with('"'), "{tag}");
        assert!(none_match(Some(&tag), &tag));
        assert!(none_match(Some(&format!("\"other\", {tag}")), &tag));
        assert!(none_match(Some(&format!("W/{tag}")), &tag));
        assert!(none_match(Some("*"), &tag));
        assert!(!none_match(Some("\"other\""), &tag));
        assert!(!none_match(None, &tag));
    }
}
