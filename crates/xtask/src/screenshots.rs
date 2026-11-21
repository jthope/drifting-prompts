//! `cargo screenshots`: each app's README screenshots, light and dark, in a real window on made-up
//! content. The development build's own settings (IP Camera's hold the cameras' passwords) are set
//! aside and put back whatever happens; a run stopped first leaves them for the next to put back.

mod dashboard;

use std::{
    ffi::OsString,
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::{IPCAMERA, MDVIEWER, NO_NSH, nsh, repository_root, runtime};

const IPCAMERA_SIZE: (u32, u32) = (1600, 900);
const MDVIEWER_SIZE: (u32, u32) = (1920, 1080);

const SETTLE: u32 = 5000;

/// Milliseconds to redraw after a mode switch: Markdown Viewer's file takes over two seconds.
const IPCAMERA_SWITCH: u32 = 2000;
const MDVIEWER_SWITCH: u32 = 6000;

/// Milliseconds a screen is given once its steps are done: the dashboard's charts grow in.
const DRAW_IN: u32 = 3000;

const TIME_LIMIT: u32 = 180_000;

const TEST_FILE: &str = "screenshots.test.ts";

/// The view comes first: a camera no page has shown stays disconnected. The search's window
/// stays open, so it goes last.
const IPCAMERA_SCREENS: &[Screen] = &[
    Screen {
        steps: &[],
        shots: ["ipcamera-light", "ipcamera-dark"],
    },
    Screen {
        steps: &[("label:Dashboard", "text:Latest events")],
        shots: ["ipcamera-dashboard-light", "ipcamera-dashboard-dark"],
    },
    Screen {
        steps: &[("label:Events", "id:preview")],
        shots: ["ipcamera-events-light", "ipcamera-events-dark"],
    },
    Screen {
        steps: &[
            ("label:Home", "text:192.168.1.21"),
            ("label:Add camera", "text:Search network for devices"),
            (
                "text:Search network for devices",
                "text:6 cameras answered.",
            ),
        ],
        shots: ["ipcamera-search-light", "ipcamera-search-dark"],
    },
];

const MDVIEWER_SCREENS: &[Screen] = &[Screen {
    steps: &[],
    shots: ["mdviewer-light", "mdviewer-dark"],
}];

/// Each camera's name, which names its picture in `cameras/`, and its made-up address.
const CAMERAS: [(&str, &str); 4] = [
    ("Front door", "192.168.1.21"),
    ("Driveway", "192.168.1.22"),
    ("Backyard", "192.168.1.23"),
    ("Side gate", "192.168.1.24"),
];

/// Address, ONVIF port, name and model (for one with ONVIF), and what else it says of itself.
const FOUND: [(&str, Option<u16>, &str, &str, &str); 6] = [
    (
        "192.168.1.21",
        Some(80),
        "Front door",
        "Doorbell D2",
        "MAC: 02:5c:a1:00:0e:21",
    ),
    (
        "192.168.1.22",
        Some(80),
        "Driveway",
        "Bullet B5",
        "MAC: 02:5c:a1:00:0e:22",
    ),
    (
        "192.168.1.23",
        Some(80),
        "Backyard",
        "Turret T4",
        "MAC: 02:5c:a1:00:0e:23",
    ),
    (
        "192.168.1.24",
        Some(80),
        "Side gate",
        "Bullet B5",
        "MAC: 02:5c:a1:00:0e:24",
    ),
    (
        "192.168.1.25",
        Some(8000),
        "IPC",
        "PTZ P8",
        "Serial: P8K2241907",
    ),
    ("192.168.1.40", None, "", "", "Video over RTSP"),
];

/// As wide as `crates/ipcamera/storage` makes thumbnails.
const THUMBNAIL_WIDTH: u32 = 480;

const HOME_VIEW: &str = "8d2e4f60-7a1b-4c3d-8e5f-6a7b8c9d0e01";

const PROJECTS: [&str; 3] = ["Weathervane", "Garden journal", "Recipes"];

pub fn take(arguments: Vec<String>) -> Result<ExitCode> {
    let products: &[&str] = match arguments.as_slice() {
        [] => &[IPCAMERA, MDVIEWER],
        [one] if one == IPCAMERA => &[IPCAMERA],
        [one] if one == MDVIEWER => &[MDVIEWER],
        _ => bail!(
            "`cargo screenshots` takes both apps' screenshots, or `ipcamera` or `mdviewer`'s alone"
        ),
    };
    let root = repository_root();
    let assets = root.join("assets").join("screenshots");
    let work = root.join("target").join("screenshots");

    for &product in products {
        let run = if product == IPCAMERA {
            let pictures = render_cameras(&assets, &work)?;
            let found = write_found(&work)?;
            Run {
                files: ipcamera_files(&work, &pictures)?,
                environment: vec![
                    ("IPCAMERA_SOURCE", OsString::from("pattern")),
                    ("IPCAMERA_PICTURES", pictures.into_os_string()),
                    ("IPCAMERA_FOUND", found.into_os_string()),
                ],
                size: IPCAMERA_SIZE,
                switch: IPCAMERA_SWITCH,
                screens: IPCAMERA_SCREENS,
            }
        } else {
            Run {
                files: mdviewer_files(&assets),
                environment: Vec::new(),
                size: MDVIEWER_SIZE,
                switch: MDVIEWER_SWITCH,
                screens: MDVIEWER_SCREENS,
            }
        };
        shoot(&root, product, &assets, &work, &run)?;
        for shot in run.screens.iter().flat_map(|screen| screen.shots) {
            println!(
                "cargo screenshots: {}",
                assets.join(format!("{shot}.png")).display()
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

struct Run {
    files: Vec<(PathBuf, Value)>,
    environment: Vec<(&'static str, OsString)>,
    size: (u32, u32),
    switch: u32,
    screens: &'static [Screen],
}

/// Each step is a press and what then shows; the shots are light then dark, without `.png`.
struct Screen {
    steps: &'static [(&'static str, &'static str)],
    shots: [&'static str; 2],
}

fn shoot(root: &Path, product: &str, assets: &Path, work: &Path, run: &Run) -> Result<()> {
    let app = root.join("crates").join(product).join("app");
    let debug = app.join("target").join("debug");
    let names: Vec<&Path> = run.files.iter().map(|(name, _)| name.as_path()).collect();
    let aside = Aside {
        debug,
        folder: work.join("kept").join(product),
    };
    if aside.folder.exists() {
        aside.put_back(&names).with_context(|| {
            format!(
                "couldn't put back {product}'s settings, which a run that didn't finish left in {}",
                aside.folder.display()
            )
        })?;
        println!(
            "cargo screenshots: put back {product}'s settings, which a run that didn't finish left aside"
        );
    }

    aside.set(&names)?;
    let shot = write_all(&aside.debug, &run.files)
        .and_then(|()| run_test(root, product, &app, &test_file(&app, assets, run), run));
    let put_back = aside.put_back(&names).with_context(|| {
        format!(
            "couldn't put back {product}'s settings: they are in {}",
            aside.folder.display()
        )
    });
    match (shot, put_back) {
        // Both failed: what was left aside matters more.
        (Err(error), Err(kept)) => {
            eprintln!("cargo screenshots: {error:#}");
            Err(kept)
        }
        (shot, put_back) => shot.and(put_back),
    }
}

/// A copy of each file in `debug` it writes over, or an empty `<name>.absent` for one that wasn't.
struct Aside {
    debug: PathBuf,
    folder: PathBuf,
}

impl Aside {
    fn set(&self, names: &[&Path]) -> Result<()> {
        let copied = names.iter().try_for_each(|name| {
            let from = self.debug.join(name);
            let to = self.folder.join(name);
            fs::create_dir_all(to.parent().unwrap_or(&self.folder))?;
            if from.exists() {
                fs::copy(&from, &to).map(|_| ())
            } else {
                fs::write(absent(&to), "")
            }
        });
        if let Err(error) = copied {
            let _ = self.put_back(names);
            return Err(error).context("couldn't set the app's settings aside");
        }
        Ok(())
    }

    fn put_back(&self, names: &[&Path]) -> Result<()> {
        for name in names {
            let kept = self.folder.join(name);
            let target = self.debug.join(name);
            if kept.exists() {
                // Copied in rather than renamed: a shell open in the folder would lock it.
                fs::create_dir_all(target.parent().unwrap_or(&self.debug))?;
                fs::copy(&kept, &target)?;
            } else if absent(&kept).exists() {
                match fs::remove_file(&target) {
                    Err(error) if error.kind() != io::ErrorKind::NotFound => {
                        return Err(error.into());
                    }
                    _ => {}
                }
            }
        }
        fs::remove_dir_all(&self.folder)?;
        // The shared `kept` folder, once it is empty.
        if let Some(kept) = self.folder.parent() {
            let _ = fs::remove_dir(kept);
        }
        Ok(())
    }
}

fn absent(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".absent");
    PathBuf::from(name)
}

fn write_all(debug: &Path, files: &[(PathBuf, Value)]) -> Result<()> {
    for (name, value) in files {
        let path = debug.join(name);
        fs::create_dir_all(path.parent().unwrap_or(debug))?;
        fs::write(&path, serde_json::to_string_pretty(value)?)
            .with_context(|| format!("couldn't write {}", path.display()))?;
    }
    Ok(())
}

fn test_file(app: &Path, assets: &Path, run: &Run) -> String {
    format!(
        "// Made by `cargo screenshots` (`crates/xtask/src/screenshots.rs`), which deletes it once \
         the app closes.\n\
         import {{ open, test }} from \"nsh/test\";\n\
         \n\
         test(\"screenshots\", async () => {{\n\
         const app = await open({entry}, {{ network: \"live\" }});\n\
         {steps}\
         }}, {TIME_LIMIT});\n",
        entry = script_string(&app.join("App.tsx")),
        steps = steps(assets, run),
    )
}

/// nsh opens the app at a phone's width, where it hides its sidebar, before this sizes it.
/// A press can go unheard just after a page shows, so each repeats until what it shows is there.
fn steps(assets: &Path, run: &Run) -> String {
    let (width, height) = run.size;
    let mut steps = format!(
        "await app.setViewport({width}, {height})\n\
         if (app.shows(\"label:Show sidebar\")) await app.press(\"label:Show sidebar\")\n\
         await app.setColorScheme(\"light\")\n\
         await app.wait({SETTLE})\n"
    );
    let mut dark = false;
    for screen in run.screens {
        for (press, shows) in screen.steps {
            let _ = writeln!(
                steps,
                "for (let tries = 0; tries < 5 && !app.shows(\"{shows}\"); tries++) {{ \
                 await app.press(\"{press}\"); await app.wait(1000) }}"
            );
        }
        if !screen.steps.is_empty() {
            let _ = writeln!(steps, "await app.wait({DRAW_IN})");
        }
        for switch in [false, true] {
            if switch {
                dark = !dark;
                let mode = if dark { "dark" } else { "light" };
                let _ = writeln!(steps, "await app.setColorScheme(\"{mode}\")");
                let _ = writeln!(steps, "await app.wait({})", run.switch);
            }
            let picture = assets.join(format!("{}.png", screen.shots[usize::from(dark)]));
            let _ = writeln!(steps, "await app.screenshot({})", script_string(&picture));
        }
    }
    steps
}

fn script_string(path: &Path) -> String {
    let path = path
        .to_string_lossy()
        .replace('\\', "/")
        .replace('"', "\\\"");
    format!("\"{path}\"")
}

/// On Ctrl+C this waits for nsh to close the app, so the settings are put back after it.
fn run_test(root: &Path, product: &str, app: &Path, test: &str, run: &Run) -> Result<()> {
    let folder = app.join(".nsh").join("test");
    let file = folder.join(TEST_FILE);
    fs::create_dir_all(&folder)?;
    fs::write(&file, test).with_context(|| format!("couldn't write {}", file.display()))?;

    let mut command = nsh(root, product);
    command.args([
        "test",
        &format!(".nsh/test/{TEST_FILE}"),
        "--target",
        "windows",
    ]);
    for (name, value) in &run.environment {
        command.env(name, value);
    }
    let ran = runtime().and_then(|runtime| {
        runtime.block_on(async {
            let mut nsh = command.spawn().context(NO_NSH)?;
            tokio::select! {
                biased;
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    tokio::select! {
                        _ = nsh.wait() => {}
                        _ = tokio::signal::ctrl_c() => {}
                    }
                    bail!("stopped before {product}'s screenshots were taken")
                }
                status = nsh.wait() => {
                    let status = status?;
                    if !status.success() {
                        bail!("{product}'s screenshots failed ({status}): nsh says why above");
                    }
                    Ok(())
                }
            }
        })
    });
    let _ = fs::remove_file(&file);
    ran
}

fn render_cameras(assets: &Path, work: &Path) -> Result<PathBuf> {
    let pictures = work.join("cameras");
    fs::create_dir_all(pictures.join("thumbnails"))?;
    let width = THUMBNAIL_WIDTH.to_string();
    for (name, _) in CAMERAS {
        let svg = assets.join("cameras").join(format!("{name}.svg"));
        if !svg.exists() {
            bail!("{name}'s picture isn't there: {}", svg.display());
        }
        let png = format!("{name}.png");
        resvg(&svg, &pictures.join(&png), &[])?;
        resvg(
            &svg,
            &pictures.join("thumbnails").join(&png),
            &["--width", &width],
        )?;
    }
    Ok(pictures)
}

fn resvg(svg: &Path, png: &Path, options: &[&str]) -> Result<()> {
    let status = std::process::Command::new("resvg")
        .args(options)
        .arg(svg)
        .arg(png)
        .status();
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => bail!("resvg couldn't render {} ({status})", svg.display()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => bail!(
            "couldn't start resvg, which renders the cameras' pictures: `cargo install resvg` installs it"
        ),
        Err(error) => Err(error).context("couldn't start resvg"),
    }
}

/// Writes [`FOUND`] as `ipcamera_core::FoundCamera`s to `work/found.json`.
fn write_found(work: &Path) -> Result<PathBuf> {
    let found: Vec<Value> = FOUND
        .iter()
        .map(|&(host, onvif_port, name, model, detail)| {
            let mut camera = json!({ "host": host, "rtsp_port": 554, "details": [detail] });
            if let Some(port) = onvif_port {
                camera["onvif_port"] = json!(port);
                camera["onvif_device_path"] = json!("/onvif/device_service");
                camera["name"] = json!(name);
                camera["model"] = json!(model);
            }
            camera
        })
        .collect();
    fs::create_dir_all(work)?;
    let file = work.join("found.json");
    fs::write(&file, serde_json::to_string_pretty(&found)?)
        .with_context(|| format!("couldn't write {}", file.display()))?;
    Ok(file)
}

fn ipcamera_files(work: &Path, pictures: &Path) -> Result<Vec<(PathBuf, Value)>> {
    let id = |index: usize| format!("3b0f6c1e-2f4a-4d8e-9a51-1c2d3e4f5a{index:02}");
    let cell = |column: u8, row: u8, columns: u8, rows: u8| json!({ "column": column, "row": row, "column_span": columns, "row_span": rows });
    let tile = |index: usize| json!({ "camera": id(index), "stream": "own" });
    let cameras: Vec<Value> = CAMERAS
        .iter()
        .enumerate()
        .map(|(index, (name, host))| json!({ "id": id(index + 1), "name": name, "host": host }))
        .collect();

    let media = work.join("ipcamera-media");
    let (snapshots, videos) = (media.join("snapshots"), media.join("videos"));
    if media.exists() {
        fs::remove_dir_all(&media)?;
    }
    let named: Vec<(String, &str)> = CAMERAS
        .iter()
        .enumerate()
        .map(|(index, (name, _))| (id(index + 1), *name))
        .collect();
    let events = dashboard::seed(&named, pictures, &snapshots, &videos)?;

    // Where the development host saves (`dp_ui::development_folder`).
    let store = PathBuf::from(".nsh");
    let settings = store.join("data").join("settings");
    Ok(vec![
        (
            store.join("settings.json"),
            json!({
                // The default appearance but following the system, which the steps switch.
                "ui": { "follow_system": true, "muted": true, "page": format!("view:{HOME_VIEW}") },
                "window": window(IPCAMERA_SIZE),
                "storage": {
                    "snapshots": snapshots.to_string_lossy(),
                    "videos": videos.to_string_lossy(),
                },
                "automation": { "log_events": true },
            }),
        ),
        (
            settings.join("cameras.json"),
            json!({ "version": 1, "cameras": cameras }),
        ),
        (
            store.join("data").join("events").join("events.json"),
            events,
        ),
        (
            settings.join("groups.json"),
            json!({ "version": 1, "groups": [] }),
        ),
        (
            settings.join("views.json"),
            json!({
                "version": 3,
                "views": [
                    {
                        "id": HOME_VIEW,
                        "name": "Home",
                        "cells": [cell(0, 0, 6, 6), cell(6, 0, 6, 6), cell(0, 6, 6, 6), cell(6, 6, 6, 6)],
                        "fit": "fill",
                        "tiles": [tile(1), tile(2), tile(3), tile(4)],
                    },
                    {
                        "id": "8d2e4f60-7a1b-4c3d-8e5f-6a7b8c9d0e02",
                        "name": "Outside",
                        "cells": [cell(0, 0, 8, 12), cell(8, 0, 4, 6), cell(8, 6, 4, 6)],
                        "fit": "fill",
                        "tiles": [tile(2), tile(3), tile(4)],
                    },
                ],
            }),
        ),
    ])
}

fn mdviewer_files(assets: &Path) -> Vec<(PathBuf, Value)> {
    let folder = |name: &str| {
        assets
            .join("projects")
            .join(name)
            .to_string_lossy()
            .into_owned()
    };
    let projects: Vec<Value> = PROJECTS
        .iter()
        .enumerate()
        .map(|(index, name)| {
            if index == 0 {
                json!({
                    "path": folder(name),
                    "last_file": "docs/sensors.md",
                    "tabs": [
                        { "file": "README.md", "pinned": true, "mode": "rendered" },
                        { "file": "docs/sensors.md", "mode": "split" },
                        { "file": "docs/configuration.md", "mode": "rendered" },
                    ],
                })
            } else {
                json!({
                    "path": folder(name),
                    "last_file": "README.md",
                    "tabs": [{ "file": "README.md", "mode": "rendered" }],
                })
            }
        })
        .collect();
    let store = PathBuf::from(".nsh");
    vec![
        (
            store.join("settings.json"),
            json!({
                "ui": { "follow_system": true, "outline_open": true, "map_open": true },
                "window": window(MDVIEWER_SIZE),
            }),
        ),
        (
            store.join("projects.json"),
            json!({ "selected": folder(PROJECTS[0]), "projects": projects }),
        ),
    ]
}

fn window((width, height): (u32, u32)) -> Value {
    json!({ "x": 160.0, "y": 90.0, "width": width, "height": height, "maximized": false })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn each_screen_is_taken_in_both_modes_switching_once_a_screen() {
        const SCREENS: &[Screen] = &[
            Screen {
                steps: &[],
                shots: ["one-light", "one-dark"],
            },
            Screen {
                steps: &[("label:Two", "text:Second")],
                shots: ["two-light", "two-dark"],
            },
        ];
        let run = Run {
            files: Vec::new(),
            environment: Vec::new(),
            size: (1200, 800),
            switch: 1500,
            screens: SCREENS,
        };
        let steps = steps(Path::new("/shots"), &run);
        let said: Vec<&str> = steps
            .lines()
            .filter_map(|line| {
                line.strip_prefix("await app.setColorScheme(\"")
                    .or_else(|| line.strip_prefix("await app.screenshot(\"/shots/"))
                    .or_else(|| line.contains("label:Two").then_some("press two"))
            })
            .map(|said| said.trim_end_matches(".png\")").trim_end_matches("\")"))
            .collect();

        assert_eq!(
            said,
            [
                "light",
                "one-light",
                "dark",
                "one-dark",
                "press two",
                "two-dark",
                "light",
                "two-light"
            ]
        );
        assert_eq!(steps.matches("await app.wait(1500)").count(), 2);
        assert!(
            test_file(Path::new("/app"), Path::new("/shots"), &run)
                .contains(&format!("}}, {TIME_LIMIT});"))
        );
    }

    #[test]
    fn a_path_in_the_steps_is_a_script_string_with_forward_slashes() {
        assert_eq!(
            script_string(Path::new(r"D:\a b\ipcamera-light.png")),
            "\"D:/a b/ipcamera-light.png\""
        );
    }

    #[test]
    fn a_missing_file_is_noted_beside_its_name() {
        let note = absent(Path::new("data/settings/views.json"));
        assert_eq!(note.extension(), Some(OsStr::new("absent")));
        assert!(note.to_string_lossy().ends_with("views.json.absent"));
    }

    #[test]
    fn settings_set_aside_come_back_as_they_were() {
        let debug = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        fs::write(debug.path().join("settings.json"), "mine").unwrap();
        let aside = Aside {
            debug: debug.path().to_path_buf(),
            folder: work.path().join("kept"),
        };
        let names = [Path::new("settings.json"), Path::new("data/views.json")];

        aside.set(&names).unwrap();
        write_all(
            debug.path(),
            &[
                (PathBuf::from("settings.json"), json!({})),
                (PathBuf::from("data/views.json"), json!({})),
            ],
        )
        .unwrap();
        aside.put_back(&names).unwrap();

        assert_eq!(
            fs::read_to_string(debug.path().join("settings.json")).unwrap(),
            "mine"
        );
        assert!(!debug.path().join("data/views.json").exists());
        assert!(!aside.folder.exists());
    }
}
