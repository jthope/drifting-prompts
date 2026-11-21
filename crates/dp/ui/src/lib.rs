//! The Rust under the suite's styled TSX controls: the themes' colors, the saved appearance and
//! window place, folders, and the bridges' listeners.

pub mod folders;
mod listen;

use std::path::{Path, PathBuf};

use nsh_core::prelude::*;

pub use crate::listen::{listen, listen_later};

const TEST_RUNNER_SUFFIX: &str = "-test";

const DEVELOPMENT_HOST_SUFFIX: &str = "-host";

/// nsh's development server never looks in a folder of this name.
const DEVELOPMENT_FOLDER: &str = ".nsh";

/// Whether this is `nsh test`'s headless runner.
pub fn is_test_runner() -> bool {
    std::env::current_exe()
        .ok()
        .is_some_and(|exe| name_ends_with(&exe, TEST_RUNNER_SUFFIX))
}

/// Where an app saves when run in nsh's development host; `None` for any other executable.
/// Workaround: nsh's development server rebuilds on any `.json` change outside `.nsh` folders.
pub fn development_folder() -> Option<PathBuf> {
    development_folder_of(&std::env::current_exe().ok()?)
}

fn development_folder_of(exe: &Path) -> Option<PathBuf> {
    if !name_ends_with(exe, DEVELOPMENT_HOST_SUFFIX) {
        return None;
    }
    Some(exe.parent()?.join(DEVELOPMENT_FOLDER))
}

fn name_ends_with(exe: &Path, suffix: &str) -> bool {
    exe.file_stem()
        .is_some_and(|name| name.to_string_lossy().ends_with(suffix))
}

/// A color of the app's theme, by its token's name (`background`, `ring`).
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct NamedColor {
    /// The token's name: `background`, `ring`.
    pub name: String,
    /// `#RRGGBB`, or `#RRGGBBAA` when it isn't opaque.
    pub value: String,
}

/// How the window looks, as an app saves it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct SavedAppearance {
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

impl From<dp_store::Appearance> for SavedAppearance {
    fn from(saved: dp_store::Appearance) -> Self {
        Self {
            dark: saved.dark,
            follow_system: saved.follow_system,
            dark_theme: saved.dark_theme,
            light_theme: saved.light_theme,
            texture: saved.texture,
            elevation: saved.elevation,
        }
    }
}

impl From<SavedAppearance> for dp_store::Appearance {
    fn from(shown: SavedAppearance) -> Self {
        Self {
            dark: shown.dark,
            follow_system: shown.follow_system,
            dark_theme: shown.dark_theme,
            light_theme: shown.light_theme,
            texture: shown.texture,
            elevation: shown.elevation,
        }
    }
}

/// Where the window was left: its bounds out of maximized and full screen, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, JsObject)]
pub struct WindowPlace {
    /// The left edge, in points from the primary display's left.
    pub x: f64,
    /// The top edge, in points from the primary display's top.
    pub y: f64,
    /// The width, in points; 0 when nothing was saved yet.
    pub width: f64,
    /// The height, in points; 0 when nothing was saved yet.
    pub height: f64,
    /// Whether the window was maximized.
    pub maximized: bool,
}

impl From<dp_store::WindowState> for WindowPlace {
    fn from(saved: dp_store::WindowState) -> Self {
        Self {
            x: f64::from(saved.x),
            y: f64::from(saved.y),
            width: f64::from(saved.width),
            height: f64::from(saved.height),
            maximized: saved.maximized,
        }
    }
}

impl From<WindowPlace> for dp_store::WindowState {
    fn from(place: WindowPlace) -> Self {
        Self {
            x: place.x as f32,
            y: place.y as f32,
            width: place.width as f32,
            height: place.height as f32,
            maximized: place.maximized,
        }
    }
}

const APP_THEME: &str = include_str!("../theme.json");

macro_rules! theme_files {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("../themes/", $name, ".json")))),*]
    };
}

const THEME_FILES: &[(&str, &str)] = theme_files![
    "abyss",
    "dark-high-contrast",
    "dark-modern",
    "dark-plus",
    "kimbie-dark",
    "light-high-contrast",
    "light-modern",
    "light-plus",
    "monokai",
    "monokai-dimmed",
    "powershell-ise",
    "quiet-light",
    "solarized-dark",
    "solarized-light",
    "tomorrow-night-blue",
];

/// The colors the app draws with: its own theme, or a theme from `themes/` over it.
#[module]
pub struct Themes;

#[module]
impl Themes {
    /// The names of the theme files in `themes/`.
    pub fn names() -> Vec<String> {
        THEME_FILES
            .iter()
            .map(|(name, _)| name.to_string())
            .collect()
    }

    /// Draws the app with theme file `name` over its own theme, or its own when `None`.
    /// Async because nsh takes new tokens from a task, not a plain call.
    pub async fn apply(name: Option<String>) -> Result<(), String> {
        set_tokens(theme_tokens(name.as_deref())?);
        Ok(())
    }

    /// Every color of theme file `name` (or the app's own) in the light or the dark.
    pub fn colors(name: Option<String>, dark: bool) -> Result<Vec<NamedColor>, String> {
        let mut colors: Vec<NamedColor> = theme_tokens(name.as_deref())?
            .colors()
            .map(|(name, color)| NamedColor {
                name: name.to_string(),
                value: if dark { color.dark } else { color.light }.to_hex(),
            })
            .collect();
        colors.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(colors)
    }
}

fn theme_tokens(name: Option<&str>) -> Result<nsh_core::element::Tokens, String> {
    let own = nsh_base::tokens_of(Some(APP_THEME))?;
    let Some(name) = name else { return Ok(own) };
    let (_, json) = THEME_FILES
        .iter()
        .find(|(each, _)| *each == name)
        .ok_or_else(|| format!("There is no theme called {name}."))?;
    Ok(nsh_base::theme_overrides(json).map_err(|e| e.0)?.over(&own))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(json: &str) -> Vec<String> {
        let value: serde_json::Value = serde_json::from_str(json).expect("a theme file is JSON");
        value["cssVars"]["theme"]
            .as_object()
            .expect("a theme file's values are in cssVars.theme")
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn every_theme_file_is_listed_and_parses() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("themes");
        let mut on_disk: Vec<String> = std::fs::read_dir(folder)
            .expect("the themes folder")
            .map(|entry| {
                let path = entry.expect("an entry").path();
                path.file_stem()
                    .expect("a name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        on_disk.sort();
        assert_eq!(on_disk, Themes::names());
        for (name, json) in THEME_FILES {
            nsh_base::theme_overrides(json).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }

    #[test]
    fn every_theme_sets_every_token() {
        let mut wanted = named(APP_THEME);
        wanted.extend(
            [
                "background",
                "foreground",
                "card",
                "card-foreground",
                "popover",
                "popover-foreground",
                "primary",
                "primary-foreground",
                "secondary",
                "secondary-foreground",
                "muted",
                "muted-foreground",
                "accent",
                "accent-foreground",
                "destructive",
                "border",
                "input",
                "ring",
                "sidebar",
                "sidebar-foreground",
                "sidebar-primary",
                "sidebar-primary-foreground",
                "sidebar-accent",
                "sidebar-accent-foreground",
                "sidebar-border",
                "sidebar-ring",
                "success",
                "warning",
                "info",
            ]
            .map(String::from),
        );
        for (name, json) in THEME_FILES {
            let set = named(json);
            for token in &wanted {
                assert!(set.contains(token), "{name} leaves out {token}");
            }
        }
    }

    #[test]
    fn the_development_host_saves_in_a_folder_nsh_never_watches() {
        let debug = Path::new("app").join("target").join("debug");
        assert_eq!(
            development_folder_of(&debug.join("nsh-app-mdviewer-windows-host.exe")),
            Some(debug.join(".nsh"))
        );
        assert_eq!(development_folder_of(&debug.join("mdviewer.exe")), None);
        assert_eq!(
            development_folder_of(&debug.join("nsh-app-mdviewer-test.exe")),
            None
        );
    }

    #[test]
    fn a_theme_goes_over_the_apps_own() {
        let own = theme_tokens(None).expect("the app's theme");
        let monokai = theme_tokens(Some("monokai")).expect("monokai");
        assert_eq!(
            monokai.color("background").map(|c| c.dark.to_hex()),
            Some("#272822".into())
        );
        assert_ne!(monokai.color("ring"), own.color("ring"));
        assert!(theme_tokens(Some("sunset")).is_err());
    }
}
