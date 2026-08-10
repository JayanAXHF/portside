use std::collections::HashMap;
use std::time::{Duration, Instant};

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedValue;

use super::NowPlayingInfo;

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

/// Queries the first MPRIS2-registered player (`org.mpris.MediaPlayer2.*` on the session bus)
/// via `zbus`'s blocking API — safe to call from our own poll thread since it has no async
/// runtime of its own to conflict with (see `Cargo.toml`'s `blocking-api`-only feature set).
/// Any failure along the way — no session bus, no player registered, a player that doesn't
/// implement the properties queried here — collapses to `None`, matching the macOS backend's
/// "unavailable is unavailable" behavior; there's no distinct error state to chase.
pub fn fetch() -> Option<NowPlayingInfo> {
    let fetched_at = Instant::now();
    let connection = Connection::session().ok()?;
    let bus_name = first_mpris_player(&connection)?;
    let player = Proxy::new(&connection, bus_name, PLAYER_PATH, PLAYER_INTERFACE).ok()?;

    let playback_status: String = player.get_property("PlaybackStatus").ok()?;
    let playing = playback_status == "Playing";

    let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").ok()?;
    let title = metadata
        .get("xesam:title")
        .and_then(|v| v.clone().downcast::<String>().ok())
        .filter(|s| !s.is_empty())?;
    let artist = metadata
        .get("xesam:artist")
        .and_then(|v| v.clone().downcast::<Vec<String>>().ok())
        .and_then(|artists| artists.into_iter().next())
        .unwrap_or_default();
    let duration_micros = metadata
        .get("mpris:length")
        .and_then(|v| v.clone().downcast::<i64>().ok())
        .unwrap_or(0);
    let elapsed_micros: i64 = player.get_property("Position").unwrap_or(0);

    Some(NowPlayingInfo {
        title,
        artist,
        playing,
        elapsed: Duration::from_micros(elapsed_micros.max(0) as u64),
        duration: Duration::from_micros(duration_micros.max(0) as u64),
        fetched_at,
    })
}

/// Finds the first bus name registered under the MPRIS2 well-known prefix. Only ever the first —
/// if multiple players are running, portside just shows whichever one happened to register
/// first; there's no "active" player concept in MPRIS2 to disambiguate further.
fn first_mpris_player(connection: &Connection) -> Option<String> {
    let dbus = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .ok()?;
    let names: Vec<String> = dbus.call("ListNames", &()).ok()?;
    names.into_iter().find(|n| n.starts_with(MPRIS_PREFIX))
}
