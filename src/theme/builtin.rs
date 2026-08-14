use ratatui::style::Color;

use super::Theme;

/// The app's original hardcoded palette, unchanged from before theming existed.
pub const DEFAULT: Theme = Theme {
    background: Color::Reset,
    running: Color::Green,
    paused: Color::Yellow,
    on_break: Color::Cyan,
    completed: Color::DarkGray,
    accent: Color::Cyan,
    panel_bg: Color::Rgb(74, 74, 74),
    gauge_filled: Color::White,
    gauge_unfilled: Color::Gray,
    heatmap_ramp: [
        Color::DarkGray,
        Color::Rgb(38, 70, 73),
        Color::Rgb(21, 110, 115),
        Color::Rgb(13, 150, 158),
        Color::Cyan,
    ],
};

/// A warmer, Solarized-inspired palette.
pub const SOLARIZED: Theme = Theme {
    background: Color::Rgb(0, 43, 54),        // solarized base03
    running: Color::Rgb(133, 153, 0),         // solarized green
    paused: Color::Rgb(181, 137, 0),          // solarized yellow
    on_break: Color::Rgb(38, 139, 210),       // solarized blue
    completed: Color::Rgb(88, 110, 117),      // solarized base01
    accent: Color::Rgb(42, 161, 152),         // solarized cyan
    panel_bg: Color::Rgb(7, 54, 66),          // solarized base02
    gauge_filled: Color::Rgb(238, 232, 213),  // solarized base2
    gauge_unfilled: Color::Rgb(88, 110, 117), // solarized base01
    heatmap_ramp: [
        Color::Rgb(7, 54, 66),
        Color::Rgb(0, 68, 68),
        Color::Rgb(0, 95, 95),
        Color::Rgb(20, 130, 130),
        Color::Rgb(42, 161, 152),
    ],
};

/// A minimal greyscale palette for terminals with limited color support.
pub const MONO: Theme = Theme {
    background: Color::Reset,
    running: Color::White,
    paused: Color::Gray,
    on_break: Color::Gray,
    completed: Color::DarkGray,
    accent: Color::White,
    panel_bg: Color::Black,
    gauge_filled: Color::White,
    gauge_unfilled: Color::DarkGray,
    heatmap_ramp: [
        Color::Black,
        Color::DarkGray,
        Color::Gray,
        Color::White,
        Color::White,
    ],
};

pub const NAMES: &[&str] = &["default", "solarized", "mono"];

pub fn by_name(name: &str) -> Option<Theme> {
    match name {
        "default" => Some(DEFAULT),
        "solarized" => Some(SOLARIZED),
        "mono" => Some(MONO),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_resolves() {
        for name in NAMES {
            assert!(by_name(name).is_some(), "expected {name} to resolve");
        }
    }

    #[test]
    fn unknown_name_resolves_to_none() {
        assert!(by_name("nonexistent").is_none());
    }
}
