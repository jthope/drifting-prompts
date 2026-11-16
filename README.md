<p>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/drifting-prompts/logo/drifting-prompts-horizontal-inverse.svg">
    <img src="assets/brand/drifting-prompts/logo/drifting-prompts-horizontal-color.svg" alt="Drifting Prompts" width="320">
  </picture>
</p>

Small desktop apps, built in Rust: a viewer for your IP cameras and a reader for folders of Markdown.

The apps share one look: the same controls, the same themes (light, dark, high contrast and more) and the same
Settings. Each keeps its data in a folder beside it, with no account and no cloud service. The screens are TSX on
[nsh](https://github.com/longbridge/nsh), a native UI framework.

## The apps

### <img src="assets/brand/ipcamera/logo/ip-camera-icon-color.svg" width="28" align="absmiddle" alt=""> IP Camera

A viewer for the IP cameras on your network. It decodes H.264 and H.265 itself, in Rust, so there are no codecs or
media frameworks to install.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/screenshots/ipcamera-dark.png">
  <img src="assets/screenshots/ipcamera-light.png" alt="IP Camera showing four cameras in a 2 × 2 view: a front door, a driveway, a backyard and a side gate in night vision">
</picture>

- Live video from several cameras at once, with sound, in tile layouts you save
- Pan, tilt and zoom, on cameras that move
- Snapshots and recordings, including recording on motion or AI detections that starts a few seconds early
- A log of what the cameras saw
- RTSP for the video; Reolink's, Dahua's and Amcrest's own APIs and ONVIF for PTZ, detections and lights; and a
  search that finds the cameras on your network
- Camera passwords saved encrypted for your user account
- A server that runs the same app in a browser, without the desktop app

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/screenshots/ipcamera-dashboard-dark.png">
  <img src="assets/screenshots/ipcamera-dashboard-light.png" alt="IP Camera's dashboard: the cameras, videos, snapshots and storage, what was saved each day, and the week's events by hour">
</picture>

More in [IP Camera's README](crates/ipcamera/README.md).

### <img src="assets/brand/mdviewer/logo/md-viewer-icon-color.svg" width="28" align="absmiddle" alt=""> Markdown Viewer

A reader for folders of Markdown: notes, documentation, a repository's READMEs.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/screenshots/mdviewer-dark.png">
  <img src="assets/screenshots/mdviewer-light.png" alt="Markdown Viewer showing a project's sensor guide raw and rendered side by side, each with a map of the whole file, between the project's files on the left and the outline on the right">
</picture>

- Opens folders as projects, with their files in a tree, by folder or in one list
- Files open in tabs: a preview tab, pinned tabs, dragged into order
- Each file raw (highlighted), rendered, or both side by side
- The file's outline beside it, and a map of the whole file
- Search across a project's files, with the results in a tab of their own

More in [Markdown Viewer's README](crates/mdviewer/README.md).

## Status

Both apps are early, and Markdown Viewer reads files but doesn't edit them. There are no packaged downloads yet, so
build from source.

## Setup

These steps are for Windows and Linux. On either one you need:

- Rust 1.97 or newer
- nsh 0.5.1

Cargo builds the video stack from source, so there are no codecs or media frameworks to install.

### Windows

1. Install Rust with [rustup](https://rustup.rs).
2. Install nsh. It installs to `%LOCALAPPDATA%\nsh` and adds itself to your `PATH`, so open a new terminal afterward:

   ```powershell
   $env:NSH_VERSION = 'v0.5.1'
   irm https://nsh.dev/install.ps1 | iex
   ```

3. Clone this repository, and from its folder run an app:

   ```powershell
   cargo ipcamera
   ```

   or `cargo mdviewer`. The first build takes a few minutes; after that, builds take seconds.

Build from PowerShell, not Git Bash. Git ships its own `link.exe`, which can get picked up instead of Microsoft's
linker.

### Linux

Ubuntu 24.04 or later, or another distribution with GTK 4.14 or newer.

1. Install Rust with [rustup](https://rustup.rs):

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Install what nsh builds against (GTK 4, WebKitGTK, GStreamer), a C compiler, and clang and LLVM. On Ubuntu or
   Debian:

   ```sh
   sudo apt install build-essential pkg-config clang llvm libgtk-4-dev libwebkitgtk-6.0-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
   ```

   On Fedora:

   ```sh
   sudo dnf install gcc pkgconf-pkg-config clang llvm gtk4-devel webkitgtk6.0-devel gstreamer1-devel gstreamer1-plugins-base-devel
   ```

3. Install nsh. It installs to `~/.nsh` and links itself into `~/.local/bin`. If that folder isn't on your `PATH`, the
   installer tells you how to add it:

   ```sh
   curl -fsSL https://nsh.dev/install.sh | NSH_VERSION=v0.5.1 sh
   ```

4. Clone this repository, and from its folder run an app:

   ```sh
   cargo ipcamera
   ```

   or `cargo mdviewer`. The first build takes a few minutes; after that, builds take seconds.

### Optional: the web build

IP Camera's web build needs two more things, once per machine: the WebAssembly target and clang.
[IP Camera's README](crates/ipcamera/README.md#the-web-build) explains why.

On Windows, from `crates/ipcamera/app`:

```powershell
rustup target add wasm32-unknown-unknown
powershell -ExecutionPolicy Bypass -File tools\fetch-wasm-clang.ps1
```

The script downloads a pinned copy of clang (about 266 MiB) into `tools\llvm` and points cargo at it in
`.cargo\config.toml`. Git ignores both; delete them to undo it.

On Linux, clang and LLVM came with the packages above, so only the target is left:

```sh
rustup target add wasm32-unknown-unknown
```

## Common commands

Run these from the repository root.

```powershell
cargo ipcamera                # run IP Camera, with hot reload
cargo mdviewer                # run Markdown Viewer
cargo ipcamera-web            # run IP Camera's server and web build together, at http://localhost:8787/

cargo test --workspace        # test everything except the apps
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all

cargo package-ipcamera        # package IP Camera into crates\ipcamera\app\.nsh\out\
cargo package-mdviewer        # package Markdown Viewer into crates\mdviewer\app\.nsh\out\
```

Only one app can run under nsh at a time: starting one stops the other. Each app has its own tests; run `nsh test`
from its folder.

## Repository layout

Each product has its own folder under `crates/`. Code that more than one product uses lives in `crates/dp/`.

```
crates/
  ipcamera/      IP Camera: core, dpapi, media, storage, server, app
  mdviewer/      Markdown Viewer: core, app
  dp/            shared by every product: store, ui
  xtask/         the repo's own cargo commands (cargo ipcamera, cargo mdviewer, ...)
assets/brand/    logos and brand colors for the suite and each product
```

| Package | Folder | What it does |
| --- | --- | --- |
| `ipcamera-core` | `crates/ipcamera/core` | Cameras, groups, views and settings, and talking to cameras (PTZ, Reolink, Dahua, ONVIF) |
| `ipcamera-dpapi` | `crates/ipcamera/dpapi` | Encrypts the cameras' passwords for your user account (DPAPI) |
| `ipcamera-media` | `crates/ipcamera/media` | The video pipeline: RTSP, decoding, sound and recording |
| `ipcamera-storage` | `crates/ipcamera/storage` | Snapshots and recordings on disk |
| `ipcamera-server` | `crates/ipcamera/server` | Serves the cameras and the app to a browser ([README](crates/ipcamera/server/README.md)) |
| `ipcamera` | `crates/ipcamera/app` | The IP Camera desktop app ([README](crates/ipcamera/app/README.md)) |
| `mdviewer-core` | `crates/mdviewer/core` | Projects, the files in a project, and a file's outline |
| `mdviewer` | `crates/mdviewer/app` | The Markdown Viewer desktop app |
| `dp-store` | `crates/dp/store` | Settings files that save safely, shared by every product |
| `dp-ui` | `crates/dp/ui` | The shared controls, themes and window helpers |
| `xtask` | `crates/xtask` | The `cargo ipcamera`, `cargo mdviewer` and packaging commands |

The two apps and `dp-ui` are built with nsh, so each is its own Cargo workspace. The apps' rows give their nsh
package names; their crates are `nsh-app-ipcamera` and `nsh-app-mdviewer`. `cargo` at the root builds everything
else. [`crates/README.md`](crates/README.md) explains how the pieces fit together.

## Where to read more

- [`crates/README.md`](crates/README.md): the workspaces, checks and CI, releases, the shared crates, themes, and nsh
  workarounds
- [`crates/ipcamera/README.md`](crates/ipcamera/README.md): IP Camera as a whole: its crates, what it saves, releases
  and camera checks
- [`crates/ipcamera/app/README.md`](crates/ipcamera/app/README.md): everything you see in the IP Camera app
- [`crates/ipcamera/server/README.md`](crates/ipcamera/server/README.md): the server and its API
- [`crates/mdviewer/README.md`](crates/mdviewer/README.md): Markdown Viewer
- [`AGENTS.md`](AGENTS.md): guidance for AI coding agents working in this repository

## License

MIT. See [LICENSE](LICENSE).
