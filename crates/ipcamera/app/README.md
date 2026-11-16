<p>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="../../../assets/brand/ipcamera/logo/ip-camera-horizontal-inverse.svg">
    <img src="../../../assets/brand/ipcamera/logo/ip-camera-horizontal-color.svg" alt="IP Camera" width="320">
  </picture>
</p>

The IP Camera app: everything you see in it, and how it works.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../../../assets/screenshots/ipcamera-dark.png">
  <img src="../../../assets/screenshots/ipcamera-light.png" alt="IP Camera showing four cameras in a 2 × 2 view: a front door, a driveway, a backyard and a side gate in night vision">
</picture>

The app is built on [nsh](https://github.com/longbridge/nsh) with the suite's UI and themes (`crates/dp/ui`). The
interface is TSX, and this folder is also a Rust crate that holds the camera code and draws the live picture. It's a
Cargo workspace of its own, so `cargo` at the repository root doesn't build it.

| For | Read |
| --- | --- |
| IP Camera as a whole: its crates, what it needs, releasing it and what it saves | [IP Camera's README](../README.md) |
| The server and its API | [`server/README.md`](../server/README.md) |
| The controls, themes and workarounds the suite's products share | [`crates/README.md`](../../README.md) |
| Setting up a machine | the [root README](../../../README.md#setup) |

**Contents:** [Run it](#run-it) · [How it is organized](#how-it-is-organized) ·
[Cameras, groups and views](#cameras-groups-and-views) · [The Cameras page](#the-cameras-page) ·
[The Groups page](#the-groups-page) · [Dashboard](#dashboard) · [Snapshots](#snapshots) · [Events](#events) ·
[Recording and videos](#recording-and-videos) · [Live video](#live-video) ·
[Settings](#settings) · [Themes](#themes) · [Tests](#tests) · [The web](#the-web) · [The window](#the-window) ·
[Workarounds for nsh 0.5.1](#workarounds-for-nsh-051)

## Run it

```powershell
nsh run
```

On first launch there are no cameras. Add one with the **+** in the sidebar and enter its password; from then on the
app connects by itself. The form flags a number that makes no sense, and Save waits until it's fixed. PTZ uses saved
settings from its next move. Cameras are saved in `data\settings\cameras.json` beside the executable, each password
encrypted for your user account (IP Camera's [README](../README.md#what-it-saves) says who that protects it from).
Under `nsh run` that folder is `target\debug\data\settings`, which git ignores.

To work without a camera, set `IPCAMERA_SOURCE` and the app shows a moving 1080p test picture instead. Nothing reaches
a camera, PTZ included:

```powershell
$env:IPCAMERA_SOURCE = 'pattern'
nsh run
```

Two more variables work with the test picture:

- `IPCAMERA_PICTURES`, a folder: a camera named like a PNG there (`Front door.png`) shows that still picture instead
  of the test picture, and its status says it's live, as a real camera's would.
- `IPCAMERA_FOUND`, a JSON file: the camera form's network search lists the cameras in it
  (`ipcamera_core::FoundCamera`s, as the server's `/api/discover` returns them) instead of saying it reaches no camera.

The READMEs' screenshots are taken this way (`cargo screenshots`, [`crates/README.md`](../../README.md#screenshots)).

The test picture is drawn in the app rather than sent by a camera, so it can't be recorded. To try recording without
a camera, run the pretend camera from `crates/ipcamera/media` instead: a test pattern encoded as H.264 and served over
RTSP on this machine. Add a camera with the address `127.0.0.1`, RTSP port `8554` and RTSP path `/stream`, then connect
it:

```powershell
cargo run -p ipcamera-media --features pretend-camera --example pretend_camera -- 8554 20
```

Only one `nsh` development session runs at a time: starting `nsh run --target web` ends a running `nsh run`.

## How it is organized

### The shell

| Path | Holds |
| --- | --- |
| `App.tsx` | The shell: the window's state, its panels and its shortcuts |
| `views/` | Each region: sidebar, header, video stage, PTZ panel, status bar, the editors' dialogs, and the app's own pages |
| `views/model.ts` | What every region reads: the cameras, groups and views from Rust, and what the stage shows |
| `views/actions.ts` | What the buttons and menus do, including every menu's items |
| `views/sidebar.tsx` | The sidebar (`Sidebar`): the pages, the views, the cameras by group, and their order |
| `views/header.tsx` | The header, which is the title bar (`Header`): what's shown, the layout menu, snapshots and recording |
| `views/statusbar.tsx` | The status bar along the window's bottom (`StatusBar`): how many cameras are live, how many record, the day's events, and the space the snapshots and videos take |
| `crates/dp/ui/` | The styled controls, the themes and the system calls every product of the suite shares (`dp-ui`), imported by path (`../../../dp/ui`). `nsh/base` brings behavior and accessibility; these bring the look, from the current theme. `App.tsx` hands them the app's `Themes` module first (`connectThemes`). Its files are listed in [`crates/README.md`](../../README.md#dp-ui) |
| `icon.png` | The app icon (`nsh.json` `icon`), `assets/brand/ipcamera/ip-camera-app-icon.svg` rendered at 1024 × 1024 |

### The stage

| Path | Holds |
| --- | --- |
| `views/stage.tsx` | The video stage (`Stage`): a view's tiles or a camera on its own, and over each picture its badges and toolbar (`Badges`, `PictureToolbar`, the lines between tiles in `edgesOf`) |
| `views/picture.tsx` | A camera's live picture and its zoom, shown by the stage's tiles and the layout editor's frames |
| `views/layout.tsx` | A view's layout menu, with Edit layout |
| `views/arrange.ts`, `views/arranger.tsx` | Editing a view's layout on the 12 × 12 grid: the draft and its rules, and the editor the stage shows |
| `views/ptz.tsx` | The PTZ panel (`PtzPanel`) |

### The Cameras and Groups pages

| Path | Holds |
| --- | --- |
| `views/cameras.tsx` | The Cameras page: every camera in a table, its search and filters, and the bar for a selection |
| `views/roster.ts` | What the Cameras page shows: the rows the search and filters keep, and what several cameras share |
| `views/settings.tsx` | A camera's settings dialog: its parts down the side, and the chosen part's fields |
| `views/streams.ts` | The stream choice in that dialog: the streams the camera listed when Detect asked it |
| `views/finder.tsx` | The window the dialog's network search opens: the cameras that answered, to take an address from |
| `views/bulk.tsx` | The editor for several cameras at once |
| `views/groups.tsx` | The Groups page: every group in a table, its search and filters, and the bar for a selection |
| `views/grouplist.ts` | What the Groups page shows: each group with what its cameras are doing, and the rows the search and filters keep |
| `views/members.tsx` | A group's dialog: its name, its cameras in order, and the other cameras beside them |
| `views/listing.ts` | What both pages' tables do with their rows, whatever they hold: the order a column sorts them in, the selection and the pages |
| `views/paged.tsx` | The parts of a table that read its listing, the same on both pages: the titles' checkbox for the page, and the footer with the pager |

### The dashboard, snapshots, videos and events

| Path | Holds |
| --- | --- |
| `views/dashboard.tsx` | The dashboard: the numbers, the charts and the cameras |
| `views/overview.ts` | What the dashboard shows: the totals, what was saved day by day, the space by camera, and the drives |
| `views/gallery.tsx` | The snapshots gallery |
| `views/grid.tsx` | The grid the snapshots, videos and events pages share: tiles in as many columns as fit, or a page's own table at the List size, under headings that fold their sections |
| `views/snapshots.ts` | What the gallery shows, and its actions |
| `views/viewer.tsx` | One snapshot at full size, stepping through the gallery |
| `views/videos.tsx` | The videos page and its player |
| `views/recordings.ts` | What the videos page shows: the recordings, their totals, and their actions |
| `views/events.tsx` | The events page: what the cameras saw, as a table with each event's picture small in its row, or as pictures |
| `views/eventlog.ts` | The event log, followed while the app runs: what the events page shows, its actions, and the alert each new event brings |
| `views/linked.ts` | The links between an event and the snapshot and videos it saved: from an event to its files on their own pages (`eventVideos`, `openEvent`), and from a file back to its event |
| `views/library.ts` | What the snapshots and videos pages share: the camera and date filters, the order and the grouping, each page with its own |
| `views/filters.tsx` | Those filters' menus in the header, for either page |
| `views/masthead.tsx` | Either page's collapsing header: the count and stats, and the filters and buttons in its bar |

### Settings

| Path | Holds |
| --- | --- |
| `views/preferences.tsx` | Settings, in its dialog over the page (`SettingsDialog` in `crates/dp/ui/settings.tsx`, which every product shares): where media is saved, signing out of a server with a password, and the appearance (`appearanceSection`, shared by every product) |
| `views/automation.tsx` | Settings' Automation section: alerts, recording on detection, how event videos are made, and the storage limits |
| `views/eventvideo.ts` | How event videos are made, as Settings → Automation and a camera's Events part offer it: the choices, their labels, the kinds and the streams |
| `views/signin.tsx` | The page a server with a password shows until someone signs in (the web build only) |

### The Rust

| Path | Holds |
| --- | --- |
| `src/lib.rs` | The crate root: what `App.tsx` imports from `ipcamera/native`, and the memory nsh keeps decoded pictures in (`PICTURE_MEMORY`) |
| `src/bridge.rs` | Everything TSX can call |
| `src/camera.rs` | What the bridge calls on the desktop, each a few lines over the hub: the cameras by id, their streams, PTZ, snapshots, recordings, settings and preferences |
| `src/camera/hub.rs` | `Hub`, every camera the app holds in the library's order, which of them stream for what's on screen, and the one hub the app runs on |
| `src/camera/device.rs` | `Camera`, one camera: its settings, its feed while connected, PTZ, sound, recording and status |
| `src/camera/feed.rs` | `Feed`, where a connected camera's frames come from: a media session or the test picture |
| `src/camera/status.rs`, `stats.rs`, `pattern.rs` | A stream's status as a sentence, its stats line, and the moving test picture |
| `src/camera/tests.rs` | The tests of all of the above |
| `src/camera/explorer.rs` | Showing a snapshot or a recording in Explorer, or opening its folder. How the files are named, written and listed is `crates/ipcamera/storage`'s, which the server shares |
| `src/camera/storage.rs` | Where snapshots and videos are saved, choosing another folder, and how full the drives under them are |
| `src/video.rs` | `ipcamera.video`, the component that draws a camera's newest frame with Direct2D |
| `src/web_backend.rs` | The web's backend: a copy of `ipcamera-server`'s library, changes sent to the server, and its player in each tile; a preview in memory when no server answers |
| `src/library.rs` | The groups and views, and changes to several cameras at once, read from either backend's `Library` and described as `ipcamera_core` `Change`s: the same code on the desktop and on the web |
| `src/form.rs` | The camera settings form, to and from a camera's settings, the same on the desktop and on the web |
| `src/form_check.rs` | What stops the camera settings form, or a change to several cameras, from being saved, the same on the desktop and on the web |

A stream belongs to its camera in `src/camera/`, not to the tile showing it, so a hot reload or a layout change can't
drop it. A tile only picks which of the camera's streams it shows: Auto, which plays the one the tile's size calls
for; the camera's own; or another of its streams, which the camera runs as a separate connection beside it.

## Cameras, groups and views

### The sidebar

Under the dashboard, the videos and the snapshots, the sidebar lists the views, then the cameras: all of them, and by
group. Only the cameras scroll, under their heading, so the pages, the views and the heading's buttons stay in reach
however many cameras there are. While cameras are scrolled above the heading, a line shows under it and the list fades
into the sidebar there; the same happens, facing up, above the rows pinned at the bottom while more cameras lie below
(`EdgedScroll` with `fade`). The fade runs 24 points into the sidebar's own color, solid along the edge and under its
line so no row shows through, with the elevation's shade over it, both drawn from the same stops either way. Neither
shows while every camera fits. The Cameras page, the Groups page and Settings stay at the bottom.

Drag the sidebar's edge to resize it, from 180 to 400 points, anywhere along its height from the title bar to the
status bar. Nothing marks the edge until the pointer is over it; then a short grip in the muted text's color shows in
its middle, stronger while dragging. There's no line along the edge, since it would run up into the title bar but
stop at the status bar. The width is saved when the drag ends (`sidebar_width` in `settings.json`, 232 at first) and
kept as the window resizes, the page taking the rest; the sidebar and the page are a `ResizablePanelGroup`, like the
video and the PTZ panel. Hiding the sidebar (its button, Ctrl+B, or a window under 640 points) keeps the width for when
it comes back. The web build saves no preferences, so it starts at 232 every time.

### The stage's islands

The video and the PTZ panel are islands on the frame, which is the sidebar's color (`Island` in `crates/dp/ui`;
[`crates/README.md`](../../README.md#islands) explains how their colors are chosen): rounded blocks a gap apart and a
gap from the frame's edges, the video in the theme's page color and the PTZ panel one step toward the frame. The
video's island has no outline (`edgeless` on `Island`), so the video fills it to the edge, and the island clips it to
its rounded corners, live picture included (nsh clips a native component to the rounded corners of the views around
it). A camera on its own, a view's tiles and the layout editor are square, so only the stage's outer corners are
rounded. Drag the gap between the video and the PTZ panel to resize the panel; a grip shows there under the pointer,
as along the sidebar's edge. In full screen the video fills the window with no gap, outline or corners. The other
pages (the dashboard, the galleries, the tables) aren't islands: each fills its space in the page color. Settings
opens in a dialog over whichever is showing.

### Menus

A group's menu, in the sidebar and on the Groups page, opens its dialog, renames it, adds or removes cameras, shows its
cameras on the Cameras page, duplicates it, moves it up or down among the groups, and deletes it. Camera and view rows
have a `…` at their right end too, shown while the pointer is over the row and always on the selected row, since a
touch screen has no hover. It sits over the row rather than in it, so pressing it opens the row's menu without
changing the page; a right press opens the same menu. A group's folder always shows its `…`.

### The cameras' order

All cameras lists the cameras in their own order, the one `cameras.json` keeps and every camera list starts from, and
that's where you change it. A grip shows at the left of a camera's row under the pointer, and always on the selected
row. Drag a row by its grip and it lifts off the list (`look().dragged`: the popup's color and the elevation's popup
shadow, outlined with a ring so it keeps its size) and follows the pointer, while each row it passes slides aside as
the dragged row's middle crosses its own. Let go, and it settles into the gap and the move is saved (`Cameras.moveTo`,
an `ipcamera_core` `Change::MoveCamera`); a move that fails settles back and says why. The row's menu moves a camera
up, down, to the top or to the bottom, and Alt+Up and Alt+Down move the camera open on its own. A group's cameras keep
the group's order, which its dialog sets.

The drag is `crates/dp/ui/reorder.tsx`, built from nsh's pans and animated values, since nsh 0.5.1 has no sortable
list. The grip's drag moves its row's own animated value on the UI thread, and the other rows follow that value
through `interpolate`, so no JS runs while the pointer moves. What a pan in nsh 0.5.1 can't do is left out rather than
worked around:

- The list doesn't scroll by itself while a row is dragged to its edge, since a pan tells JS nothing until it ends; to
  move a camera farther than the rows in view, use its menu.
- A row is picked up by its grip, never by a long press, and dragging anywhere else on a row still scrolls the list on
  a touch screen.
- Alt+Up and Alt+Down follow the open camera rather than the row with keyboard focus, and a screen reader gets the
  moves through the row's menu, since `nsh/base`'s `Button` passes on neither its focus nor accessibility actions.

### The files

Each kind is saved in a JSON file of its own in `data\settings`, through `crates/ipcamera/core`. Groups and views refer
to cameras by id, so a camera is entered once however many groups and tiles show it:

| File | Holds |
| --- | --- |
| `cameras.json` | Every camera's settings |
| `groups.json` | Named groups, each a list of camera ids. A camera can be in several |
| `views.json` | Saved views: where each tile sits on the 12 × 12 grid (`cells`), the fit for tiles that pick none, and in each tile a camera id, with the stream when the tile picks one rather than Auto and the fit when it picks its own |

### A view's layout

A view's tiles sit on a grid of 12 columns and 12 rows over the stage (`ipcamera_core::GRID`). Each covers a rectangle
of cells, at least 2 × 2, anywhere on the grid, and no two overlap (`check_arrangement`). On a 16:9 stage, a tile as
many cells wide as tall is 16:9 too, the shape most cameras send. The layout menu in the header offers the usual
security viewer splits, which a view starts as and can be reset to: Single, 2 side by side, 2 stacked, 1 large + 2
beside (two thirds of the width, with two small tiles stacked on its right), 1 large + 2 below (two thirds of the
height, with two small tiles under it), 2 × 2, 1 large + 5, 1 large + 7, 3 × 3 and 4 × 4. Each fills the grid exactly,
since 12 divides evenly into 1, 2, 3 and 4 shares (`Layout::on_grid`), and the menu checks the one a view's tiles match,
in order. A layout with fewer tiles keeps the cameras of the tiles it hides, for when the view has room for them again.

### Editing a layout, and a tile's fit

*Edit layout…*, at the end of the layout menu and of a tile's menu, moves and resizes the tiles one by one
(`views/arrange.ts` keeps the draft, `views/arranger.tsx` draws it). The stage shows the grid's lines over black, and
each tile as a frame over its camera's live picture, with the tile's fit (`fitOf`) and a chip with the camera's name
and the tile's size.

- **Moving and resizing.** Drag a frame to move it, or drag any of its four corners or four edges to resize it while
  the opposite edges stay put. Each handle is a white mark with a dark edge, so it shows on a bright picture too, and
  takes the theme's primary color under the pointer; an edge's handle moves only across its edge. The frame follows the
  pointer in the same frame, picture and all, its transform driven by nsh's pan on the UI thread (nsh draws the video
  component and places a web view inside their view's transform). When let go it lands on the nearest cells, stopping
  at the grid's edges, or goes back where it was if it would cover another tile. A resize shows the outline it
  stretches to, which stops at the grid's edges and at the smallest size, and the picture takes the new size when let
  go.
- **The pictures.** The pictures on screen are the draft tiles' (`draftStreams`), so a camera a layout brings back from
  past the cells (`ViewInfo.hiddenTiles`) plays at once, and a removed tile's camera stays connected, undecoded, in
  case of Cancel.
- **Adding and removing.** A frame's ×, on a dark chip inside its top right corner, removes the tile with its camera
  (not the last one). *Add tile* puts an empty tile in the first free space from the top left: 4 × 4 where it fits,
  else 3 × 3, else 2 × 2. Picking a layout from the menu while editing lays the draft out that way.
- **Saving.** *Done* (Return) saves the tiles in one change (`Views.arrange`, core's `Change::ArrangeTiles`), each
  keeping its tile's camera; *Cancel* (Escape), or leaving the view, discards them. Tiles moved off a preset layout are
  the view's own arrangement, and the layout menu checks none.

A tile's fit says how its picture fills it: Original (the whole picture, with bars), Fill (cropped to fill), Stretch,
or a forced 1:1, 4:3, 3:4, 16:9 or 9:16. Each tile picks its own, since a view's tiles come in every shape, from Fit on
its toolbar or in its menu (`Views.setTileFit`, `Change::SetTileFit`), and keeps it when its camera or stream changes
and when the layout editor moves it; an emptied tile has none. A tile with no fit of its own shows the view's
(`ViewInfo.fit`). A camera on its own fills the stage as its toolbar's Fit says, Original until one is picked; like the
stream picked there, that choice lasts until the app closes and isn't saved (`pageFitOf` in `views/model.ts`).

A tile with a camera is black, so the bars around its picture are black, as for a camera on its own. An empty tile sits
on the page color (the sidebar's in Default), like the stage with no cameras at all, since it has no picture to frame;
so does any part of the grid a layout leaves bare. The tiles meet edge to edge, square, like a video wall's screens.
One hairline (one device pixel, at any display scaling) in the theme's line color separates two tiles; the tile to the
right or below draws it on its left or top, so two neighbors never draw two lines side by side (`edgesOf` in
`views/stage.tsx`). Along the stage's edge, the island's own edge is the tile's. The selected tile has no mark on the
stage: the header names its camera, and the PTZ panel and the sound follow it. Full screen's tiles are the same,
reaching the window's edge.

The first launch makes an empty 2 × 2 view called Default. An empty tile's Add camera lists the cameras to show in it,
then New camera…; with no cameras yet, it opens the new camera's form at once and puts the camera in that tile.

### Auto

A tile shows its camera on Auto until it picks a stream, and so does a camera shown on its own. Auto plays the
smallest of the camera's streams that the picture is drawn no more than a quarter larger than, else the largest
(`Cameras.autoStream`, `auto_stream` in `src/library.rs`). A 4 × 4 view on a 1080p screen plays each camera's sub
stream, a fraction of the main stream's decoding, and the same camera filling the stage plays its main stream.

Auto goes by the picture's size in device pixels, its zoom and its fit (`Fit::enlargement` in `crates/ipcamera/core`),
which each tile and the page report from their `onLayout` (`drawnAt` in `views/model.ts`). It picks only among the
sized streams on the lens of the camera's own stream, so a camera with a tracking lens stays on the lens it shows. A
picture's first size counts at once, and a later change only once it has held for half a second, so dragging the
window's edge or turning the wheel doesn't switch streams at every step: enlarge the window and its tiles move up to
the larger stream half a second after the drag stops; shrink it and they move back down. A camera's tiles on Auto in
one view share one stream, the one the largest needs, so the camera keeps one connection. Until a picture has a size,
Auto picks nothing and nothing connects for it, rather than connecting to a stream only to leave it.

Auto picks only between streams whose size it knows, which Detect finds (the Cameras page, below). A camera with one
stream, or none sized yet, plays its own stream on Auto at once (`Cameras.autoPicks`); a camera whose streams aren't
sized yet needs Connect in its dialog once.

### Picking a stream

A tile picks a stream for good from Stream in its menu or its toolbar's stream picker, and so does a camera on its own.
The choices are:

- the camera's own, which follows its settings;
- for a camera of a known brand, the main or the sub stream at the brand's paths (a Dahua's `subtype`, a Reolink's
  `Preview_01_sub`);
- the streams the camera listed when Detect last asked it, with what it said about each picture
  (`Cameras.streamChoices`, `stream_choices` in `src/library.rs`), each listed once ([the stream
  menu](#the-stream-menu)).

The chip over the picture names the picked stream beside the camera's name; on Auto, its numbers give the size. A
camera on its own keeps its pick in the page (`{ kind: "camera", id, stream }`), which is saved as the page to reopen
on, so a camera opened from the sidebar or the Cameras page starts on Auto. Only the stream belongs to the tile: the
address, the login, PTZ and everything else stay the camera's (`CameraConfig::with_stream` in `crates/ipcamera/core`).
`views.json` keeps the stream in the tile: the camera's id alone on Auto, and `{ "camera": "<id>", "stream": "sub" }`
for a picked one (`own`, `main`, `sub`, or the path the camera listed).

### What streams follows what is on screen

TSX tells Rust which streams the stage shows, which it keeps, and which one is selected (`Cameras.setOnScreen`), each
as a camera and the stream its tile plays, with Auto's pick for a tile on Auto (`streamsOn` and `playedIn` in
`views/model.ts`):

- A stream shown or kept connects by itself when its camera has a password, unless the user pressed Disconnect. A
  stream that's neither disconnects. Only the selected tile's stream is heard.
- Two tiles showing one camera on two streams are two connections to it; two on one stream share one.
- Connect starts the camera's streams on screen, or its own stream if none is; Disconnect stops all of them.
- The header, the PTZ panel and the stats follow the selected tile, including the status and stats of the stream it
  shows.
- A recording is of the camera's own stream, which connects for it if the tiles show only another, and stays connected
  until the recording stops. No tile shows it, so it decodes only the picture each file starts on, for the file's
  thumbnail (`crates/ipcamera/media`'s decode gate).
- A snapshot takes the camera's own stream's picture if a tile shows it, else the selected tile's.

A double click fills the stage with a tile, and another puts it back. A camera shown on its own has everything a tile
has over its picture: the same badges (its name with its status, the picked stream, its spotlight while on, whether it
records, what it sees; `Badges` in `views/stage.tsx`), the same toolbar, and the same menu.

### The picture's chip

Under the name, on the same dark chip at the top left and always shown, are the camera's address
(`CameraState.address`, its host) and the numbers of the stream the picture shows: its size, frame rate and transport,
a point smaller than the name (`CameraState.streams`, one per running stream, so two tiles of one camera on different
streams each show their own). A narrow picture, by the toolbar's measure, keeps only the size and frame rate. What the
camera's watch reports (`RLC-811A: Motion, Person, Spotlight.`) stays off the picture: it runs long and tells the
viewer little. On the web, the server's player draws the picture and keeps its numbers, so a tile there shows only the
address.

### The toolbar and the picture's menu

The toolbar (`PictureToolbar` in `views/stage.tsx`, on `OverlayToolbar` in `crates/dp/ui/media.tsx`) fades up at the
bottom center of a picture while the pointer is over it, and of the selected tile where there's no pointer. It holds,
in order:

1. The stream it's on, by size (`1920×1080`), else by name, and on Auto what Auto plays (`Auto · 896×512`), which opens
   the stream menu, for a camera with more than one stream.
2. Its spotlight, where it has one.
3. A snapshot of that camera.
4. Record, which records that camera, and while it records stops every recording, as the header's one switch does.
5. Fit, a menu of the fits with the picture's checked and the forced shapes set apart (`fitMenu` in
   `views/actions.ts`); its tooltip names the current one (`Camera 1's fit: Fill`).
6. Its sound.
7. In a tile, Open on its own.
8. Set apart from those: Connect or Disconnect (a plug, plugged in or pulled out), which acts on the camera and so on
   every tile that shows it, and which the header leaves to the toolbar; and More, the picture's menu.

A picture under 340 points wide keeps only the stream and More. The buttons have round ends like the toolbar's pill,
two points inside it (`OnPill` in `crates/dp/ui/button.tsx`), so the first and last follow its curve rather than
sticking out past it. The layout editor's bar is the same toolbar.

The menu, from a right press or More (`pictureMenu` in `views/actions.ts`: Stream, Fit, Connect or Disconnect, then
Edit camera…, which a tile's menu lists among the tile's own items), holds what the toolbar does that a keyboard or a
narrow tile can't reach, including the streams Detect found. Both are given the stream the picture picks and plays and
what picking another does: the tile's `Views.setTile`, or the page's `show` (`tileStream`, `pageStream`); and its fit,
the same way (`tileFit`, `pageFit`). A new button, badge or menu item for a camera's picture goes in those, so both
places get it.

### The stream menu

The stream menu (`streamMenu` in `views/actions.ts`), from the toolbar or the picture's menu, lists Auto first, naming
what it plays while the picture is on it, then each of the camera's streams once, the same rows the camera's dialog
lists (`Cameras.streamChoices`; `stream_rows` in `src/library.rs`):

- its own stream, its default, marked *(default)*, which, when picked, stays on whichever stream the camera's settings
  name (a tile saves it as `own`);
- the main and sub stream at its brand's paths, for a Dahua or a Reolink;
- the streams Detect found.

Two ways to the same stream are one row: the brand's main and sub are the streams the camera listed at those paths,
under the camera's names for them, and a tile saved on either finds its row (`Cameras.choiceKey`). Each row gives the
picture's size, the codec and the stream's name where the camera reported them
(`1920×1080 · H.264 · Profile003_AutotrackStream`), largest first. A camera with a second lens gets a heading for each
lens, as its streams' names tell them apart (`CameraStream::lens` in `ipcamera_core`).

A tile switched to another stream, by its menu or by Auto, keeps drawing the old stream's last picture until the new
one's first picture arrives, for ten seconds at most (`src/video.rs`), since a stream shows nothing until its first
keyframe, which takes seconds on some cameras. Meanwhile the tile's numbers say what's coming
(`Waiting for 896×512…`).

### Kept cameras

The kept cameras are the last view's cameras and every camera opened on its own since, from the sidebar or anywhere
else, each on the stream it was last shown on (`kept` and `keptStreams` in `views/model.ts`), whatever page is open:
one of the view's cameras on its own, another camera, the dashboard, Cameras, Groups, Snapshots, Videos or Events, or
Settings over any of them. So a camera that's already connected is never reconnected on the way to it, and opening one
camera never disconnects the others.

A kept camera that no tile shows stays connected, its status green, but nothing of it is decoded or drawn:
`crates/ipcamera/media`'s decode gate (`gate.rs`) gives the decoder nothing while no tile shows the camera, and keeps
the pictures since the last keyframe. When the camera is shown again, the gate sends those, the decoder catches up in
one burst, and the tile shows that keyframe at once and the live picture a moment later. Opening another view starts
over with its cameras, and the rest disconnect. The network still carries each kept camera's whole stream.

## The Cameras page

**Cameras**, at the bottom of the sidebar above Settings, replaces the video with a table of every camera, to find
them and to change many at once. The dashboard's Cameras number opens it too. Its header is like the other pages'
(with its badge in the cameras' chart color): it counts the cameras, says what the search and filters keep, and shows
three stats beside the title: the groups, the cameras in no view, and those with no password.

- **The bar** has a search box, which keeps the cameras with every typed word in their name, address (port included),
  username, RTSP path or groups, and three filters: a group (or *In no group*), a status (*Needs a password*, *Has a
  problem*, *Connected*, *Recording*) and a stream (main or sub). *Clear filters* shows while any of them hides
  something. *Add camera* opens the dialog for a new camera, which stays on this page, selected, once added.
- **The table** has a row per camera, in the order they were added: its status dot and name, its address, its
  channel, stream and transport (`Ch 1 · Main · TCP`), its username or *No password*, and its groups. A narrow window
  drops the groups, then the login, then the stream. Each column title sorts by its column, then the other way, then
  back to the order they were added; numbers in names and addresses sort by value (Camera 2 before Camera 10, 10.0.0.9
  before 10.0.0.20). A row's pencil opens that camera's dialog, as a double click does, and its `…` opens its menu,
  with *Show live video* first.
- **The rows come a page at a time** (25, or 10, 50 or 100 by *Rows per page*), scrolling between the column titles,
  which stick under the header's bar, and a footer that stays put under them. The footer counts what the page holds of
  the cameras the search and filters keep ("11–20 of 42 cameras"), the total while those hide some, and how many are
  selected, and ends in the pager: rows per page, *Page 2 of 5*, and the first, previous, next and last page. A new
  search, filter, order or page size goes back to the first page; deleting cameras never leaves you past the last.
  Turning a page brings its first row into view under the titles.
- **Selecting** works as in Explorer: a press selects one row, Control adds or removes one, Shift adds the range from
  the last row pressed, and each row's checkbox adds or removes it. The titles' checkbox selects every row on the page,
  or clears them. Control+A selects every row the search and filters keep, on every page; Delete deletes the selection
  (after asking); Return edits it; and Control+D duplicates it, but none of these while a text field has the keyboard
  or a dialog is open. Rows selected on other pages stay selected, and the footer and the selection's bar count them.
  A camera the search or a filter hides leaves the selection, so an action never reaches a camera the filters leave
  out. While some are selected, a bar floats over the bottom of the rows, above the footer, starting under the
  checkboxes and leaving the bottom right to the toasts: *Edit*, *Duplicate*, *Groups* (each group checked when they're
  all in it, with a count when some are; choosing one puts them all in or takes them all out; *New group…* makes a
  group of them), and *Delete*. A right press on a selected row offers the same.
- **Duplicate** (also in a camera's menu, in the sidebar too) adds a copy of each camera with the same address, login,
  stream, ports and groups, on the next channel its address doesn't use yet, named after it ("Garage copy", then
  "Garage copy 2"). That's how to set up a recorder's cameras: enter channel 1, then duplicate it once per channel and
  rename each copy. Copies made on this page are selected, ready to rename or change together.

### A camera's dialog

A camera's settings (a row's pencil or a double click, *Edit camera…* in a camera's menu anywhere, or *Add camera*)
open in a dialog over the dimmed window (`EditorDialog`, `crates/dp/ui/editor.tsx`). Its parts, *Connection*,
*Stream*, *PTZ*, *Features* and, on the desktop, *Events*, run down its left side in the sidebar's style (`NavTabs`,
tabs to a screen reader, with Up and Down moving between them), and the chosen part's fields show beside them. It
always opens on *Connection*. *Events* gives the camera event video settings of its own in place of Settings →
Automation's as a whole, and shows the app's until it has its own ([Events](#events)). The dialog keeps one size
whatever part shows, so its buttons never move.

**Connect**, in the footer before *Cancel*, asks the camera what it is with the address and login in *Connection*
(this is Detect, below).

- A new camera's other parts stay closed (dimmed, ignoring presses) until it has been asked, so they open filled in
  with what the camera said. If it doesn't answer, they open anyway, since a port there may be what it takes to reach
  it.
- *Add* stays off until the camera has been asked at the address and login the form holds; changing either turns it
  off again. Until then, Return in a field asks the camera. A camera that answered is added with *Add*. One that
  didn't (it has video but nothing Connect can ask, RTSP alone, or it's off for now) is added with *Add anyway*, which
  the same button becomes after a failed Connect, and which only a press takes, never Return (`addChoice` in
  `views/settings.tsx`).
- A saved camera was reached when it was added, so its parts are open and *Save* is on whenever something changes;
  Connect asks it again.

A field with a problem says so under it, a dot marks its part (a screen reader hears "has a problem"), and *Save*
stays off until it's fixed; Return in a field saves, or shows the first part with something to fix. *Cancel*, Escape,
the close button and a press outside the dialog discard the edits.

### Brands and Detect

**The brand** (in *Connection*: Dahua / Amcrest, Reolink or Other) decides the RTSP path a blank override stands for
(the *Stream* part's help says which) and how PTZ, motion and a spotlight are reached:

- A Dahua's PTZ goes over DVRIP, or, with *PTZ and its light over its HTTP API* on (*Features*), its PTZ and white
  light go over Dahua's HTTP API.
- A Reolink's PTZ, motion, AI detections and spotlight go over its own HTTP API.
- Both APIs use the port and scheme *Features* names.
- Any other camera's PTZ goes over ONVIF.

Amcrest's light bulb cameras (set up in the Amcrest Link app, RTSP path `/live1`) have no DVRIP: they're Dahuas with
that switch on, if their HTTP API answers. They speak its XML dialect (`POST /dev-api/…`, Amcrest's "AMLINK CGI")
rather than the CGI (`GET /cgi-bin/…`); the app tries the CGI first, and the XML one when the CGI answers with an
error, with the same login. Dahua's older lighting table (`Lighting`) is the infrared lamp's on most cameras, so the
app never switches it. Changing the brand brings in the new brand's ports where the old brand's defaults still stand,
and keeps anything typed (`Cameras.withBrand`). A camera that names no brand is a Dahua.

**Detect**, which the footer's *Connect* runs (`Cameras.detect`, `ipcamera_core::detect`), first asks the camera
which brand it is without logging in, on every port at once: a Reolink's API answers "please login first"; a Dahua's
DVRIP port accepts a connection, and its HTTP API asks for a digest login at `magicBox`, in the CGI or the XML dialect.
Then, with the form's password, it fills in the form:

- **A Reolink**: it logs in once for its model, firmware and what it has.
- **A Dahua** (a DVRIP port or Dahua's HTTP API answered): it logs in once over each for its model, firmware, PTZ and
  light. PTZ stays on DVRIP where that answers; Detect turns *PTZ and its light over its HTTP API* on where DVRIP is
  closed or the camera has a light there.
- **Anything but a Reolink**: it also asks ONVIF's time, without a login, on the camera's ONVIF port and then on the
  ports cameras ship it on (80, 8000, 8899, 2020, 5000, 8080), takes the port that answered, and logs in once for the
  make, model, events, PTZ and a light among its PTZ service's auxiliary commands (a bulb's, not an infrared lamp). It
  turns *Watch it over ONVIF* on when the camera reports motion or has such a light.

The app keeps no list of models: the camera says what it has. *Watch it over ONVIF* (*Features*) stays off until you
or Detect turn it on, since not every camera has ONVIF on and a failed ONVIF login counts against the account like any
other. ONVIF that tells nothing about a Dahua is a note after what Detect found, not a failure. When ONVIF refuses a
login the camera accepts another way (a Reolink's or a Dahua's own protocol), that's remembered with the camera
(`onvif_refused`) and Detect doesn't log in to it again, since each refusal counts against the account; typing in the
address, the login or the ONVIF port or path again has it asked once more. A refused login ends Detect before any
other is tried.

Detect starts by finding which ports are open (none with a login), asks Dahua's HTTP API and ONVIF on any other open
port a camera might keep its API on, and when nothing answers, says which ports were open: only the video's means the
camera has no local API to reach. On the test picture, in tests and in the web preview, Detect asks nothing and says
it reaches no camera, which counts as an answer, so a camera can still be added there.

*Connection* reads in the order you enter a new camera: the name, the address, the username and password Detect logs
in with, the brand Detect fills in, then what the camera said. Its messages (what it found, or why nothing answered)
have a copy button (`CopyableNote` in `crates/dp/ui/note.tsx`), since nsh 0.5.1's text can't be selected.

### Searching the network

**Search network for devices**, the magnifier in the dialog's header, is for a camera whose address you don't know.
It opens *Cameras on the network* over the form (`views/finder.tsx`), which asks the network who's there as it opens
(`Cameras.discover`: ONVIF's WS-Discovery, which every camera with ONVIF on answers without a login) and lists those
that answer within about three seconds, by address, each under its model, with the name it gives itself, its address
and anything else its maker has it report (a serial number) beneath. A ring spins while it searches (`Spinner` in
`crates/dp/ui/spinner.tsx`). The question goes to everyone at once and, since Wi-Fi often drops such a question, to
each address on this machine's private networks one by one as well.

A camera with no ONVIF, or with it off, answers none of that but still sends video, so the search also tries RTSP's
port (554) on each of those addresses and asks whatever accepts the connection which requests it takes (`OPTIONS`, no
login, no stream named). A device that answers in RTSP is listed by its address as *Video over RTSP*, after the
cameras ONVIF found (`ipcamera_core::find_cameras`, `crates/ipcamera/core/src/camera_search.rs`).

Choosing a camera takes its address (with its ONVIF port and path, or for *Video over RTSP* its RTSP port) and its name
if the form has none, and closes the window. Its brand follows a moment later: the form runs Detect with the login left
out, so nothing is logged in to, and takes the brand it reports with that brand's ports (`withToldBrand` in
`views/finder.tsx`), unless the address or the brand changed meanwhile; where only the video answered, the brand is
left alone. Asking the camera what it is, with its login, is still Detect's job. *Search again* asks once more, since a
camera can miss a question. Only cameras on the same network hear it (on the web, the server's network), so one that
doesn't show up is still added by typing its address. On the test picture the search lists the cameras in
`IPCAMERA_FOUND`'s file where that's set ([Run it](#run-it)); otherwise, as in tests, it says it reaches no camera.

*Cameras on the network*, with what answered:

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../../../assets/screenshots/ipcamera-search-dark.png">
  <img src="../../../assets/screenshots/ipcamera-search-light.png" alt="IP Camera's search for cameras on the network, listing the six that answered by model, name and address">
</picture>

### Editing several cameras

With several cameras selected, *Edit* opens the same dialog without the name, the channel, *Features* and *Events*,
which belong to each camera, under a line naming the cameras it changes. A field shows the cameras' shared value, or
*Mixed* when they differ, and a mixed on/off setting shows a dash. A field changed there applies to all of them; its
undo button, or emptying a mixed field, lets each keep its own again. The password is never shown; typing one gives
them all the same. *Apply* saves them all in one write of `cameras.json` (`Cameras.saveChanges`), so either all change
or none do, and only the running streams of the cameras that changed reconnect. With one camera selected, its own
dialog opens instead.

### A camera's streams

A camera's streams come from the camera. Detect, run with a login, also asks which streams it has: its ONVIF media
service's profiles, with the RTSP address the camera gives for each, and a Reolink's own API. The dialog's Stream part
then offers those, and only those, as the stream menu's rows: size, codec and the camera's own name for it
(`1920×1080 · H.264 · main`), largest first, another lens's after the main lens's with the lens named. Choosing one
saves its path as the camera's RTSP path, its default stream, which *Stream path* under the list shows read-only, and
the list is saved with the camera.

A Reolink's web server also sends streams as FLV, some only that way: the one Reolink's apps call Balanced (`ext`),
and a tracking lens's sub stream (`autotrack_sub`). Nothing the camera replies lists them, so Detect opens each name
Reolink's guides give, with the login's token rather than the password, and lists those that send H.264 and aren't
already an RTSP stream, under their own names (`896×672 · H.264 · ext`); a second lens that answers to two names is
listed once. Their path is `flv:` and the name (`flv:channel0_ext.bcs`), and they play over the API port with that
login, getting a token each time they connect; *Stream path* says so when one is chosen. Only a Reolink takes one. FLV
carries no H.265 the app plays, so a camera's largest picture stays on RTSP.

When a camera first lists its streams, the form picks the one at the path that has played so far (from the brand,
channel and quality), or else the largest of its main lens's. A saved path that isn't in the list stays as a choice of
its own.

A stream nothing has sized is asked over RTSP: the SDP a camera answers DESCRIBE with gives its picture size
(`sized_streams` in `crates/ipcamera/media`). Where nothing listed the camera's streams, as for a camera whose ONVIF
refuses the login, Detect asks its own stream and, for a Dahua or a Reolink, the main and sub stream at the brand's
paths (named `main` and `sub`), with the saved login, one at a time, stopping at the first refusal; it lists them when
it finds two, which Auto can then pick between. A stream ONVIF listed without a size gets one the same way.

Only a camera that lists no streams shows *Stream quality* and an *RTSP path override* in place of the list: its
stream is reached by channel and quality, or at a path typed by hand. A camera with two lenses lists each lens's
streams, which is how to reach the one its zoom shows. Whether a stream is H.264 or H.265 is the camera's own setting:
the app plays either and changes neither.

## The Groups page

**Groups**, at the bottom of the sidebar between Cameras and Settings, replaces the video with a table of every group,
built like the Cameras page: the same header, table, footer, pager, selection and keys, from the same parts
(`views/listing.ts` holds the order, the selection and the pages for both, and `views/paged.tsx` the titles' checkbox
and the footer). Its header counts the groups and says what the search and filters keep, with three stats: the
cameras in a group, those in none, and the empty groups.

- **The bar** has a search box, which keeps the groups with every typed word in their name or their cameras' names,
  and two filters: a camera (the groups it's in) and a status (*Has a problem*, *Needs a password*, *Recording*,
  *Empty*). *New group* opens a group's dialog for a new group, which is selected once made.
- **The table** has a row per group, in the sidebar's order: its name (with the red dot while one of its cameras
  records), how many cameras it holds, which ones, in its order, and what they're doing (*2 of 3 connected*, or how
  many have a problem). A narrow window drops the cameras' names, then the status, then the count. Each title sorts by
  its column, then the other way, then back to the sidebar's order; the status puts groups with problems first, and an
  empty group goes last either way.
- **Selecting** works as on the Cameras page, with the same keys, except that Return opens one group's dialog. While
  some are selected, the floating bar offers *Cameras* (each camera checked when every selected group holds it, with a
  count when some do; choosing one puts it in all of them or takes it out of all of them), *Duplicate* and *Delete*.
  Deleting several is one question and one save of `groups.json` (`Groups.removeMany`): all of them go, or none do.
  Their cameras stay.
- **Duplicate** (also Control+D) adds a copy of each group with the same cameras in the same order, named after it
  ("Outside copy", then "Outside copy 2"), at the end, and selects the copies.
- **A row's menu** is the group's menu from the sidebar. *Move up* and *Move down* move it in the sidebar's order
  (`Groups.moveTo`); the page leaves them out while a column sorts the table.
- **A group's dialog** (the row's pencil, a double click, *Edit group…* in its menu, or *New group*) is the camera's
  dialog, at the same size, without parts: the group's name across the top, then two lists side by side, each
  scrolling on its own. On the left are the cameras in the group, in the sidebar's order, each moved up or down with
  its arrows or taken out; on the right, under a search, the other cameras, each added with its *+*. *Save* writes the
  name and the cameras in one save (`Groups.save`), and *Create* makes the group with its cameras in one save
  (`Groups.add`); a blank name, or a camera deleted meanwhile, changes nothing. The sidebar's *New group* still asks
  only for a name, as the Cameras page's *New group…* does for the selected cameras.

## Dashboard

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../../../assets/screenshots/ipcamera-dashboard-dark.png">
  <img src="../../../assets/screenshots/ipcamera-dashboard-light.png" alt="IP Camera's dashboard: the cameras, videos, snapshots and storage, what was saved each day, and the week's events by hour">
</picture>

**Dashboard**, at the top of the sidebar above the videos and the snapshots, replaces the video with a summary of the
cameras, what they've saved and what they've seen.

Its header (`crates/dp/ui/collapsing.tsx`) is one banner lifted off the page like a hero card. It's the app's card
(`crates/dp/ui/card.tsx`), the same one the dashboard's tiles and Settings' groups use, so its corners and outline
match theirs, inset from the page's edges with room under it for its shadow. The badge, title and subtitle fill its
top; the period it looks back over (7, 14 or 30 days) runs as tabs along its bottom, the chosen one underlined; the
texture goes inside it; and the page color shows around it. As the page scrolls, the top fades and slides up under the
title bar, the card fades away, and the tab bar settles flush at the top of the page, flat on the page color with the
theme's page line under it (none in Default), so the tabs stay in reach. The badge sits on one of the theme's chart
colors, its icon in the theme's `chartInk`, like a dashboard tile's chip; it keeps the elevation's edges and shadow but
not its sheen, which would wash the color out on a light page.

Under the header, the page is as wide as the window. On a window 1280 points wide or more, the cards sit on a
twelve-column grid, the charts over time taking most of it; on a narrower one they pair up in two columns, and below
1024 they stack in one. Each card is as tall as the tallest in its row.

- **Four numbers**, four across (two on a narrower window): the cameras, with how many are connected and recording;
  the videos, with their total length and size; the snapshots, with how many fall in the period; and the space both
  take on disk, with the drive's free space. The videos and the snapshots each have a small line beside the number
  showing how many were saved each day of the period, the day-by-day chart's two series apart. Each number opens the
  page it sums up: the Cameras page, the videos, the snapshots; the disk opens Settings over the dashboard, on
  General, where the folders are. Each has its icon on a chip of a chart color: the videos and the snapshots take
  their series' colors, so a tile, its line and the charts agree; the disk takes the drive's color in the storage
  ring, and the cameras the one left over. The page a tile opens has its header's badge in the same color (`CHART` in
  `views/model.ts`): the videos and snapshots pages their series', and the Cameras page and the dashboard itself the
  cameras', as its first tile.
- **Storage**, a third of the row under them: a ring for the drive under the folders (one per drive when they're on
  different drives) showing the videos, the snapshots, the drive's other files and the free space, with each part and
  its size to the ring's right. The pointer over a part puts its size in the middle.
- **Saved per day**, the other two thirds: a bar for each day of the period, by the local calendar, with its videos and
  snapshots stacked. The pointer, or the arrow keys once the chart has focus, picks out a day and says what it holds.
- **Events by hour**, two thirds of the next row: a cell for each hour of each weekday, Sunday at the top, holding the
  events that started in it over the period, its color fuller the more there are. Over two weeks or more each weekday
  adds up its days, so it shows when in the week the cameras see most. A cell with none is a bar's empty track, and the
  fewest above none are still a visible wash. The pointer, or the arrow keys, outlines a cell and says what it holds.
- **Busiest hours**, the third beside it: the same hours added up over the weekdays, a bar each, midnight first, with
  the busiest named in its card's line.
- **Latest events**, the larger part of the next row: the five newest, whatever the period, each with its picture,
  what the camera saw (with Now while it still sees it), which camera and how long ago. A press shows it in the events
  page's preview, with its picture open full size where it saved one, and **Show all** opens that page.
- **Space by camera**, beside them: what each camera's videos and snapshots take, largest first, each bar measured
  against the largest. Files from deleted cameras add up under one row.

The three event cards need the event log, which only the desktop keeps: on the web they're left out, and the space by
camera takes the whole row. The cameras themselves, with their status, are on the Cameras page, which the Cameras
number opens.

The files are the lists the snapshots and videos pages keep (`views/snapshots.ts`, `views/recordings.ts`), read again
whenever a folder changes, and every few seconds while a camera records, because a file being written doesn't change
the folder's listing. The events are the log `views/eventlog.ts` follows while the app runs. The free space comes from
`Storage.space` (`src/camera/storage.rs`, through `crates/ipcamera/storage` and the `fs4` crate), which asks the system
off the main thread; on the web, it's the server's drives, through the server. The charts are nsh's plot
(`nsh/base/plot`), which draws in Rust and follows the pointer with no JS per move, and they animate in when shown,
unless the system asks for less motion.

The colors are the theme's chart colors: `chart-1` for the videos and `chart-2` for the snapshots, wherever they show,
and `chart-4` for the drive, on the disk's chip and in the ring, where its other files are the full color and the free
space a wash of it, so the ring is in the theme's colors even when the app's files are a sliver of the drive.
`chart-3` is the cameras' chip and the events' color, as on the events page's badge: the busiest hours' bars, and the
heatmap's cells, whose ramp runs from a wash of it over the card to the full color (`heat` on `Look`: opaque colors,
which the plot blends between). A bar's empty track is a wash of the muted text rather than `muted`, which some themes
make their card's own color. Every theme file has its chart colors, and `theme.json` gives Default its own in place of
nsh's grays: violet (the brand's), orange, aqua, blue and amber, tuned for each appearance (light `#6250d6`,
`#eb6834`, `#1baf7a`, `#2a78d6`, `#eda100`; dark `#9085e9`, `#d95926`, `#199e70`, `#3987e5`, `#c98500`). They come
from a validated categorical palette, in an order checked against Default's cards for color-blind separation, for
neighbors told apart at a glance, and for the first three told apart wherever they meet, as they can in the storage
ring. The two series clear 3:1 on the card in both appearances. A chip's icon is the theme's `chartInk` (`Look`):
white, or the darkest of the theme's card and text, whichever reads better on all four chips, so a row of chips has
one ink color; `Brand.test.ts` checks it clears 3:1 on each in every theme.

## Snapshots

The camera button in the header saves the newest frame of every connected camera on screen, as the record button
records them: the cameras of the view, or the one camera shown on its own. A camera that isn't connected is left out;
with none connected, the button says why. Every frame is taken before any is written, so the pictures are of the same
moment. Each is a PNG in the snapshots folder (`data\snapshots` beside the executable, unless Settings names another),
named for when it was taken and which camera took it: `snapshot-1762560000-<camera id>.png`. It uses the id, not the
name, so renaming a camera keeps its snapshots. A PNG put in the folder by hand is listed as *Unknown camera*, and those
of a deleted camera as *Deleted camera*.

### The gallery

**Snapshots**, at the top of the sidebar under the videos, replaces the video and the PTZ panel with the gallery. It
has the dashboard's collapsing banner ([Dashboard](#dashboard)), at the same height. Under the title are the count and
what the filters keep; at the banner's other end are the stats (how many cameras, this month's pictures, the size on
disk), laid out like the dashboard's tiles: a small capitalized label after its icon, and the figure under it in the
tiles' type, its units smaller and muted so the numbers are what the eye compares. A window narrower than 1024 points
leaves them out. Along the banner's bottom, in place of tabs, are the filters (by camera, by tag and by date, and the
order and grouping) and the page's buttons. As the grid scrolls, the banner collapses to that bar, which stays, so the
filters stay in reach, and the count moves into it.

The pictures fill as many columns as fit, grouped by month, day or camera under headings that stick below the bar while
their pictures scroll past. A heading's title, after its chevron, folds its section away and opens it again
(`Library.toggleCollapsed`). Folded, the heading stays with its count, its pictures stay in the viewer's order and its
checkbox still selects them, and it stays folded until opened, even through another grouping and back. With no
grouping there are no headings and nothing folds.

A picture opens at full size (`PictureViewer` in `views/viewer.tsx`, which events open in too); the arrow keys step
through the gallery in its order, Delete deletes after asking, and Escape closes it. A picture's right-click menu also
shows it in Explorer or narrows the gallery to its camera, and, for one an event took, shows that event alone on the
events page.

### Tags

A snapshot or video an event saved is tagged, in its name, with what the camera saw as the event started
([Events](#events)), and shows the tag over its picture's corner (`Vehicle`), except in a List, whose picture is too
small for it. The Tag menu, after the camera menu, keeps All tags, those Taken by hand, those From events, or those
tagged with one thing the camera saw, each with its count, as the camera menu counts; on the events page it keeps the
events that saw one thing. The tags come from the names, so the folder listing is all the page reads for them, and the
web build, which lists the server's folders, filters by them too. An event's Show in Snapshots or Show in Videos opens
the page narrowed to its files (*One event* in the header's line) until the filters are cleared.

### Sizes

The View menu, last of the filters, shows the pictures Large, Medium (the default) or Small, where a row holds more of
them and they lose their captions, or as a List: a row each, its picture small at the start, a checkbox before it, and
its size at the end (`columnsFor` in `views/grid.tsx`). The videos and events pages have the same menu, each its own;
on the events page, List is a table of its own, and the page opens on it ([Events](#events)).

### Selecting

Selecting (`Library` in `views/library.ts`) works the same on the videos and events pages. A ring shows in a picture's
top left corner while the pointer is over it, and pressing it selects the picture; a press with Control does the same,
and Shift adds every picture from the one last picked, as in Explorer. Once anything is selected every ring shows, and
a plain press selects or deselects rather than opening, until the selection is cleared. A heading's checkbox selects
its whole section, shows a dash while only part of it is selected, and clears them all when they all are. Control+A
selects everything the filters keep; whatever a filter hides leaves the selection.

While anything is selected, a bar floats over the bottom of the page, as on the Cameras page: *Select all*, *Delete
(3)* in red like the delete question's own button (the `danger` kind of `Button`, the theme's `destructive`), and a
button that clears the selection, as Escape does. The count is on the button rather than beside it. Delete, the key or
the button, deletes the selection after asking once for all of them (a single one asks as its own Delete does); a right
press on one of several selected offers the same, *Delete (3)…*. They're deleted one after another, so if one fails it
says why and the rest still go.

### The folder and the thumbnails

The gallery reads the folder when it opens and checks it once a second while it shows, so files added or deleted in
Explorer appear by themselves. The check reads only the folder listing, not the files. Leaving the live video for the
gallery keeps the view's cameras connected without decoding them, as any of the app's own pages does, so their picture
is back at once on the way back.

The gallery shows each snapshot's thumbnail, never the picture itself: a PNG decodes at its full size before it's
scaled down, 8 MB for a 1080p one, so a gallery of full pictures would hold hundreds of megabytes. Each snapshot is
saved with a 480-pixel-wide PNG in a `thumbnails` folder inside the snapshots folder
(`crates/ipcamera/storage/src/thumbnail.rs`), as a recording is. A snapshot without one (its thumbnail couldn't be
written, or it was put there by hand) shows an icon until a background thread makes its thumbnail from the picture, one
at a time, after the folder is first listed. The viewer opens the picture itself. What `<Image>` decodes stays in memory
after it scrolls away, up to a budget nsh sets at a sixteenth of the machine's memory, 256 MiB at most; the app sets
64 MiB (`PICTURE_MEMORY` in `src/lib.rs`), enough for a few pages of thumbnails and the pictures last opened. Pictures on
screen are kept regardless of the budget.

Deleting a snapshot deletes the file for good; it doesn't go to the Recycle Bin. The filters, the order, the size and
the selection last until the app closes; they aren't saved.

## Events

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../../../assets/screenshots/ipcamera-events-dark.png">
  <img src="../../../assets/screenshots/ipcamera-events-light.png" alt="IP Camera's events page: today's events by time, what each camera saw and what it saved, with the newest one's snapshot and video beside the list">
</picture>

What the cameras see is logged as events, whether or not they're on screen, while *Events* is on in Settings'
Automation section, as it is unless turned off (`views/eventlog.ts`, `views/events.tsx`; `ipcamera_core::EventTracker`
and `EventLog`). An event is something a camera's AI tells apart (a person, a vehicle, an animal, a face, a package),
or motion for a camera that knows only motion; what it saw adds up as the event goes on. A camera whose AI tells things
apart logs nothing on plain motion, which trees, rain and shadows set off all day.

### Grouping

Each time the camera sees something is a sighting, from the first answer that sees it until the camera has seen nothing
for a while: ten seconds, or, while events save videos, the videos' *Keep recording* (30 seconds by default), so each
sighting is one video and `Seen 3 times` counts the videos. `Now` shows while it lasts. Where the event videos allow it
(*Motion*, on by default), plain motion keeps a sighting the AI started going between its detections, for up to two
minutes after the AI last saw something, so an animal that grazes in and out of the AI's sight stays one sighting;
motion never starts one (`MOTION_HOLDS_MS` in `ipcamera_core`).

The event stays open for Settings' *Group within* (five minutes by default) after the camera last saw something, and
whatever the camera sees within that window is another sighting of the same event. A car that pulls in, waits and
leaves, which the camera loses sight of in between, is one event with one picture and one alert, rather than one for
each time it comes back; the row says how many times it was seen. The window runs from the last sighting, so an event
lasts as long as things keep coming back. Each camera keeps its own events: a car two cameras see is an event at each.
*Don't group* ends an event with its sighting. Changing the window, or how event videos are made, restarts every watch,
which ends whatever was open; a camera's own event video settings restart only its own watch.

### A camera that opts out

The top of a camera's Events part is its own switch (`CameraConfig::log_events`, on unless turned off). Off, the app
logs nothing the camera sees, so it has no events, alerts, or event pictures and videos, and the camera is asked what
it sees only while it streams, for its badge, as every camera is when *Events* is off. Its event video settings
disappear from the part while it's off. Turning it off ends what it had going on, and the video an event of its was
recording (`watch_rules` in `src/camera/hub.rs`).

### What an event saves

What an event saves is Settings' *Save*: a picture (the default), a video, both, or nothing. The picture is taken as
the event starts: the camera's own (a Reolink's `Snap`, at its main stream's size), else the picture on screen, else
one from the camera's own stream, connected for it as for a recording and disconnected again, which comes a few seconds
late. A camera the user disconnected, or one with no password, goes without. A video records each sighting, as the
camera's event video settings say: Settings' *Event videos* for every camera, unless the camera's Events part gives it
its own in their place as a whole (`ipcamera_core::EventVideo`, `CameraConfig::event_video`; `views/eventvideo.ts`):

- *Keep recording* (10 seconds to 2 minutes; 30 seconds): how long recording continues after the camera last saw
  something, so whatever it sees again sooner continues the same video rather than starting another.
- *Start before* (Off, or 5 to 30 seconds; 10): each video starts that long before the camera saw something, or a
  little more, back to the keyframe before. Each camera whose events save videos with this on keeps that stream
  connected all the time, though no tile shows it, decoding nothing and holding the last seconds in memory as the camera
  sent them (32 MB at most per camera), so the video starts at once from the oldest keyframe held rather than connecting
  and waiting for one. That costs the stream's bandwidth around the clock, and one of the camera's connections. Off, a
  camera that isn't connected connects its own stream when it sees something, as if Record had been pressed for it
  alone, and its first seconds can be missed.
- *Longest video* (No limit, or 1 minute to an hour; 10 minutes): a video that runs longer continues in a new file from
  that keyframe, with nothing lost between them, both linked to the event.
- *Motion* (on): plain motion holds a video open between the AI's detections, as above.
- *Stream* (the camera's own): which of its streams the videos record. Settings offers each camera's own, its main or
  its sub stream; a camera's Events part offers its own or one it listed.
- *Save videos of* (all): what the camera has to see for an event to save a video. Every event is still logged; one
  that sees only kinds left out saves no video, and a picked kind that comes into a sighting of another starts one. A
  camera that knows only motion saves one for motion whatever this holds.

A camera already recording is left as it is, and a recording the user started never stops for an event. One that
can't record says why, once, in a message. What an event saves is snapshots and recordings like any others, in the
snapshots and videos folders, under the storage limits, each named with the time of its first picture, the camera's
id and what the camera saw (`snapshot-1762560000-<camera id>.vehicle.png`,
`video-1762560000-<camera id>.person.vehicle.mp4`, `motion` for a camera that knows only motion), and the event names
them in the log. A video opens with what the camera saw as it started, and once it closes it's renamed to carry
everything the camera saw while it recorded, so a car that came during a deer's video tags it `vehicle.animal`; its
thumbnail is renamed with it, and its event's link follows (`Hub::retag_closed`). A video open in a player keeps the
name it had. The snapshots and videos pages show those tags and filter by them ([Snapshots](#snapshots)).

### The log

The log keeps the newest thousand events in `data\events\events.json`; one that goes to make room leaves its files,
which are snapshots and videos like any others. An event still going on when the app closes ends where it started,
since its real end can't be known, and one whose camera stops being watched (deleted, changed, or refusing the login)
ends then.

### The Events page

**Events**, under Snapshots in the sidebar, lists them as the snapshots are listed (the same header, filters,
grouping, sizes, selecting and grid), but opens as a log, the View menu's List (`LibraryTable` in `views/grid.tsx`,
given by `views/events.tsx`): a row each, under a heading for each day that folds its rows away, like the gallery's.
The table is as wide as its columns, about 630 points, and stops there rather than stretching across a wide window; on
a narrower page, the column for what the camera saw takes what's left. The column titles stick under the header's bar
while the rows scroll (an nsh list sticks one row at a time, so the day headings scroll with the rows here): a checkbox
that selects every event the filters keep, then Time, which sorts newest or oldest first like the header's order,
Event, Camera and Saved. A row holds its checkbox, the time, what the camera saw (with `Seen 6 times` after it when it
came back, and `Now` while it goes on), the camera, a mark for each kind of thing it saved (a picture; its videos, with
a count past one), and its `…`.

### The preview

Where the page leaves at least 420 points beside the table at its narrowest (with the camera's column at 120 points),
a card there shows one event at a time (`TableAside` in `views/grid.tsx`): the first row's until another is picked.
The table starts at its columns' width and the card takes the rest. The line between them (an nsh
`ResizablePanelGroup`, its line showing under the pointer, `LINE_HANDLE`) drags the table wider, giving the room to
the camera's column, or narrower, down to its narrowest, but never leaves the card less than its 420 points. The width
is saved when the drag ends (`events_table_width` in `settings.json`, none at first) and kept as the window resizes,
the card taking what's left; the web build forgets it when the page reloads.

A plain press on a row shows it in the card, and Up and Down step through the rows the table shows (skipping folded
days), scrolling as little as needed to keep the row in view; that row is highlighted like a selected one. The card
holds:

- the event's picture, which a press opens full size in the viewer, or one of its videos in place with the system's
  controls, showing its first picture until its play button is pressed, so opening the page or stepping through the rows
  plays nothing; a video picked from the strip plays (one still being written shows its first picture and REC until it
  stops);
- under it, what the camera saw, which camera, when, how long the camera saw it (`Lasted 2 min 48 s`, from the log's
  `ended`) and how many times;
- when it saved more than one thing, a strip of its picture and each video with its length, a press showing that one;
- Show camera, Show in Snapshots, Show in Videos and the event's Delete.

The picture is as wide as the card at 16:9, unless that's taller than the room under the header at the top of the page,
so the whole card shows before anything scrolls; as the header collapses the card moves up with it, then stays under
the header's bar while the rows scroll on. The videos are found in the videos folder's listing, which the page reads
when it opens and whenever a file comes or goes, as the videos page does (`eventVideos` in `views/linked.ts`). The
preview shows the snapshot itself, one at a time; rows and tiles show thumbnails. Where the page is narrower, the table
stands alone, and a plain press on a row selects it, as in Explorer and on the Cameras page.

### Selecting, opening and deleting

Control and Shift select either way, and so does a row's checkbox. A double press on a row opens what the event saved
at full size (`openEvent` in `views/linked.ts`): its picture in the viewer, which steps through the events that saved
one and has a button that shows the camera on its own; else its videos on the videos page, narrowed to them, so the
player's arrows step through the rest. At the picture sizes, each event is a picture with what, which camera and when
under it, and `Now` in its corner: its snapshot's thumbnail, else its first video's, else the empty frame; a press
opens what it saved the same way.

An event's menu, from a right press or the row's `…`, opens its picture, plays its videos, shows its files on the
snapshots or videos page narrowed to them, shows the camera, narrows the page to the camera, or deletes the event.
Deleting one that saved nothing asks nothing, since it's only a note of what was seen; one that saved files asks first,
naming them (`Its picture and its 6 videos are deleted for good`), since they go too, from the snapshots and videos
pages, except a recording still being written. A file deleted from its own page leaves its event, which then shows one
fewer. Several selected are deleted after one question, in one save of the log (`Events.delete` takes their ids). The
header's bin deletes every event the cameras don't currently see, after asking. An event the camera sees now can't be
deleted, alone or in a selection, since its watch would only log it again at its next change. The Tag filter here keeps
the events that saw one thing (`Vehicle`).

### Alerts

As an event starts, a message at the bottom right says what and where (`Person at Backyard.`), with a Show button that
opens the camera (`notify`'s `action`): once per event, not again when it's seen again within its window. What the log
held when the app opened isn't news. Settings' Automation section turns alerts off (*Alerts*, `settings.json`'s
`alerts`). The messages show only inside the app; nsh 0.5.1's `Notifications` could show them outside it too.

### On the web

There's no event log on the web yet (`Events.kept` is false): the server's watch keeps no events, and the page says
the desktop app logs them.

## Recording and videos

The round button beside the snapshot button records every connected camera on screen: the cameras of the view, or the
one camera shown on its own. It turns red while anything records, and pressing it then stops every recording, so it
can stop them from anywhere, the videos page included. A recording camera shows REC on its tile and a red dot in the
sidebar, and so does **Videos** while anything records. A camera that isn't connected is left out; with none
connected, the button says why.

A recording camera stays connected when it leaves the screen, so switching to another view or to the videos doesn't
end it. Stopping the recording, or Disconnect, does, and a camera kept only for recording then disconnects if it's off
screen. A recording that can't write its file (a full disk, say) stops by itself and says why in a message.

### Automation

**Automation** is Settings' Automation section (`settings.json`'s `automation`); everything is off until turned on,
except *Events*. The hub's events thread does the work (`Hub::watch_cameras`, `Hub::log_event`,
`Hub::record_sighting` and `Hub::keep_within_limits`):

- *Events* (`log_events`, on) watches every camera the app can and logs what each sees ([Events](#events)). Off, a
  camera is watched only while it streams, for its badge and its spotlight, and its watch keeps nothing: no log, no
  alerts and nothing saved for events, whose settings the section then hides. Turning it off ends what's going on and
  stops whatever an event was recording; switching it either way restarts every watch. A new watch, after a camera's
  settings change, ends what the old one saw going on, and the recording it started.
- *Save* (`event_capture`: a picture by default, a video, both, or nothing) is what each event saves into the
  snapshots and videos folders, tagged with what the camera saw: a snapshot as it starts, and a recording of each
  sighting ([Events](#events)).
- *Event videos* (`event_video`), shown while events save videos, is how those videos are made for every camera
  without settings of its own: how long they keep recording, the seconds before they start with, the longest a file
  runs, whether motion holds one open, the stream, and the kinds that save one ([Events](#events)). A camera's dialog
  has an Events part that turns its events off (`cameras.json`'s `log_events`) or gives it videos of its own in their
  place (`event_video`).
- *Group within* (`event_window_minutes`, 5; 0 is *Don't group*) is how long after a camera last saw something what it
  sees still joins the same event ([Events](#events)).
- *All media* caps the snapshots and videos together (both folders, not the thumbnails), including what events save.
  At the cap, a snapshot or recording is refused with the reason, and while anything records the folders are added up
  every second and every recording stops once they reach it. *Make room*, offered under a cap, instead deletes the
  oldest snapshots and videos, for good and with their thumbnails, until the rest fit; it never deletes a video still
  being written, so recordings stop only when nothing else is left to delete (`ipcamera_storage::quota`).
- *Video file size* makes a recording whose file reaches it continue in a new file at the next keyframe, the user's
  recordings and events' alike, so nothing is lost between them.

The limits apply to what the desktop app saves. On the web the section isn't shown (`Automation.available` is false):
the server records and takes snapshots without them.

### The files

Each recording is the camera's own stream (or, for an event, the stream its event video settings pick), copied as it
arrives into an MP4 in the videos folder (`data\videos` beside the executable, unless Settings names another):
`video-1762560000-<camera id>.mp4`, named like a snapshot, from when its first picture arrived. Nothing is decoded or
re-encoded, so recording costs little CPU and loses nothing. H.264 or H.265 video goes in with AAC sound; G.711 sound
has no place in an MP4, so those cameras record only their picture.

The file is a fragmented MP4, written about a second at a time, so one cut short by a crash or power cut still plays
up to its last second. When it closes its header gets its length, which lets a player show it and scale its scrubber.
A file starts at a keyframe, and a new one starts when the stream reconnects, when the picture changes size, or when
the file reaches *Video file size* or an event's *Longest video*. An event's video can start from the seconds held
back before it (*Start before*). `crates/ipcamera/media/src/record.rs` has the details.

Each recording also gets a thumbnail of its first picture, a small PNG of the same name in a `thumbnails` folder
inside the videos folder.

### The Videos page

**Videos**, at the top of the sidebar under the dashboard, lays the recordings out like the snapshots gallery, by day
until another grouping is picked, under the same collapsing header, and each tile shows the thumbnail and the length
(at the row's end in a List, whose picture is too small for it). The same camera, date, sort, group and View menus
narrow it, and it selects the same way, separately from the gallery. A recording still in progress shows REC and its
length so far, and can't be played or deleted until it stops: deleting a selection skips it, and the question says
so. A recording opens in the system's own player over the window, with the arrow keys stepping through the page,
Delete deleting after asking, and Escape closing it. A recording's menu also shows it in Explorer.

The page reads the folder when it opens and whenever a file comes or goes, and every two seconds while a camera
records, because a folder's listing isn't updated for a file still being written. Deleting a recording deletes it and
its thumbnail for good.

## Live video

`<Video>` plays through Media Foundation and `Canvas` has no pixel access, so neither can show what
`crates/ipcamera/media` decodes. `ipcamera.video` is a native component registered with
`#[native_components(windows)]`. nsh hands it the Direct2D context it draws the window with, which makes the picture
ordinary view content: controls draw over it, and it clips and resizes like a view.

`crates/ipcamera/media` decodes both H.264 and H.265 in Rust and plays the sound through WASAPI, so the app needs
nothing installed and `nsh package` has nothing to bundle beside it. IP Camera's README has the details
([What it needs](../README.md#what-it-needs)).

A frame's work happens once per stream, however many tiles show it. Each stream on screen has one bitmap, reused for
as long as it runs, which every tile showing it draws, so a live feed allocates no GPU memory per frame. Each stream
also has one thread that looks for new frames and asks all its tiles to draw at once, so the window draws once per
frame (`src/video.rs`: the component's `camera` and `stream` props, and the camera's `Output` for that stream). A
camera no tile shows has no bitmap and no thread, and a connected one isn't decoded either
([What streams follows what is on screen](#what-streams-follows-what-is-on-screen)); the test picture stops drawing
the same way.

### A picture that isn't live

A stream that loses its camera keeps its last picture in its slot while it reconnects, and the window keeps drawing
it, so on its own a dropped camera would look like a still scene. Instead, a tile's picture, or a camera's on its own,
is covered (`PictureNotice` in `crates/dp/ui/media.tsx`, chosen by `pictureNotice` in `views/model.ts`): a dark layer
over all of it (`OVERLAY.dim`), which leaves the last picture visible but plainly not live, and over that an icon, a
title, what happened and what to do:

- A camera that isn't connected says *Not connected* and why, with Connect, or the camera's settings when it needs a
  password first.
- A stream that lost the camera says *Connection lost* (*Can't reach the camera* when it never had a picture), which
  attempt it's on, and when its last picture came. It needs no button, since it reconnects by itself.
- A stream that gave up for good (one the app can't play, a camera that never came back, *Reconnect by itself* off)
  says *Stream stopped* and why, with Try again, which is Connect: it restarts a stream that gave up, which nothing
  else does.
- One whose login the camera refused says *Login refused*, with the camera's settings, so the same password isn't sent
  again.

Each stream reports this for the tiles that show it (`RunningStream.trouble`, `problem` and `lastPictureAt`, from
`src/camera/status.rs` and `stats.rs`), so a tile on the sub stream reports the sub stream's state. A stream
connecting for the first time isn't covered: its numbers already say it's waiting for a picture. On the web the
server's player says the same in its own words, over its own picture dimmed the same way
(`crates/ipcamera/server/player/index.html`).

### Zoom

The wheel over a picture, a tile's or a camera's on its own, zooms in up to 8 × and back out, around the point under
the pointer, which stays under it; a precision touchpad zooms as smoothly. Only the picture under the pointer zooms,
each tile on its own, and the zoom draws part of the same bitmap larger, so it costs nothing per frame. With a fit that
crops (Fill), it zooms into what the tile shows. The chip over the picture shows how far in it is (`2.4×`), and while
it's zoomed the toolbar has Show all, which goes back to the whole picture, as does the wheel the other way.

Drag a zoomed picture to look around it, as far as its edges; it shows the grab cursor. A picture that isn't zoomed
takes no drag, so a drag over it stays a press on the tile. A tile switched to another camera or stream starts whole,
and the zoom isn't saved.

The zoom is `src/video.rs`'s (`Zoom`, the `wheel` hook): the component takes the wheel itself, so a step never waits
on JavaScript, reports its scale to TSX with its `zoom` event, and takes `resetZoom` as a command. The drag follows the
pointer in the same frame: `views/picture.tsx` binds it (`pan`) to two animated values the component reads as its
`panX` and `panY` props, which nsh sets on every frame the drag moves, with no JavaScript in between. Each drag adds to
the last (`extractOffset` when it ends), so the values never jump back. The web build has no zoom yet: there the
picture is the server's player in a web view.

### What a camera sees

A Reolink with a password, a Dahua with *PTZ and its light over its HTTP API* on, and any other camera with *Watch it
over ONVIF* on are watched for as long as the app runs, on screen or not, while events are logged (Settings →
Automation, on unless turned off; off, only while the camera streams) (`ipcamera_core::CameraWatch`, kept going by the
hub's events thread, `Camera::tend_watch`).

The watch starts as soon as nothing else is logging in: a camera the screen is about to connect waits for its stream
to go live, so a wrong password costs the camera one failed login rather than two at once. A camera that refused a
stream's login isn't watched until its settings change, and one that refused the watch's login doesn't connect by
itself either, its status saying why: the same password would only be refused again.

The watch follows what moves and what the camera tells apart, about once a second:

- A Reolink is asked over its own API, its spotlight too.
- A Dahua over its HTTP API is asked about its white light every five seconds, and switching the light sets every
  profile's row to `Manual` or `Off` (a light left in `Auto` reads as off).
- Any other camera sends its ONVIF events to a pull-point subscription, and a light bulb camera's light is switched
  with its PTZ service's auxiliary command (ONVIF can't say whether it's on, so the switch shows how it was last set).

`CameraState` carries it (`motion`, `detected`, `spotlight`, `spotlightOn`, `watchStatus`). A tile showing the camera
puts what it sees in a chip at the picture's bottom left (`Person, Vehicle`, else `Motion`); a camera with a spotlight
gets its switch (`sun`, amber while on) over the picture at the top right, beside the tile's `…`, both on the same dark
chip as the camera's name (`OverlayBar`, buttons of kind `overlay`, white in any theme: `OVERLAY` in
`crates/dp/ui/brand.tsx`) (`Cameras.setSpotlight`). Motion isn't worked out from the picture: decoding is the heaviest
work, and a camera no tile shows isn't decoded, while the camera's own detection costs the app one request a second. On
the web the server watches, and its status carries what the camera sees.

## Settings

**Settings**, at the bottom of the sidebar, opens a dialog over the dimmed window, whatever page shows; the page stays
as it was behind it, and stays selected in the sidebar. The dialog is the one a camera's settings open in
(`EditorDialog` in `crates/dp/ui/editor.tsx`), at its wide size, 1000 × 640 points (smaller only in a smaller window),
as every product's Settings is (`SettingsDialog` in `crates/dp/ui/settings.tsx`): a header with its badge, title and
close button, its sections down the left side in the sidebar's style, each with its icon, the chosen section's
settings scrolling beside them, and a footer. Each setting applies and saves as it changes, so there's nothing to save:
the footer says so, beside Done. Escape, a press on the dimmed window, the close button and Done close it. While the
app runs, it reopens on the section you left; the dashboard's disk and the status bar's drive open it on General, and
the status bar's media gauge on Automation. The sections are:

- **General** (the cog) has where new snapshots and videos are saved. *Change…* opens the system's folder dialog;
  *Use default* goes back to the folder beside the app. A folder the app can't write to is refused when it's chosen,
  not when the next snapshot fails. Files already saved stay where they are, and the videos folder can't change while
  a camera records. Both are saved in `settings.json`'s `storage`.
- **Automation** (the bell), on the desktop only, has whether the app logs events, what it does by itself when a
  camera sees something (an alert and a recording), how event videos are made while events save them (*Event
  videos*: [Events](#events)), and the storage limits ([Recording and videos](#recording-and-videos)), each saved as it
  changes (`views/automation.tsx`). A limit is picked from a menu of sizes; one set by hand in `settings.json` shows
  among them.
- **Appearance** (the palette), shared by every product (`appearanceSection`), has one panel of settings, each with its
  own icon: the mode (Light, Dark or System, which follows the system) as a segmented control; what goes over every
  theme, the texture (None, Haze or Dots; see [Textures](../../README.md#textures)) and the elevation (None, Soft or
  Raised; see [Elevation](../../README.md#elevation)); then the theme, one dark and one light picked, its line naming
  the picks.

Under the theme setting are tiles, each a small window in that theme's colors, four to a row at the dialog's size: one
Default first, in a section with no heading, then the theme files under their appearance's name (by name, then high
contrast). Default is one theme whose appearance the mode decides: its tile draws the Default the app would show (with
System, the one for the current appearance), is marked while the app shows Default, and picks Default for the mode's
appearance, or for both with System. Light and Dark show their own theme files; System shows the dark ones under Dark
and the light ones under Light, marks the one in use, and switches between the picks as the system does. Changing the
mode never changes a pick.

The palette icon in the title bar, beside full screen, is a quicker way to both: its menu starts with the mode, the
same switch as here and saved the same way, which leaves the menu open on the new appearance's themes; below it are the
themes for the appearance the mode shows (with System, the one the system shows), each beside a miniature window, and
picking one picks it for that appearance as the tiles do. The choices are saved in `settings.json`'s `ui` as `dark`,
`follow_system`, `dark_theme`, `light_theme`, `texture` and `elevation`; Default is saved by its name for each
appearance, `default-dark` or `default-light`. A fresh install follows the system (System), with Default for both
appearances, no texture and no elevation.

Settings are rows in cards (`crates/dp/ui/rows.tsx`), in the style of nsh's gallery: each a tinted icon, its name with a
line about it, and its control at the end, moving under the name only in a window too narrow for both. Anything under a
setting (a folder's path, the theme tiles) lines up with its name. A panel holds several settings in one row, with no
lines between them. A section's settings fade up into place one after another as it opens (`crates/dp/ui/appear.tsx`).

A camera's own settings stay in its dialog, from its menu or the + in the sidebar; several cameras' settings are
changed together on the Cameras page.

## Themes

The themes, theme files, textures and elevation are the same in every product, so they're documented in
[`crates/README.md`](../../README.md#themes): how a theme is layered, Default's colors, adding a theme, and what each
texture and elevation draws.

## Tests

```powershell
nsh test
```

This runs the Rust tests, then every `*.test.ts(x)` file in a headless runner. That runner gets throwaway settings and
can't reach a camera, even if a test presses Connect, because it sits beside the development host and would otherwise
read the saved password.

| Test | What it covers |
| --- | --- |
| `App.test.tsx` | The app as a whole: the Cameras page over the cameras it adds, the Groups page with a group's dialog and menu, the camera's and Settings' dialogs, and the pages that open empty |
| `Brand.test.ts` | The colors the shared controls and Markdown use against their surfaces in every theme, and the chips' ink ([`crates/README.md`](../../README.md#theme-files)) |
| `Cameras.test.ts` | The Cameras page's search, filters, order, selection and shared values, on cameras it makes up |
| `Groups.test.ts` | The same for the Groups page, on groups it makes up |
| `Settings.test.ts` | A new camera's Add button, by what Connect has asked and heard (`addChoice` in `views/settings.tsx`) |
| `Finder.test.ts` | How a camera found on the network is listed (`views/finder.tsx`) |
| `Layouts.test.ts` | Editing a view's layout on the 12 × 12 grid: moving, resizing, adding and removing tiles, the layouts a draft starts from, and the cameras its frames show |
| `Sidebar.test.tsx` | Reordering All cameras: dragging a camera by its grip, from its menu, and with Alt+Down while it's open on its own |
| `Notice.test.ts` | What covers a camera's picture while it isn't live (`pictureNotice` in `views/model.ts`), on cameras and streams it makes up |
| `Snapshots.test.ts` | The gallery's filtering, order and grouping, on snapshots it makes up |
| `Recordings.test.ts` | The videos page's filters, grouping and labels |
| `Events.test.ts` | The events page's menus, labels and preview, on events it makes up |
| `Dashboard.test.ts` | The dashboard's sums, and the page key the app reopens on |
| `Automation.test.ts` | The menus in Settings' Automation section: what each event saves, how long it stays open, how its videos are made, and the storage limits |

`App.test.tsx` can only open the gallery, the videos and the events empty: nsh grants a test no file system, so it
can't put files where the app reads them, and granting the app's own `capabilities.fs.write` for a test would widen
what the shipped app may do. The page tests above cover those pages on made-up data instead, and the files are tested
where they're written: `crates/ipcamera/storage/src/snapshot.rs` and `recording.rs`, recording itself in
`crates/ipcamera/media` against the pretend camera, and the server's snapshots, recording and files in
`crates/ipcamera/server/tests`.

The camera's dialog shows one part at a time, so a test presses a part by its id (`id:section:stream`) before its
fields; Settings' dialog the same (`id:section:general`), since it opens on the section it was left on.

A run's tests share its preferences, and the app opens on the page it closed on, so `App.test.tsx`'s `start` goes to
the Default view before each test, and `launch` opens the app as the last test left it.

```powershell
$env:IPCAMERA_SOURCE = 'pattern'
nsh test --target windows
```

This runs in a real window, which is the real app: without `IPCAMERA_SOURCE` it connects to the camera. The headless
tests fail here, because they expect the throwaway settings, so name a test file of your own.

Headless tests read a model of the app, not the screen, so they can pass while a real launch shows no window at all.
After a change to the window or to drawing, launch the app and look at it.

## The web

`nsh run --target web` needs the setup in IP Camera's README ([The web build](../README.md#the-web-build)). A browser
can't open an RTSP connection to a camera, so on the web `ipcamera-server` does ([its README](../server/README.md)),
and keeps the cameras, groups and views. The web build is its client, and `src/web_backend.rs` is its backend:

- **It reads a copy of the server's library.** The bridge's reads are synchronous, so the web build holds a copy:
  `crates/ipcamera/core`'s own `Library`, made from the snapshot the server sends (`Library::from_snapshot`), with all
  the desktop's rules. `src/library.rs` and `src/form.rs` are the same code on both, so the web accepts and refuses
  what the desktop does, in the same words.
- **It changes the server's.** Every change, from a renamed group to a new camera, is a `crates/ipcamera/core`
  `Change` sent to the server, which applies it to its own library (`Library::apply`, as the desktop applies it to its
  files). The web build then takes a fresh copy, so the change shows at once, and a refusal reaches the usual toast.
  Changes made in another browser arrive within a second: while the app listens for changes, a task asks the server
  for its status (`GET /api/status`), which says whether the library, the snapshots or the recordings changed, which
  cameras record and what their PTZ says. Requests go through nsh's Rust `fetch`, which is the browser's own on the
  web, so the web build needs no JavaScript glue.
- **It's never told a password.** The copy says which cameras have one. The settings form leaves the password empty
  and says that leaving it so keeps the saved one, which saving that way does.
- **Its tiles show the server's player.** `Cameras.liveUrl` gives each camera's page on the server
  (`/watch?camera=<id>`), which the stage shows in a `WebView` (an `<iframe>` on the web) where the desktop draws with
  Direct2D. The page takes no presses, so the tile under it keeps its own. A camera reads as connected: the player
  connects as it comes on screen and shows its own status. The player is served from where the server says, which is
  this machine under its other name (`127.0.0.1` for an app opened as `localhost`): nsh strips the origin from a frame
  showing a page of the app's own origin, and the browser then refuses that frame's connection to the server.

The server is at the page's own address when it serves the packaged web build (`cargo package-ipcamera-web` at the
repository root, with `.nsh\out\web` copied to `web` beside the server). `nsh run --target web` serves the page from
port 8787, so `IPCAMERA_SERVER` tells the build where the server is, and the server needs `--allow-origin` for that
page: `cargo ipcamera-web` at the repository root starts both that way ([the server's README](../server/README.md)), as
`.claude/launch.json` does. When no server answers, the web build is a preview: one pretend camera with no address,
kept in the page's memory and forgotten on reload, so the layout can still be worked on. It switches to the server's
library when one answers.

### Snapshots, recording and PTZ are the server's

The buttons are the desktop's, and the server does the work, in its folders and by the desktop's rules
(`crates/ipcamera/storage`):

- **A snapshot** is the server's newest picture of each camera on screen, saved as a PNG, and it says so as on the
  desktop. The server takes them all at once.
- **Recording** is the same one switch: it records the cameras on screen on the server, and stops every recording.
  The status says which cameras record, and why a recording stopped by itself.
- **PTZ** moves while a button is held. The server stops a move that isn't sent again within a second, so
  `web_backend.rs` resends a held move every 400 ms until it's let go, and a page that goes away mid-move can't leave
  the camera turning. A refused move says why in the PTZ panel's status.
- **The gallery and the videos page** list the server's files, and `<Image>` and `<Video>` show them from the server's
  URLs (`SnapshotInfo.path` and `thumbnail`, `VideoInfo.path` and `thumbnail`), videos with scrubbing. The server makes
  any missing snapshot thumbnails as the desktop does, when it first lists them. The server's folders can't be opened
  or chosen from a browser, and `Storage.abilities()` says so: "Open in a new tab" replaces "Show in Explorer" (nsh's
  `openUrl`, from the player's address, since it takes only a whole URL), and the folder buttons in Settings and the
  Open folder buttons don't show. The dashboard shows the free space on the server's drives. `nsh.json` declares no
  `capabilities`: an app without them may fetch, show web views and remote pictures, and open URLs, and declaring any
  one capability denies everything it doesn't list.

A camera with no saved password isn't tried by itself, for its live video either: its tile says to add one, or to
press Connect if it has none, which has the server try it once, as the desktop's Connect does. A camera whose stream
the server gave up on (a rejected login, a stream it can't play) reads as not connected, with the reason, and Connect
has the server try it again. In the preview, with no server, the gallery and the videos page are empty and these
buttons say they need the server or the desktop app.

### Signing in

A server with a password (`ipcamera-server --set-password`) shows the sign-in page (`views/signin.tsx`) in place of
the app until someone signs in, and Settings then has a Sign out. The page keeps its session key in memory only, so a
reload asks again. `Server` in the bridge holds these calls; on the desktop it never asks.

Sound still needs the desktop app.

### Menus are drawn in the browser

The desktop's menus are the system's own (`nsh/desktop`'s `Menu.popup`), which nsh 0.5.1 has nowhere else, so in a
browser the app draws the same items with `nsh/base`'s `Menu`, like the header's filter menus, in the theme's colors
(`crates/dp/ui/menu.tsx`). A button's menu, such as a tile's Add camera or a row's `…`, opens under the button
(`MenuOpener`); a right click's opens where it was (`popup`, in the newest `MenuLayer`: the window's, or that of a
dialog covering it, as the snapshot viewer and the video player do). Submenus cascade beside their item. Every menu's
items are the same objects on both, made in `views/actions.ts`.

In the browser the ids are counted rather than random (`crates/ipcamera/core/src/ids.rs`). Randomness there comes only
through JavaScript, and the glue for it (wasm-bindgen) would need its CLI on every machine that builds the web. Ids are
made where the files are, on the server, so the web build's own only need to differ within the preview's memory.

### Web streaming

The server connects to the cameras and passes each one's compressed video on over a WebSocket, and its page plays it
with the browser's WebCodecs in plain JavaScript. glass2glass has browser elements for that half too, but they wrap
the same decoder and would need wasm-bindgen's CLI at an exact version and a second wasm build.

H.265 depends on the browser. Many cameras send H.265, and a browser decodes it only with a hardware decoder, which
not every browser has: the page says so, and the camera's H.264 sub stream plays everywhere.

Still to do, in order:

- **HTTPS**, before the server listens beyond this machine: it serves camera feeds, settings, recordings and
  passwords, and its own password would cross the network in the clear.
- **The rest on the server**: sound, and choosing the server's folders from the browser.

## The window

The header and the sidebar's top row are the title bar: they drag the window, and the system's own minimize, maximize
and close buttons stay. The app opens on the page it closed on, saved in `settings.json`'s `ui` as `page` each time the
page changes, off the main thread, with the newest save winning whichever finishes last. If that view or camera has
been deleted, or no page was saved, it opens the first view. The smallest window is 200 × 150, nsh's fixed minimum,
and the layout copes with it. One-off messages, such as a saved snapshot or the reason something failed, show as
toasts.

The status bar (`views/statusbar.tsx`) runs along the bottom of the window, under the sidebar as well as every page, in
the sidebar's color in Default like the title bar (a theme file gives it its own `status-bar`). It sums up the app,
each item a colored icon and a number that opens its page (`StatusItem` in `crates/dp/ui/status.tsx`), in the color
each thing has everywhere (`CHART`): how many cameras are live (Cameras), how many record (Videos; red, as recording is
everywhere, and only while one does), and on the desktop the day's events (Events). At its right end are the space the
videos and the snapshots take and the free space on the drive under the videos (Settings), which `Storage.onUsage`
adds up from the folder listings every few seconds, off the main thread. Under a media limit (Settings → Automation)
those give way to a gauge of the media against the limit, amber from 90% (`Meter`), which opens Settings on
Automation, as the drive's free space opens it on General. A camera's own numbers are on its tiles.

## Workarounds for nsh 0.5.1

Everything that stands in for something nsh 0.5.1 lacks is listed for the whole suite, most of it in `crates/dp/ui`:
[`crates/README.md`](../../README.md#workarounds-for-nsh-051).
