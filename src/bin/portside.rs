use std::path::PathBuf;

use clap::Parser;
use portside_tui::app::App;
use portside_tui::errors::{AppError, Result};

/// A terminal UI for tracking work sessions.
#[derive(Parser)]
struct Cli {
    /// Disable Discord Rich Presence for this run. Doesn't change the persisted `:discord`
    /// setting — use the in-app `:discord off` command for that.
    #[arg(long)]
    no_discord: bool,

    /// Write a starter theme file to <config_dir>/themes/<name>.toml (a copy of the default
    /// theme's values) for editing, then exit without launching the TUI.
    #[arg(long, value_name = "NAME")]
    init_theme: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(name) = cli.init_theme.as_deref() {
        let path = portside_tui::theme::scaffold(&config_dir()?, name)?;
        println!("Wrote {}", path.display());
        return Ok(());
    }

    let db_path = data_dir()?.join("portside.db");

    // Redirect our own stderr to a log file instead of the terminal. Subprocesses we spawn (e.g.
    // macOS's `osascript` for now-playing info, invoked on its own poll cadence in
    // `media::MediaWatcher`) inherit our stderr, and anything they write there — such as
    // `osascript`'s "execution error" text when nothing is currently registered with
    // `MRNowPlayingRequest` — lands directly in the alternate screen and corrupts the TUI's
    // display. Nothing in this app writes diagnostics to stderr on the happy path, so this is
    // safe to redirect unconditionally rather than filtering by source.
    redirect_stderr_to_log(&data_dir()?.join("portside.log"));

    // Match ratatui::restore() on panic too, so a mid-render panic doesn't leave the terminal in
    // raw/alternate-screen mode.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        default_hook(info);
    }));

    let mut terminal = ratatui::init();
    let mut app = App::new(db_path, config_dir()?, cli.no_discord)?;
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}

fn data_dir() -> Result<PathBuf> {
    directories::ProjectDirs::from("", "", "portside")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or(AppError::NoDataDir)
}

fn config_dir() -> Result<PathBuf> {
    directories::ProjectDirs::from("", "", "portside")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .ok_or(AppError::NoDataDir)
}

/// Points fd 2 (stderr) at `path`, so writes that would otherwise hit the terminal — including
/// from inherited-stderr subprocesses we spawn — go to the log file instead. Best-effort: if the
/// log file can't be created, stderr is left alone.
#[cfg(unix)]
fn redirect_stderr_to_log(path: &std::path::Path) {
    use std::os::unix::io::AsRawFd;

    unsafe extern "C" {
        fn dup2(oldfd: i32, newfd: i32) -> i32;
    }

    if let Ok(file) = std::fs::File::create(path) {
        unsafe {
            dup2(file.as_raw_fd(), 2);
        }
    }
}

#[cfg(not(unix))]
fn redirect_stderr_to_log(_path: &std::path::Path) {}
