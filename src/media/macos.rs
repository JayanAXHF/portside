use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::NowPlayingInfo;

/// Queries `MediaRemote.framework` for the currently playing track via `media-remote`'s
/// `get_raw_info`, which shells out to `osascript -l JavaScript` running a JXA snippet that loads
/// the framework in-process and reads `MRNowPlayingRequest.localNowPlayingItem` directly (see
/// `nowPlaying.jxa` in that crate). No Apple Events/Automation permission prompt is involved —
/// the script loads the framework into its own process rather than scripting a foreign app.
///
/// This crate also ships a lower-level path (`get_now_playing_info`, backed by the classic
/// `MRMediaRemoteGetNowPlayingInfo` dispatch-queue C API) that looked like the natural choice —
/// no process spawn per poll — but on this machine (macOS 26.3.1) it reliably returned `None`
/// even with Spotify actively playing, while `get_raw_info` returned full metadata every time.
/// Verified directly against both paths side by side before picking this one; costs ~40ms per
/// call, which is fine at `MEDIA_POLL_INTERVAL`'s cadence since it runs on its own thread and
/// never blocks the render loop.
pub fn fetch() -> Option<NowPlayingInfo> {
    let fetched_at = Instant::now();
    let raw = media_remote::get_raw_info()?;
    let info = raw.get("info")?;

    let title = info
        .get("kMRMediaRemoteNowPlayingInfoTitle")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    let artist = info
        .get("kMRMediaRemoteNowPlayingInfoArtist")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let playing = raw.get("isPlaying").and_then(|v| v.as_bool()).unwrap_or(false);
    let elapsed = info
        .get("kMRMediaRemoteNowPlayingInfoElapsedTime")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let duration = info
        .get("kMRMediaRemoteNowPlayingInfoDuration")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    // `elapsed` above is a cached snapshot, not queried live: verified directly that
    // `kMRMediaRemoteNowPlayingInfoElapsedTime` (and its sibling `...Timestamp`) stay byte-for-
    // byte identical across several polls seconds apart while a track keeps playing — macOS only
    // refreshes it on its own schedule (track/seek/pause), not on every read. `...Timestamp` is
    // the wall-clock instant (epoch ms) that snapshot was taken, so correct for however stale it
    // is using that, rather than trusting it as "elapsed as of right now".
    let snapshot_epoch_ms = info
        .get("kMRMediaRemoteNowPlayingInfoTimestamp")
        .and_then(|v| v.as_f64());
    let staleness = snapshot_epoch_ms
        .and_then(|ms| {
            let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs_f64() * 1000.0;
            Some(Duration::from_secs_f64(((now_ms - ms) / 1000.0).max(0.0)))
        })
        .unwrap_or(Duration::ZERO);
    let duration = Duration::from_secs_f64(duration.max(0.0));
    let elapsed = Duration::from_secs_f64(elapsed.max(0.0))
        + if playing { staleness } else { Duration::ZERO };
    let elapsed = if duration > Duration::ZERO { elapsed.min(duration) } else { elapsed };

    Some(NowPlayingInfo {
        title: title.to_string(),
        artist: artist.to_string(),
        playing,
        elapsed,
        duration,
        fetched_at,
    })
}
