use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::errors::Result;

fn default_true() -> bool {
    true
}

/// Persisted user settings, stored as `config.toml` in the platform config dir (separate from
/// `portside.db`, which holds session data). Currently just the Discord Rich Presence toggle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_true")]
    pub discord_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            discord_enabled: true,
        }
    }
}

impl Config {
    /// Best-effort: a missing file or a parse error both silently fall back to defaults, the same
    /// "collapse unrecoverable read failures rather than error out" convention as
    /// `media::fetch`'s `None` and `notify::fire_break_alert`'s `let _ = `.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| toml::from_str(&contents).ok())
            .unwrap_or_default()
    }

    /// Unlike `load`, this propagates errors — it's only called from an explicit user action
    /// (`:discord on`/`:discord off`) that can surface a failure via a toast.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_falls_back_to_default_when_file_is_missing() {
        let path = std::env::temp_dir().join("portside-config-test-missing-nonexistent.toml");
        let config = Config::load(&path);
        assert!(config.discord_enabled);
    }

    #[test]
    fn load_falls_back_to_default_on_parse_error() {
        let path = std::env::temp_dir().join("portside-config-test-corrupt.toml");
        std::fs::write(&path, "not valid toml {{{").unwrap();
        let config = Config::load(&path);
        let _ = std::fs::remove_file(&path);
        assert!(config.discord_enabled);
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = std::env::temp_dir().join("portside-config-test-roundtrip.toml");
        let config = Config {
            discord_enabled: false,
        };
        config.save(&path).unwrap();
        let loaded = Config::load(&path);
        let _ = std::fs::remove_file(&path);
        assert!(!loaded.discord_enabled);
    }
}
