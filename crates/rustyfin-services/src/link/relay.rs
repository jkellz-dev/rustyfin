//! Relaying a linked copy's transcode (roadmap 16b): the address this
//! server offers it at, and the token swap on the way through. The friend's
//! token never reaches a client; the client's own is put in its place.
//!
//! The Jellyfin side offers the address (its `PlaybackInfo`); Rustyfin's
//! API serves it (`GET /api/v1/links/{link}/relay/{*path}`).

/// Where a linked copy's transcode is offered: this server's relay for
/// `link`, followed by the friend's `url` without its token.
pub fn relay_url(link: uuid::Uuid, url: &str) -> String {
    format!("/api/v1/links/{link}/relay{}", without_api_key(url, None))
}

/// The value of an `api_key` (any case) in a query string.
pub fn api_key_of(query: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        k.eq_ignore_ascii_case("api_key").then(|| v.to_string())
    })
}

/// `url` with any `api_key`/`ApiKey` parameter taken out, and `key` (already
/// URL-encoded) appended as `api_key` when given.
pub fn without_api_key(url: &str, key: Option<&str>) -> String {
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let mut kept: Vec<&str> = query
        .split('&')
        .filter(|p| {
            !p.is_empty()
                && !p.split('=').next().is_some_and(|k| {
                    k.eq_ignore_ascii_case("api_key") || k.eq_ignore_ascii_case("apikey")
                })
        })
        .collect();
    let added;
    if let Some(key) = key {
        added = format!("api_key={key}");
        kept.push(&added);
    }
    if kept.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", kept.join("&"))
    }
}

/// Every URI in an HLS playlist (its own lines and `URI="..."` attributes)
/// with the friend's token out and the client's in.
pub fn rewrite_playlist(text: &str, key: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    for line in text.lines() {
        if line.is_empty() {
            out.push('\n');
            continue;
        }
        if !line.starts_with('#') {
            out.push_str(&without_api_key(line, key));
        } else if let Some(start) = line.find("URI=\"") {
            let from = start + 5;
            match line[from..].find('"') {
                Some(len) => {
                    out.push_str(&line[..from]);
                    out.push_str(&without_api_key(&line[from..from + len], key));
                    out.push_str(&line[from + len..]);
                }
                None => out.push_str(line),
            }
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relay_address_drops_the_friends_token() {
        let link = uuid::Uuid::nil();
        assert_eq!(
            relay_url(link, "/videos/a/master.m3u8?MediaSourceId=a&api_key=tok"),
            format!("/api/v1/links/{link}/relay/videos/a/master.m3u8?MediaSourceId=a")
        );
    }

    #[test]
    fn the_friends_token_goes_and_the_clients_comes() {
        assert_eq!(
            without_api_key(
                "/videos/a/master.m3u8?MediaSourceId=a&api_key=tok&x=1",
                None
            ),
            "/videos/a/master.m3u8?MediaSourceId=a&x=1"
        );
        assert_eq!(
            without_api_key("main.m3u8?ApiKey=tok", Some("mine")),
            "main.m3u8?api_key=mine"
        );
        assert_eq!(without_api_key("0.ts", Some("mine")), "0.ts?api_key=mine");
        assert_eq!(api_key_of("a=1&API_KEY=k"), Some("k".into()));
        let list =
            "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4?api_key=tok\"\n#EXTINF:6,\n0.mp4?x=1&api_key=tok\n";
        assert_eq!(
            rewrite_playlist(list, Some("mine")),
            "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4?api_key=mine\"\n#EXTINF:6,\n0.mp4?x=1&api_key=mine\n"
        );
        assert!(!rewrite_playlist(list, None).contains("tok"));
    }
}
