use std::sync::mpsc;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

/// Snapshot of the currently playing media, as reported by the platform media backend.
///
/// `elapsed` is the position the OS reported *as of `fetched_at`*, not a continuously updating
/// value — both the macOS and MPRIS backends only get a fresh number from the OS on
/// track/seek/play-pause events, not on every poll. `NowPlayingComponent::render` interpolates
/// forward from `fetched_at` using wall-clock time to get a smoothly ticking progress bar between
/// polls.
#[derive(Debug, Clone)]
pub struct NowPlayingInfo {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// Position within the track as of `fetched_at`. `duration` of `Duration::ZERO` means
    /// unknown (e.g. a live stream, or a source that doesn't report track length) and hides the
    /// progress bar.
    pub elapsed: Duration,
    pub duration: Duration,
    pub fetched_at: Instant,
}

/// One-shot query of the platform media backend. Always returns `None` rather than an error —
/// "no player running", "nothing playing", and "the OS denied us access" (e.g. macOS 15.4+'s
/// MediaRemote entitlement checks) are all equally unrecoverable from portside's perspective, so
/// they collapse to the same "Nothing playing" UI state rather than a distinct error path.
fn fetch() -> Option<NowPlayingInfo> {
    #[cfg(target_os = "macos")]
    {
        macos::fetch()
    }
    #[cfg(target_os = "linux")]
    {
        linux::fetch()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Polls the platform media backend on a dedicated background thread so the (potentially slow,
/// blocking) OS call never stalls the render loop, mirroring `EventHandler` in `src/event.rs`.
/// Fully decoupled from that crossterm-driven loop: it never touches terminal state, it just
/// hands back whatever it last learned via a non-blocking channel read.
pub struct MediaWatcher {
    rx: mpsc::Receiver<Option<NowPlayingInfo>>,
}

impl MediaWatcher {
    pub fn new(poll_interval: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            loop {
                if tx.send(fetch()).is_err() {
                    return;
                }
                std::thread::sleep(poll_interval);
            }
        });
        Self { rx }
    }

    /// Non-blocking; drains to the most recent update if more than one arrived since the last
    /// call, since only the latest snapshot matters for rendering.
    pub fn try_recv(&self) -> Option<Option<NowPlayingInfo>> {
        self.rx.try_iter().last()
    }
}
