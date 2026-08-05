mod action;
mod app;
mod commands;
mod components;
mod db;
mod errors;
mod event;

use std::path::PathBuf;

use app::App;
use errors::{AppError, Result};

fn main() -> Result<()> {
    let db_path = data_dir()?.join("portside.db");

    // Match ratatui::restore() on panic too, so a mid-render panic doesn't leave the terminal in
    // raw/alternate-screen mode.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        default_hook(info);
    }));

    let mut terminal = ratatui::init();
    let mut app = App::new(db_path)?;
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}

fn data_dir() -> Result<PathBuf> {
    directories::ProjectDirs::from("", "", "portside")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or(AppError::NoDataDir)
}
