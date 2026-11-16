# AGENTS.md

Drifting Prompts is a monorepo of products, each in a folder of its own under `crates/`, with what they share in
`crates/dp/` as `dp-*` crates. A crate's package name carries its product (`ipcamera-core`), since Cargo needs every
name in a workspace to differ and `core` is Rust's own; its folder doesn't (`crates/ipcamera/core`). A piece moves to
`crates/dp/` when a second product needs it, and not before: `dp-store` (safe settings files, the lock helper) and
`dp-ui` (the styled TSX controls, the themes, and the Rust under them) moved there when Markdown Viewer came.

The second product is Markdown Viewer (`crates/mdviewer`, the name for now): folders of Markdown opened as projects
from New, listed in IP Camera's sidebar, their `.md` files in a panel to the left of the file (as a tree, by folder or
in one list), open in tabs (a preview, pinned ones, dragged into order), each raw (nsh's code editor, highlighted),
rendered (nsh's `TextView`) or both, with its outline on the right. `crates/mdviewer/app` is its
nsh app and `crates/mdviewer/core` (`mdviewer-core`) its plain Rust: projects and settings, the files in a project,
and a file's outline. Its README is `crates/mdviewer/README.md`.

The first product is IP Camera, a desktop viewer for IP cameras: live RTSP video from several cameras in saved tile
layouts, PTZ over Dahua DVRIP, snapshots and recordings. `crates/ipcamera/server` is its server, which relays the
cameras' live video to a browser with no desktop app, and takes their snapshots, records them and moves them there:
the start of a web-only build. The app is `crates/ipcamera/app`, on nsh (Native Shell) 0.5.1: TSX for the interface,
and a Rust crate of its own for the cameras and the live picture, which it draws with Direct2D. Under it,
`crates/ipcamera/core` (settings, cameras, groups, views, RTSP addressing, PTZ) and `crates/ipcamera/media` (the
glass2glass pipeline: RTSP, decoding, sound, recording) are plain Rust that cargo builds whole: nothing to install,
nothing on `PATH`. `crates/ipcamera/storage` keeps the snapshots and recordings on disk by one set of rules for the
app and the server. The web build runs on the server (without one it is a preview with no live video).

This file is the short version. The details are in `README.md` (setup), `crates/README.md` (the workspaces and what
the products share), `crates/ipcamera/README.md` and `crates/mdviewer/README.md` (each product),
`crates/ipcamera/app/README.md` and `crates/ipcamera/server/README.md` (IP Camera's app and server), and the module
docs (`//!`), which are the reference for how a module works inside.

## Layout

```
Cargo.toml            The root workspace: every crate but the apps and `dp-ui`; every dependency declared once
crates/
  dp/                 what the products share, as `dp-*` crates
    store/            dp-store: a document saved in a file (JSON, write-then-rename, never over a file it
                      could not read, another program's change read first: `lib.rs`), the lock helper every
                      crate uses (`sync.rs`), and what every product saves of its window (`window.rs`: `Appearance`,
                      `WindowState`); in the root workspace
    ui/               dp-ui, a workspace of its own (built with nsh, like the apps): the styled TSX controls every
                      app composes, exported from `index.ts` (`brand.tsx` the themes and `look()`, `elevation.ts`,
                      `texture.ts`, `appearance.tsx` the Appearance settings, the title bar's theme menu and the saved appearance shown and kept (`showAppearance`, `keepAppearance`), `editor.tsx` the dialog the editors open in (`EditorDialog`),
                      `settings.tsx` every product's Settings in that dialog (`SettingsDialog`) and the Appearance section they share
                      (`appearanceSection`), `count.tsx` a search's count of matches, `tree.tsx` a tree (picking several
                      items) and an outline, `tabs.tsx` a page's tabs and a reader's file tabs (rounded cards with a pin
                      and a close button, in one row that scrolls or wrapped onto more), `breadcrumbs.tsx` the way to
                      what a pane shows (`Breadcrumbs`), `island.tsx` a pane as an
                      island on the window's frame (`Frame`, `Islands`, `Island`, `GRIP_HANDLE`), in one of three steps
                      (`Look.tiers`), `nav.tsx` the sidebar (`SidebarFrame`) and its rows, `scrollbar.ts` the
                      suite's always-shown, wider scroll bars (`SCROLLBARS`), `window.ts` the window's place and
                      full screen, `reorder.tsx`
                      dragging items into order down a column or along a row (along its own row where they wrap),
                      `logos.ts` every product's logo); `theme.json`, every app's own tokens over nsh's, and
                      `themes/`, the other themes; its Rust (`src/`): the `Themes` module each app registers
                      `SavedAppearance` and `WindowPlace`, how the window looks and where it was left (`lib.rs`), `listen` and `listen_later` for
                      the bridges' listeners (`listen.rs`), and the folder picker
                      (`folder_dialog.rs`) and Explorer (`explorer.rs`), the only `unsafe` outside IP Camera's app
                      and `ipcamera-dpapi`
  ipcamera/           IP Camera
    core/             ipcamera-core: the settings file (`config.rs`: `settings.json`, its only writer), a camera's
                      settings and their one check (`camera_config.rs`), groups (`camera_group.rs`), views
                      (`camera_view.rs`), all three in `data\settings` (`library.rs`, the only way in; `camera_store.rs`, `group_store.rs`,
                      `view_store.rs` behind it, with `records.rs` their shared rules; `saved_password.rs`, each
                      password in `cameras.json` encrypted for the user account; `change.rs`, every change as one
                      value that the desktop and the server both apply), all of them on `dp-store`
                      (safe saving, or in memory for the web preview), RTSP addressing (`rtsp_url.rs`),
                      the PTZ contract, Dahua mapping, DVRIP client and command queue, its transport picked by the
                      camera's `Brand` (`ptz/`: DVRIP or Dahua's HTTP API, Reolink's API, or ONVIF), what a camera does beyond its stream in
                      no brand's terms (`camera_features.rs`: motion, AI detections, a spotlight; `StreamPath`, an RTSP
                      path or a Reolink's `flv:` stream), Reolink's HTTP API and its client, its FLV streams among
                      them (`reolink.rs`, `reolink/`), Dahua's HTTP API (`dahua_http.rs`: the CGI or
                      the XML `/dev-api/` Amcrest Link's bulbs speak, the digest login, PTZ, a white light), ONVIF's device, event, media and PTZ services (`onvif.rs`:
                      over the `oxvif` crate, which speaks the protocol; here what the app asks, a refused login kept
                      apart, which topics mean motion, a bulb's light), finding the cameras on the network with no login (`camera_search.rs`: ONVIF's discovery, and RTSP's port for those without it), watching a camera for those on a thread of its own
                      (`camera_watch.rs`) and telling a camera's brand from its address (`camera_detect.rs`; these
                      seven not in the web build), what a watch saw as events (`camera_event.rs`) and their log in `data\events`
                      (`event_log.rs`), how an event's videos are made, Settings → Automation's or a camera's own
                      (`event_video.rs`), the one way to take a lock (`sync`, `dp-store`'s); no UI, no media stack, no nsh
    dpapi/            ipcamera-dpapi: DPAPI behind two safe calls (`protect`, `unprotect`), for the user account
                      running the program, which `core`'s `saved_password.rs` encrypts the cameras' passwords
                      with; the root workspace's only `unsafe`
    media/            ipcamera-media: one RTSP session per camera on a thread of its own (`session.rs`: reconnecting,
                      volume, the failures never retried), asking the camera what it sends and building the glass2glass
                      graph for it (`pipeline.rs`), H.264 and H.265 in Rust (`avc.rs` on rust_h264, `hevc.rs` on
                      rust_h265), the size tap and the last
                      video element (`sink.rs`), the pass-through element both taps are (`tap.rs`), the decode gate,
                      which decodes nothing while no tile shows a camera (`gate.rs`), the newest decoded
                      picture (`frame.rs`: BGRA bytes in a slot), recording to fragmented MP4 by stream copy,
                      splitting a long or large file at a keyframe and starting from the seconds held back before
                      (`record.rs`), a Reolink's FLV stream as a source, with a token from its API (`flv.rs`),
                      relaying the compressed video to viewers (`relay.rs`), decoding one picture from
                      a relay when asked (`still.rs`), how large a camera's RTSP streams are, from their answer
                      to DESCRIBE, for Detect (`sizes.rs`), reading Annex-B access units and their codec string
                      (`annexb.rs`); the pretend camera, a local H.264 RTSP camera behind the `pretend-camera` feature
                      (`pretend.rs`, `examples/pretend_camera.rs`); no renderer
    storage/          ipcamera-storage: snapshots and recordings on disk, for the app and the server alike: their
                      names (an event's tagged with what the camera saw), listing and change detection
                      (`saved_file.rs`), writing a PNG and making the thumbnails any lack (`snapshot.rs`),
                      naming a recording (`recording.rs`), the small PNG of each that the pages show
                      (`thumbnail.rs`), what all media hold
                      and making room under a limit (`quota.rs`), a drive's free space (`space.rs`); no Explorer
    server/           ipcamera-server: the app without the desktop, for a browser; its routes, the packaged web build
                      it serves and the this-machine-only check (`lib.rs`), the library the web build reads and
                      changes, its status, snapshots, recording, PTZ, spotlight and Detect requests (`api.rs`), one
                      relay session per camera started by a viewer, a snapshot or a recording, with its camera's watch
                      once it is live (`streams.rs`), its password and who signed in (`access.rs`), PTZ connections and moves
                      that stop by themselves (`ptz.rs`), closing both off the runtime's threads and waiting for them
                      as the server ends (`closing.rs`), the saved files and their folders (`files.rs`), one viewer's
                      WebSocket and what goes over it (`live.rs`); `player/`, the page and its plain-JS WebCodecs
                      player, built in (`page.rs`)
    app/              the app, nsh package `ipcamera`, on nsh 0.5.1, a Cargo workspace of its own, so `cargo` at
                      the root never builds it:
      App.tsx           the shell: the window's state, its panels and its shortcuts
      views/            each region (sidebar, header, stage, PTZ panel, status bar, Dashboard, Settings, Snapshots, Videos,
                        Events);
                        `model.ts` what every region reads, `actions.ts` what buttons and menus do (every menu's items);
                        the styled controls are `crates/dp/ui`'s, imported as `../../../dp/ui`
      src/              the crate (`nsh.json` `"rust": "."`): `bridge.rs`, everything TSX imports from `ipcamera/native`;
                        `camera.rs`, what it calls on the desktop, over `camera/` (`hub.rs`,
                        every camera; `device.rs`, one camera; `feed.rs`, its frames); `web_backend.rs`, the web's:
                        a copy of the server's library, its player, its status, snapshots, recordings and PTZ, or a
                        preview with no server; what both share:
                        `library.rs` (groups, views and changes to several cameras, read from either backend's
                        `Library`, changed as `ipcamera_core` `Change`s), `form.rs` and `form_check.rs` (the camera form and its
                        checks); `video.rs`, the `ipcamera.video` Direct2D component; the window's place, the
                        folder picker and Explorer are `dp-ui`'s
      *.test.ts(x)      headless tests (`nsh test`)
      tools/            `fetch-wasm-clang.ps1`: clang for the web build, into `tools\llvm`, which git ignores
  mdviewer/           Markdown Viewer
    core/             mdviewer-core: `settings.json` (`config.rs`: the window's preferences and place) and
                      `projects.json` (`projects.rs`: the projects, the one open, each one's tabs and last file),
                      opened together (`saved.rs`, on `dp-store`), the Markdown files of a project and reading one
                      (`files.rs`: walked in the panel's order, build and hidden folders skipped, a name never
                      leaving the project), a file's outline and length (`outline.rs`, on pulldown-cmark: top-level
                      headings with their lines), what each part of a file's text is, for the raw view to color
                      (`syntax.rs`, the same parse: lines and UTF-16 columns, as the editor counts), finding text in
                      a project's files as the raw view's search finds it (`search.rs`, on `regex`, the crate nsh's
                      editor finds with); no UI, no nsh
    app/              the app, nsh package `mdviewer`, a Cargo workspace of its own like IP Camera's:
      App.tsx           the shell: sidebar, header, the files panel, the tabs over the reader, the outline, status bar,
                        the shortcuts
      views/            `model.ts` what every region reads (the projects, the tabs and the file in front, where the
                        reader is), `actions.ts` what buttons, rows, tabs, keys, menus and links do; `sidebar.tsx`,
                        `header.tsx`, `files.tsx` (Tree, Folder or List, picking several), `tabstrip.tsx` (the tabs,
                        scrolling or wrapping as Settings says, and the breadcrumbs under them),
                        `reader.tsx` (Raw, Split, Rendered and the outline), `newproject.tsx`, `settings.tsx`,
                        `statusbar.tsx`; `tabs.ts`, `document.ts` and `tree.ts`, what they work out, tested;
                        `syntax.ts`, the theme's syntax colors (`syntax-*` tokens) on Raw's decorations and Rendered's parts;
                        `scrollsync.ts`, Split's two sides scrolling together (the header's toggle); `searchbar.tsx`,
                        the title bar's search box and its drop-down (nsh's `Command` in a `Popover`); `search.ts`,
                        what the search works out: where each scope looks, the result rows, the counts, F3's steps
                        across the files, Raw's marks, the page's highlights; `minimap.ts`, where the scroll bars'
                        marks of a whole file (headings, matches, the reader's section) and the heading rail's lines
                        go, and the maps' pictures (Raw's lines, the page's block shapes) and previews; `rail.tsx`,
                        the heading rail, the outline's stand-in while it is hidden; `map.tsx`, the map of the whole
                        file beside each view, on `Canvas`; `results.tsx`, the search's results as a tab of their own
      src/              `bridge.rs`, everything TSX imports from `mdviewer/native`; `dropped.rs`, a dropped folder's `file:` URL as a path
  xtask/              the repository's own commands, run through cargo (aliases in `.cargo/config.toml`), each
                      product's by its name: `cargo ipcamera` is `nsh run` in crates/ipcamera/app; `cargo
                      ipcamera-web` starts ipcamera-server on the desktop app's `crates\ipcamera\app\target\debug`
                      settings and `nsh run --target web` against it; `cargo package-ipcamera` and `cargo
                      package-ipcamera-web` run `nsh package` for each; `cargo mdviewer` and `cargo package-mdviewer`
                      the same for Markdown Viewer; `cargo screenshots` the READMEs' screenshots of both
                      (`screenshots.rs`); it depends on none of the crates
assets/brand/         the brands the products share, a folder each (`drifting-prompts/`, `ipcamera/`, `mdviewer/`): its
                      logos and its `brand.json`; `ipcamera/ip-camera-app-icon.svg` and
                      `mdviewer/md-viewer-app-icon.svg` are the sources of each app's `icon.png`, and
                      `ipcamera/logo/ip-camera-icon-color.svg` and `mdviewer/logo/md-viewer-icon-color.svg` of the
                      marks in `crates/dp/ui/logos.ts`
assets/screenshots/   the READMEs' screenshots of each app (and IP Camera's dashboard, events and network search), light and dark, which
                      `cargo screenshots` takes, and what they show: the cameras' pictures (`cameras/`, an SVG
                      named for each camera) and Markdown Viewer's projects (`projects/`, which its `App.test.tsx` drops too), all made up; the
                      dashboard's events and files are made as it runs (`crates/xtask/src/screenshots/dashboard.rs`)
```

Dependency direction: `ipcamera-core` ← `ipcamera-media` ← `ipcamera-storage` ← `crates/ipcamera/app` (which also uses
`core` and `media` directly), and the same three ← `ipcamera-server`, which uses nothing of the app's. `core`
depends on neither `media` nor any UI, so `cargo test -p ipcamera-core` runs with no system library at all. `media`
depends on no renderer: frames leave it as bytes, and the app copies the newest one into a Direct2D bitmap. In
the app, `media`, `storage` and `nsh-platform-windows` are `cfg(windows)` dependencies, which keeps the camera stack
out of the web build; `core` is built everywhere, so the web preview keeps its cameras, groups and views with the desktop's own rules.
On the web, `core` makes its ids with a count rather than at random (`ids.rs`): randomness there needs JavaScript glue
the web build doesn't have. `ipcamera-dpapi` ← `core` (`cfg(windows)`).

Under both products: `dp-store` ← `ipcamera-core` and `mdviewer-core`; `dp-ui` ← each app, which imports its TSX by
path and registers its `Themes` module. `mdviewer-core` ← `crates/mdviewer/app`. Neither product depends on the
other, and `dp-*` crates depend on no product.

## Documentation

| Changing | Read |
|---|---|
| setup, the toolchain, build errors | `README.md` "Setup", "Troubleshooting"; `crates/README.md` "The workspaces" |
| what is saved, and where | `crates/ipcamera/README.md` "What it saves", `crates/mdviewer/README.md` "What it saves", `crates/ipcamera/core/src/config.rs` |
| the cameras' passwords in `cameras.json` | `crates/ipcamera/README.md` "What it saves", `crates/ipcamera/core/src/saved_password.rs`, `crates/ipcamera/dpapi/src/lib.rs` |
| cameras, groups and views | `crates/ipcamera/app/README.md` "Cameras, groups and views", `crates/ipcamera/core/src/library.rs` |
| a view's tiles on the 12 × 12 grid, the layouts, editing a layout | `crates/ipcamera/app/README.md` "Cameras, groups and views", `crates/ipcamera/core/src/camera_view.rs`, `views/arrange.ts`, `views/arranger.tsx` |
| the Cameras page: the table, selecting, a camera's dialog, the streams a camera lists, editing several cameras at once | `crates/ipcamera/app/README.md` "The Cameras page", `views/roster.ts`, `views/settings.tsx`, `views/finder.tsx` (finding cameras on the network), `views/streams.ts`, `crates/dp/ui/editor.tsx` |
| the Groups page and a group's dialog | `crates/ipcamera/app/README.md` "The Groups page", `views/grouplist.ts`, `views/members.tsx` |
| what both pages' tables share: order, selection, pages | `views/listing.ts`, `views/paged.tsx` |
| the stream: RTSP, a Reolink's FLV streams, decoding, sound, reconnecting | `crates/ipcamera/README.md` "What it needs", "How the code is organized", `crates/ipcamera/media/src/session.rs`, `pipeline.rs`, `flv.rs`; `crates/ipcamera/core/src/reolink.rs` |
| PTZ | `crates/ipcamera/core/src/ptz.rs` module docs |
| automation: recording on detection, event videos (post-roll, pre-roll, splitting, kinds, a camera's own), the storage limits | `crates/ipcamera/app/README.md` "Events", "Recording and videos", "Settings"; `crates/ipcamera/app/src/camera/hub.rs`, `device.rs`, `views/eventvideo.ts`; `crates/ipcamera/core/src/event_video.rs`, `config.rs` (`Automation`); `crates/ipcamera/media/src/record.rs`; `crates/ipcamera/storage/src/quota.rs` |
| events: the log, grouping, what they save and its tags, the Events page, alerts | `crates/ipcamera/app/README.md` "Events"; `crates/ipcamera/core/src/camera_event.rs`, `event_log.rs`, `camera_watch.rs`; `crates/ipcamera/storage/src/saved_file.rs`; `crates/ipcamera/app/views/linked.ts` |
| motion, AI detections, a spotlight; Reolink's API, Dahua's HTTP API, ONVIF events; brands and Detect | `crates/ipcamera/app/README.md` "Live video", "The Cameras page"; `crates/ipcamera/core/src/camera_features.rs`, `camera_watch.rs`, `camera_detect.rs`, `reolink.rs`, `dahua_http.rs`, `onvif.rs` module docs |
| the live picture | `crates/ipcamera/app/README.md` "Live video", `src/video.rs`, `views/picture.tsx` |
| snapshots, recording and the videos page | `crates/ipcamera/app/README.md` "Snapshots", "Recording and videos", `crates/ipcamera/media/src/record.rs` |
| what the snapshots, videos and events pages share: filters, sizes, selecting, the grid, the viewer | `crates/ipcamera/app/README.md` "Snapshots", `views/library.ts`, `views/grid.tsx`, `views/viewer.tsx` |
| themes, textures, elevation, islands; the shared controls | `crates/README.md` "dp-ui", "Themes", "Theme files", "Islands", "Textures", "Elevation"; `crates/dp/ui/index.ts`, `brand.tsx`, `src/lib.rs` |
| what the products share, and moving more there | `crates/README.md`, `crates/dp/store/src/lib.rs`, `crates/dp/ui/src/lib.rs` |
| Markdown Viewer: projects, the files panel and its views, tabs and the breadcrumbs, Raw, Split and Rendered, the outline, the search and its results' tab, the scroll bars' marks, the heading rail and the map | `crates/mdviewer/README.md`; `crates/mdviewer/core/src/config.rs`, `files.rs`, `outline.rs`, `search.rs`; `crates/mdviewer/app/views/files.tsx`, `tree.ts`, `tabs.ts`, `tabstrip.tsx`, `reader.tsx`, `document.ts`, `search.ts`, `searchbar.tsx`, `results.tsx`, `minimap.ts`, `rail.tsx`, `map.tsx` |
| Settings, in its dialog | `crates/ipcamera/app/README.md` "Settings", `crates/mdviewer/README.md` "Settings", `crates/dp/ui/settings.tsx`, `editor.tsx` |
| the dashboard | `crates/ipcamera/app/README.md` "Dashboard", `views/overview.ts` |
| the web preview, and web streaming later | `crates/ipcamera/app/README.md` "The web" |
| the server, relaying, the browser player | `crates/ipcamera/server/README.md`, `crates/ipcamera/server/src/lib.rs`, `live.rs`, `crates/ipcamera/media/src/relay.rs` |
| snapshots, recording and PTZ on the web | `crates/ipcamera/server/README.md`, `crates/ipcamera/server/src/streams.rs`, `ptz.rs`, `files.rs`, `crates/ipcamera/media/src/still.rs`, `crates/ipcamera/app/src/web_backend.rs` |
| how snapshots and recordings are named, listed and written | `crates/ipcamera/storage/src/lib.rs` |
| something nsh 0.5.1 lacks | `crates/README.md` "Workarounds for nsh 0.5.1" |
| the checks, CI, the xtask commands, `.cargo/config.toml` | `crates/README.md` "The workspaces", "Checks and CI" |
| releasing either app, `release.yml` | `crates/README.md` "Releases", each product's README "Releases" |
| the READMEs' screenshots | `crates/README.md` "Screenshots", `crates/xtask/src/screenshots.rs` |

Keep them current: a change that alters documented behavior updates its README section or module doc in the same
change. Each README has one job:

- `README.md` is for people: what the suite is, its packages, setting up a machine, the common commands and the build
  errors to expect. It is written plainly, for a developer reading it for the first time, and stays short; detail goes
  in the READMEs below, and the root links to them.
- `crates/README.md` is what the products share: the workspaces, the checks and CI, releases, `dp-store`, `dp-ui`,
  the themes, the workarounds for nsh 0.5.1 and the brands.
- `crates/<product>/README.md` is each product as a whole (`crates/ipcamera/README.md`: its crates, what it needs, the
  web build, releases, what it saves, the camera checks; `crates/mdviewer/README.md`: all of Markdown Viewer).
- `crates/ipcamera/app/README.md` is everything you see in IP Camera's app, and `crates/ipcamera/server/README.md` the
  server and its API.

Those below the root are plain prose that says what the code does and why.

## Commands

Run cargo from PowerShell, not Git Bash: Git ships its own `link.exe`, which can shadow the Microsoft
linker.

```powershell
# The root workspace (core, media)
cargo test -p ipcamera-core                                  # the fast subset: no media stack, no display, milliseconds
cargo test -p mdviewer-core -p dp-store                      # as fast: Markdown Viewer's projects, files and outline; the shared store
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                             # --check to only report
cargo run -p ipcamera-media --features pretend-camera --example pretend_camera -- 8554 20   # a local H.264 camera: 127.0.0.1, RTSP port 8554, path /stream
cargo run -p ipcamera-server                                 # http://127.0.0.1:8770/, target\debug\settings.json and data\ (--settings, --data, --port, --app, --allow-origin; --set-password or --clear-password, then it ends)
cargo ipcamera                                              # `nsh run` in crates/ipcamera/app from the root; arguments go to nsh run
cargo package-ipcamera                                      # `nsh package --target windows`: crates\ipcamera\app\.nsh\out\ipcamera\ and .zip
cargo package-ipcamera-web                                  # `nsh package --target web` into crates\ipcamera\app\.nsh\out\web\, with no IPCAMERA_SERVER: copy it to `web` beside ipcamera-server
cargo ipcamera-web                                          # the server and the web build on it at http://localhost:8787/, one Ctrl+C for both; the desktop app's crates\ipcamera\app\target\debug settings unless --settings; arguments go to the server
cargo mdviewer                                              # `nsh run` in crates/mdviewer/app from the root; arguments go to nsh run
cargo package-mdviewer                                      # `nsh package --target windows`: crates\mdviewer\app\.nsh\out\mdviewer\
cargo screenshots                                           # the READMEs' screenshots, in real windows (only when the owner asks); `ipcamera` or `mdviewer` for one; needs resvg
cargo test --workspace -- --ignored --nocapture             # against the real camera (IPCAMERA_HOST / _USER / _PASSWORD): the owner runs these
cargo test -p ipcamera-media diagnose_a_saved_camera -- --ignored --nocapture   # a saved camera's first failure, with its reason (IPCAMERA_SETTINGS_DIR)
cargo test -p ipcamera-core --test reolink_client -- --ignored --nocapture       # the owner's Reolink: what it is, has and sees, changing nothing (IPCAMERA_REOLINK_HOST / _USER / _PASSWORD, _PORT 443, _HTTPS 1)
cargo test -p ipcamera-core --test onvif_client -- --ignored --nocapture         # an owner's ONVIF camera: what it is, its events, ten seconds of what it sees (IPCAMERA_ONVIF_HOST / _USER / _PASSWORD, _PORT 80, _PATH)
cargo test -p ipcamera-core --test dahua_http -- --ignored --nocapture           # an owner's Dahua or Amcrest: which ports answer (no login), Detect, then its HTTP API's model, PTZ, light and lighting tables, changing nothing (IPCAMERA_DAHUA_HOST / _USER / _PASSWORD, _PORT 80, _HTTPS 0)

# crates/ipcamera/app
nsh run                                                     # a Win32 window, hot reload
$env:IPCAMERA_SOURCE = 'pattern'; nsh run                    # a moving 1080p test picture: nothing reaches a camera, PTZ included
$env:IPCAMERA_PICTURES = '<folder>'                          # with the pattern: a camera named X shows <folder>\X.png, still, and says it is live
$env:IPCAMERA_FOUND = '<file>.json'                         # with the pattern: the network search lists the cameras in it (ipcamera_core::FoundCamera)
nsh run --target web                                        # the preview over pretend cameras (one-time setup: root README "Setup"); ends a running `nsh run`
$env:IPCAMERA_SERVER = 'http://127.0.0.1:8770'; nsh run --target web   # the web build on the server (start it with --allow-origin http://localhost:8787)
nsh package --target web                                    # the web build as a static site in .nsh\out\; copy it to `web` beside ipcamera-server
nsh test                                                    # the crate's `cargo test`, then the *.test.ts(x) files headless
nsh test <file>.test.tsx --target windows                   # in a real window, which is the real app: set IPCAMERA_SOURCE first
nsh test <file>.test.tsx --target windows --fps -v          # the same, with an `nsh::perf` line a second (fps, frame / paint / present ms, cpu, memory)
nsh check                                                   # cargo check, then TypeScript's type check
cargo clippy --all-targets -- -D warnings
cargo fmt
nsh package                                                 # .nsh\out\ipcamera\ (deploy that, not target\release)

# crates/mdviewer/app (the same commands as IP Camera's: nsh run, nsh test, nsh check, clippy, fmt)
nsh test                                                    # its `cargo test`, then App.test.tsx (drops assets\screenshots\projects\Weathervane as a project), Document and Tree
# crates/dp/ui
cargo test                                                  # the theme files; either app's `nsh check` type checks the TSX
cargo clippy --all-targets -- -D warnings
cargo fmt
```

`.claude/launch.json`'s `ipcamera-web` runs the web build on port 8787 against the server, and `ipcamera-server` the
server on 8770, allowing the web build's origin; `crates/xtask/src/main.rs` (`cargo ipcamera-web`) starts the same two, so a
change to one changes the other. `cargo ipcamera-web` is the owner's: its server works on the desktop app's settings, the
password included, where `launch.json`'s keeps its own in `target\debug`, so checks use `launch.json`. Only one `nsh` development session runs at a
time: starting another ends the first.

The floor is Rust 1.95 for the root workspace (`rust-version`: the highest any dependency declares, from
glass2glass's `loom`) and 1.97 for the workspaces built with nsh (nsh 0.5.1's), with the MSVC toolchain, the Visual
Studio C++ build tools and the Windows SDK; raise it when a dependency's floor rises, and keep `ci.yml`'s `msrv` job in
step with the root's. MSVC `D8050 ... failed to get command line into debug records` or gcc's "Cannot create
temporary file" come from a Low integrity label on the repository folder, not from long paths: check
`icacls . | Select-String 'Mandatory Label'` first, and leave clearing it to the owner, since it is a security setting.
The web build needs the `wasm32-unknown-unknown` target and `tools\fetch-wasm-clang.ps1`, once per machine. The root
`.cargo/config.toml` optimizes `rust_h264`, `rust_h265` and `g2g-plugins` in the dev profile (unoptimized, a 1080p
H.265 camera falls behind), not a `Cargo.toml`: cargo reads profiles only from the workspace it builds, and `nsh run`
builds through one nsh generates (`crates/ipcamera/app/.nsh/rust`). It also links with `rust-lld.exe`, which every toolchain ships, instead of
MSVC's `link.exe`, which took far longer over the app's debug build, and keeps full debug info for the suite's own
crates only, every other one keeping line tables (a new crate of ours gets a line there): with all of it, each debug
build's PDB passed a gigabyte and the apps' `target` folders reached tens of gigabytes. `xtask` keeps the variables `cargo run` sets
(`CARGO_PKG_NAME`, `CARGO_MANIFEST_DIR`) from the cargo builds it starts: `ring`'s build script watches them, and
passed on they rebuilt `rustls`, nsh and the app whenever `cargo ipcamera` followed another build. Keep the `nsh-*` dependencies at the version of the installed `nsh` (`nsh --version`);
the installer takes the version as `$env:NSH_VERSION = 'v0.5.1'`.

## Rules

- **Three layers.** `core` is plain data and sockets: no UI, no media stack, no nsh. `media` is the pipeline: no
  renderer, no UI types; a picture leaves it as BGRA bytes in a slot. The app draws. A change that needs a lower layer to
  know about a higher one is the wrong shape.
- **One bridge, two backends.** `src/bridge.rs` is everything TSX can call; behind it
  `src/camera/` does the work on the desktop and `src/web_backend.rs` on the web, against `ipcamera-server`. A new bridge function
  gets both, and the web keeps the desktop's contract: the same checks (`form_check.rs`), the same errors, the same
  revision bumps. What can be shared is: the cameras, groups and views are `core`'s `Library` on both (the web's a copy
  of the server's), read through each backend's `with_library` and `src/library.rs`, and changed as `core` `Change`s
  that `Library::apply` makes, on the desktop's files or the server's. `App.tsx` never asks which one it got; where the
  two differ in kind, the bridge says so with data (`Cameras.liveUrl` is `""` where the app draws the picture itself).
- **A stream belongs to its camera, not to a tile.** Streams live in `src/camera/`, found by camera id and stream, so a
  hot reload or a layout change cannot drop one. A tile only picks which of its camera's streams it shows (`ViewTile`,
  `TileStream` in `core`): Auto, the one its size calls for (`Cameras.autoStream`), the camera's own, or another,
  which is a connection of its own; PTZ, the watch and the login stay the camera's, and recording is of its own
  stream, or for an event's video the stream its event video settings pick. What streams follows the screen: TSX says
  what it shows with `Cameras.setOnScreen(shown, kept, selected)`, each a camera and the stream it plays, Auto's
  already picked; a stream shown or kept whose camera has a password connects, one that is neither disconnects (unless
  it records, or is the stream a camera whose events record videos with a pre-roll keeps to hold it back:
  `Camera::set_armed`), a user's Disconnect holds until Connect, and only the selected tile's stream is heard. Kept is the last view's streams and every camera opened on its own since,
  whatever the page: connected, but not decoded (`crates/ipcamera/media`'s decode gate) while no tile shows them. Opening
  another view starts over with its streams.
- **`Library` is the only way to cameras, groups and views, and `config.rs` the only writer of `settings.json`.** Saves
  are atomic (temp file and rename), and a store never overwrites a file it could not read. Groups and views list
  cameras by id, and file names carry the id (`snapshot-<secs>-<id>.png`): a name is never an id. Everything is saved
  beside the executable (`settings.json`, `data\`; Markdown Viewer's `settings.json` and `projects.json`, in a `.nsh`
  folder beside its development host: see Pitfalls), never in `%APPDATA%` or the user's profile.
- **Camera safety.** Never print, log or repeat a camera's password, or leave a copy where it gets distributed
  (`.nsh\out`, a packaged build). `cameras.json` keeps each one encrypted for the account that saved it
  (`saved_password.rs`), so read a saved camera through `Library`, never from the file's text. Never try a blank or
  guessed login: failed logins lock the camera's account. A
  rejected login and a stream the app cannot play stop the session at once and are never retried; anything else
  reconnects by itself. Don't save or screenshot the real camera's picture, which shows the owner's property (sample its
  brightness instead), and don't record it: ask the owner to try it.
- **nsh's defaults first.** Build with nsh's components, tokens and behavior as they are. When a design detail would
  take a workaround, drop the detail rather than add the workaround, and say what was dropped. A workaround for something nsh 0.5.1 lacks names in a comment what replaces it, and is listed
  in `crates/README.md` "Workarounds for nsh 0.5.1" until a release makes it unnecessary.
- **nsh is pinned to 0.5.1.** Its website and gallery follow its `main` branch, which moves on past 0.5.1: check what
  you take from them against the installed crates (`~/.cargo/registry/src/*/nsh-*-0.5.1`).
- **A component asks `look()` how to draw and never checks which theme is shown**, so a new theme reaches every place
  at once. Colors are tokens: classes that name them (`bg-title-bar`, `ring-ring`), or `Themes.colors` for a style a
  class can't say. No color literals outside `crates/dp/ui/brand.tsx`, `elevation.ts` and `texture.ts` (black and
  white at low alphas) and `logos.ts`. A themed place that needs something new gets a field on `Look`. Red means
  recording in every theme; the high contrast themes stay flat.
- **Theme files are nsh theme files, colors only.** One file per theme in `crates/dp/ui/themes/`, shadcn's `cssVars`
  as `theme.json` beside it writes them, with a value for every token; shadows and lights are the elevation's. Token
  names are neutral (`title-bar`, `toolbar-hover`, `option`, `state-outline`). Adding a theme is its file, a line in
  `THEME_FILES` (`crates/dp/ui/src/lib.rs`) and an entry in `THEMES` (`crates/dp/ui/brand.tsx`), in its appearance's
  section, and every product has it. Nothing in the code, comments or docs names another product's themes (another
  company's: IP Camera and Markdown Viewer are this suite's).
- **Pages compose `crates/dp/ui`.** `nsh/base` brings behavior, focus and accessibility; `dp/ui` brings the look,
  the same in every product. Pages use its controls rather than styling base components themselves, and a new styled
  control goes there and is exported from its `index.ts`, whichever product needs it first. Each app imports it by
  path (`../../dp/ui` from `App.tsx`) and calls `connectThemes(Themes)` before it shows anything.
- **Motion that follows the pointer or the scroll follows it in the same frame; transitions are for discrete
  changes.** The collapsing header tracks the scroll 1:1; a tab's settings fading up as it opens (`dp/ui/appear.tsx`)
  animates. Anything that
  moves on its own holds still when the system asks for less motion (`reduceMotion()`) and while the app is in the
  background.
- **The main thread never waits on a camera, a disk, the server or a dialog.** Bridge calls that talk to a camera,
  write files, reach the server or open a system dialog are `async`, every change to the library included; quick reads
  stay synchronous (on the web they read the copy of the server's library).

## Code conventions

- Name context bindings and parameters `cx`.
- Formatting: rustfmt's defaults (there is no `.rustfmt.toml`): `cargo fmt --all` at the root, `cargo fmt` in
  `crates/ipcamera/app`, `crates/mdviewer/app` and `crates/dp/ui`. All are clean, so a diff from any is yours. TS and
  TSX follow the surrounding code: two-space indents, double quotes, semicolons.
- Lints: the root workspace sets `unsafe_code = "forbid"`, `missing_docs = "warn"`, `clippy::all = "warn"` and
  `clippy::mod_module_files = "warn"`, and clippy runs with `-D warnings`. A new crate in the root workspace sets
  `[lints] workspace = true`. The workspaces of their own (`crates/ipcamera/app`, `crates/mdviewer/app`,
  `crates/dp/ui`) repeat them in their `Cargo.toml`'s `[lints]`; IP Camera's app, `dp-ui` and, in the root workspace,
  `ipcamera-dpapi` put `clippy::undocumented_unsafe_blocks` in place of forbidding `unsafe`.
- Crates: a product's crate goes in its folder, named for it (`crates/ipcamera/core` is `ipcamera-core`); one that
  two products use goes in `crates/dp/` as `dp-<name>`, and only once the second one needs it. Never a bare
  `core`, `media` or `storage`: Cargo needs every name in a workspace to differ, and `core` is Rust's own.
- Modules: a module with children is `name.rs` beside a `name/` folder (`ptz.rs` and `ptz/dvrip.rs`), never
  `name/mod.rs`.
- Tests: a module's unit tests sit at its end, and move to `name/tests.rs` (`#[cfg(test)] mod tests;`) once they
  outgrow the code they test (`session/tests.rs`). Integration tests go in a crate's `tests/`, a few files grouped
  by subject, since each is a program to build and link; what they share is a module under `tests/support/` that each
  takes in with `#[path]` (the server's). A test waits for what it needs, never a fixed time, and none stands up
  a closed local port it could do without: the system takes two seconds to refuse one.
- `unsafe` lives only in `crates/ipcamera/app` and `crates/dp/ui` (workspaces of their own), for the Win32, COM and
  Direct2D calls in IP Camera's `src/video.rs` and `dp-ui`'s `src/folder_dialog.rs`, and in `crates/ipcamera/dpapi`,
  for DPAPI's two calls, which keeps it out of every other crate of the root workspace. Every
  `unsafe` block has a `// SAFETY:` comment, which clippy checks there.
- Errors: a library crate returns an error enum of its own that says which case it is (`StoreError`, `DvripError`,
  `InvalidCamera`, `MediaError`, `StorageError`), each reading as a sentence; `anyhow` is for the programs (`server`'s
  `main.rs`, `xtask`). The bridge turns an error into the `String` TSX shows. What may be missing is an `Option`, not
  `""` or `0`, except in the bridge's structs, which TSX reads.
- `core`'s types are used from its root (`ipcamera_core::CameraConfig`, `Library`, `Change`, `CameraId`), whichever
  file holds them: a new one is re-exported in `crates/ipcamera/core/src/lib.rs`. `api`, `config`, `ptz`, `rtsp_url` and `sync`
  stay namespaces of their own.
- Every Rust module starts with a `//!` doc saying what it holds and how it works; every public item has a `///` doc.
  In `src/bridge.rs` those docs become the TypeScript declarations' docs, every field included; the lint can't
  see them there (TSX reaches those types, not the crate root), so check by hand. Exported TS gets a `/** */` doc.
- Everything is written in English, American spelling (color, behavior, center, organize, canceled, gray): code,
  comments, docs, UI strings and errors. Comments explain why, not what the code says.
- Library code doesn't print (`println!`, `eprintln!`): a failure goes back as an error, and the UI shows it. Tests
  and examples may print.
- Dependencies are declared once in the root `[workspace.dependencies]`, each with a comment on why it is there and
  what ties its version (`retina` to glass2glass's); crates write `.workspace = true`.
  The apps and `crates/dp/ui` declare their own, being workspaces of their own. Platform-only dependencies go under
  `[target.'cfg(...)'.dependencies]`, with a comment saying why. The repository is MIT, and a release ships only the
  licenses `accepted` in `.github/notices/about.toml` lists (`crates/README.md` "Releases"): a dependency under
  another one stops it, so name its license to the owner before adding it there.
- The desktop's camera code is `#[cfg(windows)]`, and the web gets `src/web_backend.rs` in its place. `cargo test`
  builds `web_backend.rs` too, so its tests run there.
- TSX tags are PascalCase. Imports follow the layers: primitives from `nsh` (`View`, `Text`, `Scroll`, `signal`),
  base components from `nsh/base`, desktop-only APIs from `nsh/desktop` (`Window`, `Menu`, `titleBar`), the app's Rust
  from `ipcamera/native` (`mdviewer/native`), and the styled controls from `crates/dp/ui` by path.
- Tailwind classes are written out whole, never assembled from pieces (`bg-${name}`), so the bundler sees every class.
- What several regions read lives in `views/model.ts`; what a button or a menu does, in `views/actions.ts`.

## The bridge (`ipcamera/native`)

What TSX calls in Rust stays typed and documented:

- Its TypeScript declarations are generated from the Rust types into `.nsh/types/ipcamera/native.d.ts`, never written
  by hand. A module is a `#[module]` struct; data that crosses is a struct deriving `JsObject` or an enum deriving
  `PropEnum`, with every field documented, never a JSON string or a tuple. Rust's `set_on_screen` is TS's
  `setOnScreen`.
- A call that can fail returns `Result<_, String>`, and the string is what the user reads in a toast (`failed` in
  `views/actions.ts`), so write it as a sentence that says what to do: "Connect first, then take a snapshot."
- A listener is `on_<what>(listener, cx) -> Stop`: it calls the listener now and after every change, until the
  returned function is called or what registered it goes. It checks at a stated interval (`REFRESH_EVERY` and its
  neighbors in `src/bridge.rs`), not as fast as it can, through `dp_ui::listen` or `listen_later`.

## Performance rules

1. **Decoding is the heaviest work.** `rust_h264`, `rust_h265` and `g2g-plugins` are optimized even in debug builds;
   keep them so in the root `.cargo/config.toml`, which `nsh run` reads too.
2. **A frame's work happens once per camera, however many tiles show it.** Each camera on screen has one bitmap, reused
   for the whole stream (no GPU memory allocated per frame), and one thread that asks all its tiles to draw at once (the
   window draws once per frame). Per-tile work is the thing to avoid: a 4 × 4 grid of one camera once copied each frame
   to the GPU sixteen times.
3. **Keep the newest picture, never a queue.** The slot holds one decoded picture; a frame the window never draws is
   dropped, and counted (`StreamStats`) though the tiles no longer show the count.
4. **Poll cheaply.** Listeners check at fixed intervals, and the folder checks read only the listing, never the files,
   off the main thread (`listen_later` in `src/bridge.rs`).
5. **Before / after numbers for the video path, the stage or the layout are taken when the owner asks**: fps, frame /
   paint / present ms, CPU and memory from a scripted `nsh test --target windows --fps` run on the test picture. Say
   when a change might move them.

## Parallel agents

Subagents never run `cargo build`, `cargo test`, `cargo clippy`, `nsh run` or `nsh test` (several at once overload
the machine, only one `nsh` session can run, and `--target windows` opens real windows); at most an occasional
`cargo check -p <crate>` for compile errors. They write the code and its tests and report which tests to run; the
main session runs builds and tests itself, one at a time.

## Verification after changes

Keep it short. The owner looks at the app themselves, which takes them less time than a scripted run takes here.

1. Run the tests of what changed, not every suite: `cargo test -p <crate>` for each root crate touched; in
   `crates/ipcamera/app`, `cargo test` for a Rust change and `nsh test <file>.test.ts(x)` for the file that covers a TSX
   change. The whole `cargo test --workspace` or `nsh test` only for a change across the crates, or when asked.
2. Clippy with `-D warnings` and `cargo fmt --check` where the change is.
3. **Nothing that opens a window, a browser or a camera unless the owner asks**: no `nsh run`, no `--target windows`
   runs, no web build, no `ipcamera-server`, no pretend camera, no browser pane, no screenshots. Say in a line what is
   worth a look by hand, and stop there. Headless tests read a model of the views, not the screen, so name the change
   to look at when it is one they can't see.
4. The real camera (the ignored tests, `diagnose_a_saved_camera`, sound, PTZ): the owner runs these with their own
   credentials. Say what to run and what to look for.

When the owner asks for a real check:

- The window, drawing or layout: `nsh test <file>.test.tsx --target windows` with `IPCAMERA_SOURCE=pattern` drives the
  real window, and `await app.screenshot("<absolute path>.png")` takes screenshots there (headless runs draw nothing).
  Check dark and light.
- Recording: the test picture cannot record, so run the pretend camera and add a camera at `127.0.0.1`, RTSP port
  `8554`, path `/stream`.
- The web preview: `nsh run --target web`.
- The server, the player or the web build against it: run the pretend camera, put a camera at `127.0.0.1`, RTSP port
  `8554`, path `/stream` in the server's library, start `ipcamera-server` and the web build from `.claude/launch.json`,
  and use the app in the browser pane. Text fields there take focus from Tab, not from a click on another field. The
  pane throttles drawing while it is hidden, so measure the frame rate by decoding the WebSocket in the page. Stop
  what was started afterwards, and delete the `settings.json` and `data` the server made in `target\debug`.

## Pitfalls

Verified constraints, by area:

- **Ports.** nsh's development server takes 8790 (`nsh run`) and the web preview 8787 here, so `ipcamera-server` is on
  8770.
- **nsh 0.5.1.** A link inside text (`TextView`'s, or any nested `Text` with `onPress`) shows the pointing hand, over
  selectable text too, and its address as a hint while the pointer rests on it. Any other cursor comes from views alone
  (`cursor` in `nsh-platform-windows`' `input.rs`): nested text merges into its paragraph's view, so a `cursor-*`
  class on a span does nothing.
  `nsh run` builds the app again whenever a `.json` file anywhere in the app's folder changes, `target` included,
  and skips only folders named `.nsh` and `node_modules`: an app that saves JSON beside its development host (in
  `target\debug`) shows a build with every save. Markdown Viewer's host saves in `target\debug\.nsh\` for this
  (`dp_ui::development_folder`); IP Camera's doesn't yet.
  `Menu` (`nsh/desktop`) does nothing in a browser, so a button opens its menu through `MenuOpener` and a right click
  through `popup` (`views/actions.ts`), which draw it there (`crates/dp/ui/menu.tsx`); a dialog covering the window places a
  `MenuLayer` last. A drawn menu's items are `id:menu` to a test, which opens the app as the web with
  `platform: "web"`. A Rust
  `fetch` runs on the web too, through the browser's own, and a relative URL is this page's server. On the web a
  `WebView` is a sandboxed `<iframe>`, and one showing a page of the app's own origin loses its origin, after which
  the browser refuses its connections to this machine: the server hands out the tiles' player under this machine's
  other name (`localhost` / `127.0.0.1`) for that reason. `nsh.json` declares no `capabilities`, which keeps
  unrestricted HTTP, web views, remote images and video and opening URLs: declaring any capability denies everything
  it doesn't list, and the web build's `fetch` stops at once. `openUrl` takes only a whole URL, so the web build opens
  a saved file from the player's address, which is always whole.
  The engine has no `Intl.Collator`, so text that sorts numbers by value compares them by hand (`byText` in
  `views/roster.ts`). A
  `Menu` trigger must be a plain view: a nested `Button` swallows its press. Headless `nsh test` is a touch screen
  unless `open` gets `platform: "windows"`, and `open` forces a light system appearance. A test gets no file system:
  seed files from outside it, and never widen the app's `capabilities` for a test. `nsh run --target web` refuses to
  start once the web build calls JavaScript through wasm-bindgen (`uuid`'s `js` feature, `getrandom`'s browser
  backend, `web-sys`) unless the wasm-bindgen CLI of that exact version is installed; keep such crates out of the web
  build (`ids.rs` counts its ids there for this reason).
- **The window.** `keepWindowPlace` (`crates/dp/ui/window.ts`) only places the window and never shows or hides it: nsh
  shows it when its first frame is ready, and a race can leave it hidden for good. Stopping PTZ when the window closes
  is in `VideoView`'s `Drop`, because nsh's close event goes to JS, which a closing window never waits for.
- **Real-window runs.** `nsh run` and `nsh test --target windows` read and write `crates\ipcamera\app\target\debug\settings.json`
  and `crates\ipcamera\app\target\debug\data\settings` (the app's own workspace, not the root's `target`). Back both up before a scripted run and restore them after; the backup holds the
  password, so delete it. A shell whose working directory is inside `data\settings` locks the folder: restore by
  copying files in, not by deleting the folder. Swapping the test cameras into `data\settings` doesn't swap the
  snapshots and videos folders: the Snapshots, Videos and Dashboard pages there show the owner's own pictures, the real
  camera's included, so a real-window run never screenshots them; read their text instead, and delete only the files the
  run made.
- **Files.** A folder listing reports a file still being written with a stale size and time: read
  `std::fs::metadata` for it. That is also why the videos page rereads its folder every two seconds while a camera
  records.
- **Pictures.** `<Image>` decodes a PNG at its full size before it makes it smaller (8 MB for 1080p), and nsh keeps
  what it decoded after it scrolls away, up to a budget the app sets to 64 MiB (`PICTURE_MEMORY` in
  `crates/ipcamera/app/src/lib.rs`; nsh's own is 256 MiB). A page that lists pictures shows their thumbnails
  (`crates/ipcamera/storage/src/thumbnail.rs`), never the files: a gallery of full snapshots held hundreds of megabytes.
- **glass2glass.** It reports every RTSP failure as the same connect error, so `pipeline.rs` sends its own DESCRIBE to
  tell a rejected password apart. Its Media Foundation decoder, `MfDecode`, is no longer used: it cannot reach the
  HEVC decoder the Microsoft Store installs, and its H.264 decoder cost about 160 MB a camera with settings it can't
  change, so both codecs decode in Rust everywhere (`avc.rs`, `hevc.rs`). It also returned
  the coded size (1080 lines became 1088), which is why `SizeTap` records the size the stream declares and `SlotSink`
  crops to it. glass2glass's elements need not be `Send`, so each session drives its graph on a current-thread tokio
  runtime of its own, and so does each still a relay decodes (`still.rs`).
  A recording's file stays empty until the muxer writes its head with the first fragment, about a second in.
- **ONVIF.** The protocol is `oxvif`'s, pinned to one release (`=0.17.0`): its minor versions change its API, so
  raise it on purpose and run `cargo test -p ipcamera-core --test onvif_client` after. It is async, and `core` is
  not: an `OnvifClient` owns a current-thread tokio runtime and blocks on it, so make, use and drop one on a plain
  thread or in `spawn_blocking`, never on an async runtime's own thread, where tokio panics. `oxvif` has no error
  for a refused login; `told` in `onvif.rs` reads one out of its SOAP fault or HTTP 401, and that is what keeps
  a refused login from being sent again. Its `ContinuousMove` always carries pan, tilt and zoom, and it follows
  the addresses a camera gives, so `onvif.rs` passes it the camera's own address with only the path taken from
  those. It brings `reqwest` and a second TLS provider (`aws-lc-rs`, beside `ureq`'s `ring`), which the C compiler
  builds.
- **The server.** A relay decodes nothing: a snapshot or a thumbnail decodes the pictures since the last keyframe
  (`Feed::picture`), a keyframe interval of decoding, so it runs off the runtime's threads. A PTZ move on the web lasts a
  second unless sent again (`ptz.rs`; `web_backend.rs` sends a held one every 400 ms): don't make a move that lasts until a
  stop arrives, since a lost stop leaves the camera turning. Snapshots, recording and PTZ refuse a camera with no saved
  password rather than try a blank login; live video does too, until someone presses Connect for that camera. A
  server with a password (`access.rs`) answers only requests with a session's key, which what a browser fetches
  itself carries in its address (`keyed` in `web_backend.rs`, `player.js`): a new URL the web build makes for the
  server needs it too.

## Working with the owner

The owner works in C# and is new to Rust: explain Rust and nsh in those terms, and make the idiomatic
choice rather than asking them to pick between Rust-specific options.
