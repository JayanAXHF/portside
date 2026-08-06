use std::io::Write;

/// Rings the terminal bell and attempts a best-effort OS desktop notification when a timed break
/// expires. Writing the bare `\x07` (BEL) control byte to stdout is safe alongside ratatui's
/// raw-mode/alt-screen session: terminal emulators intercept it to ring the bell rather than
/// painting it into the visible buffer, so it can't desync ratatui's diff-based renderer. The OS
/// notification is allowed to fail silently (no notification daemon, or an unbundled binary on
/// macOS where notify-rust's backend is unreliable) since the caller always also fires an in-app
/// toast as the guaranteed fallback.
pub fn fire_break_alert(topic: &str) {
    let _ = std::io::stdout().write_all(b"\x07");
    let _ = std::io::stdout().flush();

    let _ = notify_rust::Notification::new()
        .summary("portside")
        .body(&format!("Break over — {topic}"))
        .show();
}
