use std::sync::mpsc;

use discord_rich_presence::{DiscordIpc, DiscordIpcClient, activity};

/// portside's Discord Application client ID, used to attribute the Rich Presence status shown on
/// a user's profile to this app rather than a generic one.
const DISCORD_CLIENT_ID: &str = "1537797587569086524";

enum Command {
    SetActivity {
        details: String,
        state: String,
        start: Option<i64>,
    },
    Clear,
    Shutdown,
}

/// Pushes work-session state to Discord Rich Presence from a dedicated background thread,
/// mirroring `media::MediaWatcher`'s "never let a slow/blocking OS call stall the render loop"
/// pattern — but inverted: `MediaWatcher` is polled every tick, `DiscordPresence` is pushed to
/// only on session state changes, since Discord computes the live elapsed counter itself from the
/// `start` timestamp rather than needing continuous updates.
pub struct DiscordPresence {
    tx: mpsc::Sender<Command>,
}

impl DiscordPresence {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || run(rx));
        Self { tx }
    }

    /// Best-effort; if the background thread has gone away there's nothing to recover.
    pub fn set_activity(&self, details: String, state: String, start: Option<i64>) {
        let _ = self.tx.send(Command::SetActivity {
            details,
            state,
            start,
        });
    }

    pub fn clear(&self) {
        let _ = self.tx.send(Command::Clear);
    }
}

impl Drop for DiscordPresence {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
    }
}

/// Discord not running, or the IPC handshake failing for any other reason, is treated exactly
/// like `media::fetch`'s "unrecoverable, collapse to no-op" case: every failure just flips
/// `connected = false`, and the next command retries `reconnect()`. Nothing here can block or
/// panic the TUI thread — worst case, presence updates are silently dropped.
fn run(rx: mpsc::Receiver<Command>) {
    let mut client = DiscordIpcClient::new(DISCORD_CLIENT_ID);
    let mut connected = client.connect().is_ok();

    for cmd in rx {
        match cmd {
            Command::Shutdown => {
                if connected {
                    let _ = client.close();
                }
                return;
            }
            Command::SetActivity {
                details,
                state,
                start,
            } => {
                if !connected {
                    connected = client.reconnect().is_ok();
                }
                if connected {
                    let mut payload = activity::Activity::new().details(&details).state(&state);
                    if let Some(start) = start {
                        payload = payload.timestamps(activity::Timestamps::new().start(start));
                    }
                    if client.set_activity(payload).is_err() {
                        connected = false;
                    }
                }
            }
            Command::Clear => {
                if !connected {
                    connected = client.reconnect().is_ok();
                }
                if connected && client.clear_activity().is_err() {
                    connected = false;
                }
            }
        }
    }
}
