//! Concurrent playback: `--streams 1,4,8` plays a generated film to that many
//! clients at once, first by direct play, then transcoded to HLS.
//!
//! A direct-play client downloads the whole file as fast as the server sends
//! it; how many times real time it got is what matters. A transcoding client
//! plays like a player: it starts when the first segment arrives, keeps at
//! most [`BUFFER_AHEAD`] seconds buffered, and stalls whenever a segment
//! arrives after playback reached it.

use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use reqwest::Url;
use serde_json::{Value, json};

use crate::report::StreamRun;
use crate::stats;

/// The most a transcoding client buffers ahead of playback (seconds).
pub const BUFFER_AHEAD: f64 = 30.0;

/// The film every stream plays: H.264 1080p24, generated with ffmpeg.
pub struct Film {
    pub item_id: String,
    pub seconds: f64,
    pub bytes: u64,
}

/// A segment in a media playlist: its address and length (seconds).
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub url: Url,
    pub seconds: f64,
}

/// The first variant a master playlist lists.
pub fn variant_url(master: &str, base: &Url) -> anyhow::Result<Url> {
    let line = master
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .context("master playlist lists no variant")?;
    Ok(base.join(line)?)
}

/// Every segment of a media playlist, in order.
pub fn segments(playlist: &str, base: &Url) -> anyhow::Result<Vec<Segment>> {
    let mut out = Vec::new();
    let mut length = None;
    for line in playlist.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let secs = rest.split(',').next().unwrap_or_default().trim();
            length = Some(secs.parse::<f64>().context("EXTINF length")?);
        } else if !line.is_empty() && !line.starts_with('#') {
            let seconds = length.take().context("segment without #EXTINF")?;
            out.push(Segment {
                url: base.join(line)?,
                seconds,
            });
        }
    }
    Ok(out)
}

/// A player's clock: playback starts when the first segment arrives and
/// stops (stalls) whenever the next one is not there yet.
#[derive(Debug, Default)]
pub struct Player {
    started: Option<f64>,
    stalled: f64,
    pub stalls: usize,
}

impl Player {
    /// When playback reaches media time `at` (seconds since the client
    /// started), or `None` before playback has begun.
    pub fn reaches(&self, at: f64) -> Option<f64> {
        self.started.map(|s| s + self.stalled + at)
    }

    /// The segment starting at media time `at` arrived at `now`.
    pub fn arrived(&mut self, at: f64, now: f64) {
        match self.reaches(at) {
            None => self.started = Some(now),
            Some(due) if now > due => {
                self.stalls += 1;
                self.stalled += now - due;
            }
            Some(_) => {}
        }
    }

    /// How long to wait at `now` before asking for the segment at media time
    /// `at`, so that no more than `ahead` seconds are buffered.
    pub fn wait_before(&self, at: f64, now: f64, ahead: f64) -> f64 {
        self.reaches(at)
            .map_or(0.0, |due| (due - ahead - now).max(0.0))
    }

    pub fn stalled_seconds(&self) -> f64 {
        self.stalled
    }
}

/// What one client saw.
#[derive(Debug, Default, Clone)]
pub struct ClientResult {
    /// Seconds to the first segment (transcode) or first byte (direct play).
    pub start: Option<f64>,
    pub stalls: usize,
    pub stalled_seconds: f64,
    /// Direct play: the download's speed in multiples of real time.
    pub speed: Option<f64>,
    pub error: Option<String>,
}

/// One row of the report from every client's result.
pub fn summarize(
    mode: &str,
    clients: &[ClientResult],
    server_cores: Option<f64>,
    peak_memory_mib: Option<f64>,
) -> StreamRun {
    let starts: Vec<f64> = clients
        .iter()
        .filter_map(|c| c.start.map(|s| s * 1000.0))
        .collect();
    let ok: Vec<&ClientResult> = clients.iter().filter(|c| c.error.is_none()).collect();
    let kept_up = ok
        .iter()
        .filter(|c| c.stalls == 0 && c.speed.is_none_or(|s| s >= 1.0))
        .count();
    let slowest = ok
        .iter()
        .filter_map(|c| c.speed)
        .min_by(f64::total_cmp)
        .map(|s| (s * 10.0).round() / 10.0);
    StreamRun {
        mode: mode.to_string(),
        streams: clients.len(),
        start_p50_ms: (!starts.is_empty()).then(|| stats::percentile(&starts, 50.0).round()),
        start_max_ms: starts
            .iter()
            .copied()
            .max_by(f64::total_cmp)
            .map(f64::round),
        kept_up,
        stalls: ok.iter().map(|c| c.stalls).sum(),
        stalled_seconds: (ok.iter().map(|c| c.stalled_seconds).sum::<f64>() * 10.0).round() / 10.0,
        slowest_speed: slowest,
        server_cores,
        peak_memory_mib,
        errors: clients.len() - ok.len(),
    }
}

/// A signed-in client of the server under test.
#[derive(Clone)]
pub struct Client {
    pub http: reqwest::Client,
    pub base: Url,
    pub auth: String,
    pub user_id: String,
}

impl Client {
    fn url(&self, path: &str) -> anyhow::Result<Url> {
        Ok(self.base.join(path)?)
    }

    /// `PlaybackInfo` as the admin web player asks for it; `transcode` turns
    /// direct play and direct stream off, as it does to force a transcode.
    async fn playback_info(&self, film: &Film, transcode: bool) -> anyhow::Result<Value> {
        let body = json!({
            "UserId": self.user_id,
            "DeviceProfile": device_profile(),
            "MaxStreamingBitrate": 120_000_000,
            "StartTimeTicks": 0,
            "EnableDirectPlay": !transcode,
            "EnableDirectStream": !transcode,
            "EnableTranscoding": true,
            "AutoOpenLiveStream": false,
        });
        Ok(self
            .http
            .post(self.url(&format!("/Items/{}/PlaybackInfo", film.item_id))?)
            .header("Authorization", &self.auth)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn text(&self, url: Url) -> anyhow::Result<String> {
        Ok(self
            .http
            .get(url)
            .header("Authorization", &self.auth)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }

    /// Download the film once, as fast as the server sends it.
    pub async fn direct_play(&self, film: &Film) -> ClientResult {
        match self.try_direct_play(film).await {
            Ok(r) => r,
            Err(e) => ClientResult {
                error: Some(format!("{e:#}")),
                ..ClientResult::default()
            },
        }
    }

    async fn try_direct_play(&self, film: &Film) -> anyhow::Result<ClientResult> {
        let started = Instant::now();
        let info = self.playback_info(film, false).await?;
        let source = &info["MediaSources"][0];
        if source["SupportsDirectPlay"].as_bool() != Some(true) {
            bail!("the server offered no direct play for the film");
        }
        let ms = source["Id"].as_str().unwrap_or(&film.item_id);
        let session = info["PlaySessionId"].as_str().unwrap_or_default();
        let url = self.url(&format!(
            "/Videos/{}/stream?static=true&MediaSourceId={ms}&PlaySessionId={session}",
            film.item_id
        ))?;
        let mut resp = self
            .http
            .get(url)
            .header("Authorization", &self.auth)
            .send()
            .await?
            .error_for_status()?;
        let mut start = None;
        let mut bytes = 0u64;
        let body_started = Instant::now();
        while let Some(chunk) = resp.chunk().await? {
            start.get_or_insert_with(|| started.elapsed().as_secs_f64());
            bytes += chunk.len() as u64;
        }
        if bytes != film.bytes {
            bail!("got {bytes} of {} bytes", film.bytes);
        }
        let took = body_started.elapsed().as_secs_f64().max(1e-6);
        Ok(ClientResult {
            start,
            speed: Some(film.seconds / took),
            ..ClientResult::default()
        })
    }

    /// Play `length` seconds of the film transcoded, like a player would,
    /// then stop the transcode.
    pub async fn transcode(&self, film: &Film, length: Duration) -> ClientResult {
        let mut session = None;
        let r = match self.try_transcode(film, length, &mut session).await {
            Ok(r) => r,
            Err(e) => ClientResult {
                error: Some(format!("{e:#}")),
                ..ClientResult::default()
            },
        };
        if let Some(id) = session
            && let Ok(url) = self.url(&format!("/Videos/ActiveEncodings?PlaySessionId={id}"))
        {
            let _ = self
                .http
                .delete(url)
                .header("Authorization", &self.auth)
                .send()
                .await;
        }
        r
    }

    async fn try_transcode(
        &self,
        film: &Film,
        length: Duration,
        session: &mut Option<String>,
    ) -> anyhow::Result<ClientResult> {
        let started = Instant::now();
        let info = self.playback_info(film, true).await?;
        *session = info["PlaySessionId"].as_str().map(str::to_string);
        let master = info["MediaSources"][0]["TranscodingUrl"]
            .as_str()
            .context("no TranscodingUrl")?;
        let master = self.url(master)?;
        let variant = variant_url(&self.text(master.clone()).await?, &master)?;
        let segs = segments(&self.text(variant.clone()).await?, &variant)?;
        let mut player = Player::default();
        let mut at = 0.0;
        let now = || started.elapsed().as_secs_f64();
        for seg in segs {
            if at >= length.as_secs_f64() {
                break;
            }
            let wait = player.wait_before(at, now(), BUFFER_AHEAD);
            if wait > 0.0 {
                tokio::time::sleep(Duration::from_secs_f64(wait)).await;
            }
            self.http
                .get(seg.url)
                .header("Authorization", &self.auth)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            player.arrived(at, now());
            at += seg.seconds;
        }
        Ok(ClientResult {
            start: player.reaches(0.0),
            stalls: player.stalls,
            stalled_seconds: player.stalled_seconds(),
            ..ClientResult::default()
        })
    }
}

/// The admin web player's profile for a browser that plays H.264/AAC in MP4:
/// MP4 plays directly, anything else is transcoded to HLS.
fn device_profile() -> Value {
    json!({
        "Name": "rustyfin-bench",
        "MaxStreamingBitrate": 120_000_000,
        "MaxStaticBitrate": 120_000_000,
        "DirectPlayProfiles": [
            {"Type": "Video", "Container": "mp4,m4v", "VideoCodec": "h264", "AudioCodec": "aac"}
        ],
        "TranscodingProfiles": [
            {"Type": "Video", "Container": "ts", "VideoCodec": "h264", "AudioCodec": "aac",
             "Protocol": "hls", "Context": "Streaming"}
        ]
    })
}

/// Write the film: `seconds` of 1080p24 H.264 (a moving test pattern) with
/// stereo AAC, in MP4 so a browser profile can play it directly.
pub fn make_film(path: &std::path::Path, seconds: u64) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=24"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"])
        .args(["-t", &seconds.to_string()])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-b:v", "8M"])
        .args(["-pix_fmt", "yuv420p", "-g", "48"])
        .args(["-c:a", "aac", "-b:a", "192k", "-ac", "2"])
        .args(["-movflags", "+faststart"])
        .arg(path)
        .output()
        .context("run ffmpeg")?;
    if !out.status.success() {
        bail!("ffmpeg failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("http://127.0.0.1:8096/Videos/abc/master.m3u8?PlaySessionId=p&api_key=k")
            .unwrap()
    }

    #[test]
    fn the_variant_is_resolved_against_the_master() {
        let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=8000000\nmain.m3u8?PlaySessionId=p\n";
        let url = variant_url(master, &base()).unwrap();
        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:8096/Videos/abc/main.m3u8?PlaySessionId=p"
        );
    }

    #[test]
    fn a_master_without_a_variant_is_an_error() {
        assert!(variant_url("#EXTM3U\n", &base()).is_err());
    }

    #[test]
    fn segments_carry_their_lengths_in_order() {
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.000000,\nhls1/p/0.ts\n\
                        #EXTINF:6.0,\nhls1/p/1.ts\n#EXTINF:2.5,\nhls1/p/2.ts\n#EXT-X-ENDLIST\n";
        let segs = segments(playlist, &base()).unwrap();
        let lengths: Vec<f64> = segs.iter().map(|s| s.seconds).collect();
        assert_eq!(lengths, vec![6.0, 6.0, 2.5]);
        assert_eq!(
            segs[1].url.as_str(),
            "http://127.0.0.1:8096/Videos/abc/hls1/p/1.ts"
        );
    }

    #[test]
    fn a_segment_in_time_is_no_stall() {
        let mut p = Player::default();
        p.arrived(0.0, 3.0); // playback starts at 3 s
        assert_eq!(p.reaches(6.0), Some(9.0));
        p.arrived(6.0, 8.5); // due at 9 s
        assert_eq!(p.stalls, 0);
    }

    #[test]
    fn a_late_segment_stalls_and_pushes_playback_back() {
        let mut p = Player::default();
        p.arrived(0.0, 3.0);
        p.arrived(6.0, 11.0); // due at 9 s: 2 s late
        assert_eq!(p.stalls, 1);
        assert!((p.stalled_seconds() - 2.0).abs() < 1e-9);
        assert_eq!(p.reaches(12.0), Some(17.0), "playback resumed at 11 s");
    }

    #[test]
    fn the_player_waits_rather_than_buffer_more_than_it_may() {
        let mut p = Player::default();
        assert_eq!(
            p.wait_before(0.0, 0.0, 30.0),
            0.0,
            "before playback, no wait"
        );
        p.arrived(0.0, 2.0);
        // Media time 60 s is reached at 62 s; with 30 s buffered, ask at 32 s.
        assert!((p.wait_before(60.0, 10.0, 30.0) - 22.0).abs() < 1e-9);
        assert_eq!(p.wait_before(12.0, 10.0, 30.0), 0.0);
    }

    #[test]
    fn the_summary_counts_who_kept_up() {
        let clients = vec![
            ClientResult {
                start: Some(1.0),
                ..ClientResult::default()
            },
            ClientResult {
                start: Some(3.0),
                stalls: 2,
                stalled_seconds: 4.25,
                ..ClientResult::default()
            },
            ClientResult {
                error: Some("boom".into()),
                ..ClientResult::default()
            },
        ];
        let row = summarize("transcode", &clients, Some(3.5), Some(900.0));
        assert_eq!(row.streams, 3);
        assert_eq!(row.kept_up, 1);
        assert_eq!(row.stalls, 2);
        assert_eq!(row.stalled_seconds, 4.3);
        assert_eq!(row.errors, 1);
        assert_eq!(row.start_max_ms, Some(3000.0));
        assert_eq!(row.slowest_speed, None);
    }

    #[test]
    fn direct_play_below_real_time_did_not_keep_up() {
        let fast = ClientResult {
            start: Some(0.01),
            speed: Some(40.0),
            ..ClientResult::default()
        };
        let slow = ClientResult {
            speed: Some(0.84),
            ..fast.clone()
        };
        let row = summarize("direct play", &[fast, slow], None, None);
        assert_eq!(row.kept_up, 1);
        assert_eq!(row.slowest_speed, Some(0.8));
    }
}
