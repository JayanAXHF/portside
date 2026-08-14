mod builtin;

use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

pub use builtin::DEFAULT;

use crate::db::SessionStatus;

/// A color palette applied across every component. Fields map to the semantic uses found
/// throughout the UI (session status, accent highlights, the now-playing gauge, the history
/// heatmap) rather than to raw terminal color slots, so a theme reads as "what does Running look
/// like" instead of "what is color #3".
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct Theme {
    /// The whole-screen background, painted behind every component each frame. `Color::Reset`
    /// leaves the terminal's own background showing through, which is what `default` and `mono`
    /// do — only themes that want to actually override the terminal background (e.g.
    /// `solarized`) should set this to something else.
    pub background: Color,
    pub running: Color,
    pub paused: Color,
    pub on_break: Color,
    pub completed: Color,
    pub accent: Color,
    pub panel_bg: Color,
    pub gauge_filled: Color,
    pub gauge_unfilled: Color,
    pub heatmap_ramp: [Color; 5],
}

impl Theme {
    pub fn status_color(&self, status: SessionStatus) -> Color {
        match status {
            SessionStatus::Running => self.running,
            SessionStatus::Paused => self.paused,
            SessionStatus::OnBreak => self.on_break,
            SessionStatus::Completed => self.completed,
        }
    }

    pub fn status_style(&self, status: SessionStatus) -> Style {
        let style = Style::new().fg(self.status_color(status));
        if status == SessionStatus::Completed {
            style.add_modifier(Modifier::DIM)
        } else {
            style
        }
    }

    pub fn heatmap_color(&self, level: u8) -> Color {
        self.heatmap_ramp[(level as usize).min(self.heatmap_ramp.len() - 1)]
    }

    pub fn dim(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    pub fn highlight(&self) -> Style {
        Style::new().add_modifier(Modifier::REVERSED)
    }
}

/// Looks up a theme by name: built-ins first, then a user theme from
/// `<config_dir>/themes/<name>.toml`.
pub fn resolve(name: &str, config_dir: &std::path::Path) -> Option<Theme> {
    builtin::by_name(name).or_else(|| load_user_theme(config_dir, name))
}

fn load_user_theme(config_dir: &std::path::Path, name: &str) -> Option<Theme> {
    let path = config_dir.join("themes").join(format!("{name}.toml"));
    let contents = std::fs::read_to_string(path).ok()?;
    toml::from_str(&contents).ok()
}

/// Built-in names plus any `*.toml` files found in `<config_dir>/themes/`, for listing in error
/// messages. Best-effort: an unreadable/missing dir just yields no user themes.
pub fn available_names(config_dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = builtin::NAMES.iter().map(|s| s.to_string()).collect();
    if let Ok(entries) = std::fs::read_dir(config_dir.join("themes")) {
        for entry in entries.flatten() {
            if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }
    names
}

/// Writes `<config_dir>/themes/<name>.toml` containing `DEFAULT`'s values as a starter file for
/// `--init-theme <name>`. Errors if the file already exists, so it never clobbers user edits.
pub fn scaffold(config_dir: &std::path::Path, name: &str) -> std::io::Result<std::path::PathBuf> {
    let dir = config_dir.join("themes");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.toml"));
    if path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} already exists", path.display()),
        ));
    }
    let contents = toml::to_string_pretty(&DEFAULT)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(&path, contents)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_color_covers_every_variant() {
        let theme = DEFAULT;
        assert_eq!(theme.status_color(SessionStatus::Running), theme.running);
        assert_eq!(theme.status_color(SessionStatus::Paused), theme.paused);
        assert_eq!(theme.status_color(SessionStatus::OnBreak), theme.on_break);
        assert_eq!(
            theme.status_color(SessionStatus::Completed),
            theme.completed
        );
    }

    #[test]
    fn heatmap_color_clamps_out_of_range_levels() {
        let theme = DEFAULT;
        assert_eq!(theme.heatmap_color(0), theme.heatmap_ramp[0]);
        assert_eq!(theme.heatmap_color(4), theme.heatmap_ramp[4]);
        assert_eq!(theme.heatmap_color(255), theme.heatmap_ramp[4]);
    }

    #[test]
    fn resolve_finds_builtin_before_touching_disk() {
        let missing_dir = std::env::temp_dir().join("portside-theme-test-nonexistent-dir");
        assert!(resolve("default", &missing_dir).is_some());
    }

    #[test]
    fn resolve_returns_none_for_unknown_name_and_missing_dir() {
        let missing_dir = std::env::temp_dir().join("portside-theme-test-nonexistent-dir-2");
        assert!(resolve("nonexistent", &missing_dir).is_none());
    }

    #[test]
    fn scaffold_then_resolve_round_trips() {
        let dir = std::env::temp_dir().join("portside-theme-test-scaffold-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = scaffold(&dir, "mytheme").unwrap();
        assert!(path.exists());

        let loaded = resolve("mytheme", &dir).expect("scaffolded theme should resolve");
        assert_eq!(loaded.running, DEFAULT.running);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scaffold_refuses_to_overwrite_existing_file() {
        let dir = std::env::temp_dir().join("portside-theme-test-scaffold-no-clobber");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        scaffold(&dir, "mytheme").unwrap();
        let result = scaffold(&dir, "mytheme");
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn available_names_includes_builtins_and_user_themes() {
        let dir = std::env::temp_dir().join("portside-theme-test-available-names");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        scaffold(&dir, "mytheme").unwrap();

        let names = available_names(&dir);
        assert!(names.contains(&"default".to_string()));
        assert!(names.contains(&"mytheme".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
