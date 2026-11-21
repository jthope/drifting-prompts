//! What every product saves of its window: how it looks and where it was left.

use serde::{Deserialize, Serialize};

/// How the window looks; an unknown name (a removed theme, a hand edit) shows the default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Whether the window is dark: the chosen mode, or the system's last while following it.
    pub dark: bool,
    /// Whether the window follows the system's light or dark mode, rather than `dark`.
    pub follow_system: bool,
    /// The theme for when the window is dark: `default-dark` or a theme file's name.
    pub dark_theme: String,
    /// The theme for when the window is light: `default-light` or a theme file's name.
    pub light_theme: String,
    /// The texture over page headers: `none`, `haze` or `dots`.
    pub texture: String,
    /// How far what stands out lifts off the page: `none`, `soft` or `raised`.
    pub elevation: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            dark: true,
            follow_system: true,
            dark_theme: "default-dark".into(),
            light_theme: "default-light".into(),
            texture: "none".into(),
            elevation: "none".into(),
        }
    }
}

/// Where the window was left, in points; a zero width or height means none was saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    /// The left edge, in points on the screen.
    pub x: f32,
    /// The top edge, in points on the screen.
    pub y: f32,
    /// The width, in points on the screen; zero when never saved.
    pub width: f32,
    /// The height, in points on the screen; zero when never saved.
    pub height: f32,
    /// Whether the window was maximized.
    pub maximized: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_partial_appearance_takes_the_defaults() {
        let partial: Appearance = serde_json::from_str(r#"{ "dark_theme": "monokai" }"#).unwrap();
        assert_eq!(
            partial,
            Appearance {
                dark_theme: "monokai".into(),
                ..Appearance::default()
            }
        );
        assert_eq!(
            serde_json::from_str::<WindowState>("{}").unwrap(),
            WindowState::default()
        );
    }
}
