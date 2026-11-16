//! What the suite's apps share to look and behave alike, on the Rust side: the themes' colors
//! (`Themes`, which each app registers in its own native module), how the window looks and where
//! it was left, as each app saves them ([`SavedAppearance`], [`WindowPlace`], from `dp_store`'s
//! `Appearance` and `WindowState`), where an app saves under nsh's tools ([`is_test_runner`],
//! [`development_folder`]), and on Windows the folder picker (`folder_dialog`) and showing files
//! in Explorer (`explorer`). [`listen`] and [`listen_later`] are how every bridge's listeners look
//! for changes.
//!
//! The styled controls themselves are TSX, beside this crate (`index.ts`): each app imports them
//! by path and hands them its `Themes` module (`connectThemes`), since each app's native module
//! goes by its own name (`ipcamera/native`).
//!
//! An app registers the module with the rest of its own:
//!
//! ```ignore
//! #[native_module("ipcamera/native")]
//! fn native(registry: &mut Registry) {
//!     registry.module::<dp_ui::Themes>();
//! }
//! ```

#[cfg(windows)]
pub mod explorer;
#[cfg(windows)]
pub mod folder_dialog;
mod listen;

use std::path::{Path, PathBuf};

use nsh_core::prelude::*;

pub use crate::listen::{listen, listen_later};

/// The end of the name nsh gives its headless test runner.
const TEST_RUNNER_SUFFIX: &str = "-test";

/// The end of the name nsh gives its development host (`nsh-app-mdviewer-windows-host`), which
/// `nsh run` starts.
const DEVELOPMENT_HOST_SUFFIX: &str = "-host";

/// The folder beside the development host that it saves in: one nsh's development server never
/// looks in.
const DEVELOPMENT_FOLDER: &str = ".nsh";

/// Whether this is `nsh test`'s headless runner.
///
/// It sits beside the development host, so an app that keeps its settings beside its executable
/// would read and write the same ones from every test. An app gives the runner settings of its own
/// in a throwaway folder instead.
pub fn is_test_runner() -> bool {
    std::env::current_exe()
        .ok()
        .is_some_and(|exe| name_ends_with(&exe, TEST_RUNNER_SUFFIX))
}

/// Where an app keeps the files it saves when it runs in nsh's development host: a `.nsh` folder
/// beside the host. `None` for any other executable, which saves beside itself.
///
/// A workaround for nsh 0.5.1, whose development server takes a change to any `.json` file in the
/// app's folder for a change to its TSX and builds the app again, `target` included, which it
/// skips only when it looks for Rust changes. The development host is in the app's
/// `target\debug`, so every window move, preference or tab saved beside it showed as a build. The
/// server never looks in a folder named `.nsh`. A release whose server skips `target` for TSX too
/// replaces this with the folder beside the executable, as for a packaged app.
pub fn development_folder() -> Option<PathBuf> {
    development_folder_of(&std::env::current_exe().ok()?)
}

/// [`development_folder`] for the executable at `exe`.
fn development_folder_of(exe: &Path) -> Option<PathBuf> {
    if !name_ends_with(exe, DEVELOPMENT_HOST_SUFFIX) {
        return None;
    }
    Some(exe.parent()?.join(DEVELOPMENT_FOLDER))
}

/// Whether the name of the executable at `exe`, without its `.exe`, ends with `suffix`.
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

/// How the window looks, as an app saves it in its `settings.json` (`dp_store::Appearance`) and
/// hands it to `showAppearance` and `keepAppearance` (`appearance.tsx`) among its preferences.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct SavedAppearance {
    /// Whether the window is dark: the mode chosen, or the one the system showed last while it
    /// follows the system.
    pub dark: bool,
    /// Whether the window follows the system's light or dark mode, rather than `dark`.
    pub follow_system: bool,
    /// The theme for when the window is dark: `default-dark` or a theme file's name
    /// (`dark-modern`).
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

/// Where the window was left, as an app saves it in its `settings.json` and hands it to
/// `keepWindowPlace` (`window.ts`), which gives it to nsh: the window's bounds out of maximized and
/// full screen, in points on the screen as `nsh/desktop` measures them, and whether it was
/// maximized.
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

/// The apps' own theme file, which each app's `nsh.json` names too. A theme from `themes/` goes
/// over it while it shows, so the tokens a theme leaves alone (`radius`) stay as this file has
/// them.
const APP_THEME: &str = include_str!("../theme.json");

/// The theme files in `themes/`, by name: each one as `theme.json` is, with a value for every token.
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

    /// Draws the app with theme file `name` over its own theme, or with its own theme again when it
    /// is `None`. Classes and `token()` follow at once. Async because nsh takes a new set of tokens
    /// from a task, not from a plain call. Rejects a name with no file, and the app keeps the
    /// colors it had.
    pub async fn apply(name: Option<String>) -> Result<(), String> {
        set_tokens(theme_tokens(name.as_deref())?);
        Ok(())
    }

    /// Every color of theme file `name` over the app's own theme, or of the app's own theme when it
    /// is `None`, in the light (`dark` false) or the dark, whatever theme the app shows now: what a
    /// theme picker draws each theme in.
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

/// The tokens the app draws with for theme file `name`: nsh's default theme with `theme.json` over
/// it, as nsh draws the app before it picks another, and the theme file over that.
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

    /// The tokens `json` (a theme file) gives a value.
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

    /// A theme sets every token the apps' own theme file adds and every one of nsh's they draw
    /// with, so nothing of the apps' own colors shows through it.
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
        // A packaged app and the test runner are not the development host.
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
        // What a theme sets is its own, in place of the app's.
        assert_ne!(monokai.color("ring"), own.color("ring"));
        assert!(theme_tokens(Some("sunset")).is_err());
    }
}
