# ipcamera-server

Part of [IP Camera](../README.md). It serves the app's web build; the app's README covers that side
([The web](../app/README.md#the-web)).

`ipcamera-server` is IP Camera without the desktop app: the cameras, their groups and views, live video, snapshots,
recordings and PTZ, in a browser. A browser can't open the TCP connections RTSP needs, so the server connects to the
cameras and passes their video on over a WebSocket, still compressed as the camera sent it, and the browser decodes it
with WebCodecs. The server also keeps the library the web build reads and changes, and does everything else the
desktop does with a camera, in the same files and by the same rules. From the repository root:

```powershell
cargo run -p ipcamera-server
```

Then open `http://127.0.0.1:8770/`. With the packaged web build beside it (below), that's the whole app. Without one,
the page lists the cameras and plays each full size at `/watch?camera=<id>`, the page a web build's tile shows; add
`&stream=<key>` for another of the camera's streams (`main`, `sub`, or a path the camera listed).

- A camera connects when its first viewer arrives and disconnects 30 seconds after the last one leaves. Every viewer
  of a stream shares one connection; a tile on another stream opens a connection of its own, with the camera's login.
- Snapshots and recordings use the camera's own stream.
- A viewer who joins late starts at once from the last keyframe, and one who falls behind skips to the next keyframe
  rather than queuing.
- The port is 8770 because nsh's development server takes 8790 and the web preview 8787.

**Its data.** The server reads `settings.json` beside its executable (`target\debug` under `cargo run`), as the app
does, and keeps its cameras in `data\settings` beside it, in the same files as the app. Snapshots go to
`data\snapshots` and recordings to `data\videos`, unless the file's `storage` names other folders.

- `--settings <file>` uses another `settings.json`. The desktop app's (`crates\ipcamera\app\target\debug\settings.json`
  under `nsh run`) makes the server share the app's cameras, snapshots and videos.
- `--data <folder>` uses another cameras folder only.
- `--port <port>` changes the port.

The app and the server can run on the same files at once. Each keeps a copy of the cameras, groups and views while it
runs, and rereads a file before changing it if the other has written it since, so neither overwrites the other's
change. Each also checks every few seconds for what the other saved (the app every two, the server every five), and a
camera the other changed restarts with its new settings. Two changes to the same camera, group or view within those
few seconds still end with the later one.

They share the cameras' passwords only when they run as the same user account: `cameras.json` keeps each password
encrypted for the account that saved it ([`crates/ipcamera/README.md`](../README.md#what-it-saves)). Run as another
account (a service's, say), the server reads every camera as having no password, and its next save writes none.

**Snapshots, recording and PTZ.** These work as on the desktop, and the web build's buttons for them call the server:

- **A snapshot** is the newest picture the camera sent. The server decodes nothing while it relays, so a snapshot
  decodes the pictures since the last keyframe in a short graph of its own, with the desktop's decoders, and saves the
  last one as a PNG. That takes a keyframe interval of decoding: well under a second for H.264, up to a second or two
  for H.265, which decodes in Rust. A camera nobody is watching is connected for it.
- **A recording** copies the stream into a fragmented MP4, with an AAC camera's sound, as on the desktop, plus a
  thumbnail of the keyframe it starts on. A recording camera stays connected with nobody watching until recording
  stops. One switch records the cameras on screen and stops every recording, as on the desktop.
- **PTZ** moves while a button is held. The server holds the camera's DVRIP connection and stops any move that isn't
  sent again within a second. The web build resends a held move every 400 ms, so a tab closed mid-move can't leave the
  camera turning. A camera is sent only the newest command, so on a slow camera the stop doesn't wait behind the held
  move's repeats. A click released before its move went out still sends the move and then its stop, so a quick press on
  a zoom button always does something.
- **The Snapshots and Videos pages** list the server's files and show them from the server, videos with scrubbing.
  "Open in a new tab" replaces "Show in Explorer". Settings shows the server's folders and the dashboard shows the free
  space on its drives, but a browser can't open or choose them; set them in `settings.json` or from the desktop app.

None of these, and no viewer's live video, will try a camera with no saved password on their own: a blank login
fails, and failed logins can lock the camera's account. The viewer is told to add the password in Settings. Pressing
Connect tries such a camera once, as on the desktop, so a camera that really has no password plays from then on.
Connect also retries a camera the server gave up on for good (a rejected login, a stream it can't play):
`POST /api/cameras/<id>/connect`, one try per press.

**Stopping.** Ctrl+C stops the server once every moving camera has been sent its stop and every recording's file is
finished, which takes a moment. A camera that no longer answers gets eight seconds, and then the server exits anyway.

**The web build.** `cargo package-ipcamera-web`, from the repository root, builds the app as a static site in
`crates\ipcamera\app\.nsh\out\web` (it runs `nsh package --target web` there, without the `IPCAMERA_SERVER` a
development build gets). Copy that folder to `web` beside the server's executable (or point `--app <folder>` at it),
and the server serves the app at `/`, with its API and video on the same address. The web build holds a copy of the
server's library and sends every change to the server, which applies it with the desktop's rules and messages. It
picks up changes made in other browsers, and new snapshots and recordings, within a second. When the server stops
answering, every camera says so within a few seconds instead of showing what it last heard. Its tiles show each
camera's player.

**Working on the web build.** `nsh run --target web` serves the app from port 8787, so the build has to be told where
the server is, and the server has to let that page in. `cargo ipcamera-web`, from the repository root, does both: it
starts the server and the web build together in one console, with the web build at `http://localhost:8787/`. Ctrl+C
stops both, the server finishing any recording first; if one exits on its own (a build error, say), it stops the
other. The server reads the desktop app's own settings, the `settings.json` `nsh run` uses in
`crates\ipcamera\app\target\debug`, so the web build shows the same cameras, groups and views, and its snapshots and
recordings go to the same folders. Arguments after `cargo ipcamera-web` go to the server (`--settings` for another
settings file, say).

```powershell
cargo ipcamera-web
```

`cargo ipcamera-web` is an alias in `.cargo/config.toml` for `crates/xtask`, a small program that runs these two
commands, which also work on their own:

```powershell
cargo run -p ipcamera-server -- --allow-origin http://localhost:8787 --allow-origin http://127.0.0.1:8787
```

```powershell
$env:IPCAMERA_SERVER = 'http://127.0.0.1:8770'; nsh run --target web
```

With no server, the web build is a preview over a pretend camera kept in the page's memory. To try the server without
a real camera, run the pretend camera and add a camera at `127.0.0.1`, RTSP port `8554`, path `/stream`:

```powershell
cargo run -p ipcamera-media --features pretend-camera --example pretend_camera
```

**What it answers.**

- `GET /api/library` is the whole library, with every password taken out and a list of which cameras have one.
- `POST /api/changes` makes one change (`ipcamera_core::change::Change`, as JSON) and answers with what it added.
- `GET /api/status` is what the web build checks every second: the library's revision, numbers that change with the
  snapshots and the recordings, which cameras record and what their PTZ says, and what each watched camera sees
  (motion, its AI's detections, its spotlight).
- `POST /api/snapshots`, `POST` and `DELETE /api/recording`, and `POST /api/cameras/<id>/ptz` and `…/ptz/stop` take
  snapshots, start and stop recording, and move a camera. Snapshots and recording take the list of cameras on screen,
  skip those that can't, and refuse only when none can.
- `POST /api/cameras/<id>/spotlight` switches a watched camera's spotlight, and `POST /api/detect` tells which camera
  is at an address for the camera form, logging in with the saved password of the camera the form edits.
- `GET /api/discover` searches the server's network for cameras (ONVIF's WS-Discovery, and RTSP's port for those
  without ONVIF; no login) and returns those that answered, for the camera form's network search.
- `GET`, `POST` and `DELETE /api/session` say whether the server has a password, sign in and sign out
  (`crates/ipcamera/server/src/access.rs`).
- `GET /api/snapshots` and `/api/videos` list the files, `GET /files/…` serves them and their thumbnails
  (`/files/snapshots/thumbnails/<name>`, `/files/videos/thumbnails/<name>`), `DELETE` on either list's `/<name>`
  deletes one, and `GET /api/storage` says where they are and how full the drives are.

A refusal is a 4xx whose body is a sentence saying why, in the desktop's words. The JSON is `ipcamera_core::api`'s
and `change.rs`'s types, which the web build reads too; `crates/ipcamera/server/src/api.rs` and `files.rs` have the
details, and `live.rs` what goes over the video's WebSocket.

**Watching.** A Reolink with a password, a Dahua with *PTZ and its light over its HTTP API* on, or another camera with
*Watch it over ONVIF* on gets a `CameraWatch` (`crates/ipcamera/core/src/camera_watch.rs`) beside its relay once the
relay is live, as on the desktop: one thread that logs in (to Reolink's or Dahua's HTTP API, or ONVIF's services),
asks what the camera is and has, then follows what it sees about once a second, along with its spotlight or light. It
uses the stream's login, already shown to work, so a wrong password costs one failed login rather than two; a refused
one ends the watch for good.

**Only this machine.** The server listens on 127.0.0.1 only, and turns away a request that names another host or comes
from a page whose origin is neither its own nor one that `--allow-origin` names. A tile's player is served under this
machine's other name (`127.0.0.1` when the app was opened as `localhost`, and the other way round), because nsh strips
the origin from a frame of the app's own origin, and the browser then refuses its connections. The cameras' passwords
never leave the server: the web build's settings form shows none, and saving with the password left empty keeps the
saved one. A camera that rejects the login, or sends a stream the server can't play, isn't tried again until its
settings change or someone presses Connect, because failed logins can lock the camera's account.

**A password of its own.** By default the server asks for no password, and any program on this machine can use it.
To set one:

```powershell
cargo run -p ipcamera-server -- --set-password
```

It asks for the password on the terminal (never as an argument, which other programs could read), saves a salted
Argon2 hash of it under `server` in `settings.json`, and exits; `--clear-password` removes it. From its next start,
the server answers nothing but its pages' own files until someone signs in: the web build shows a sign-in page, and
the camera list a password box. Signing in gives the page a session key, which goes with every request
(`Authorization: Bearer`) and in the address of anything the browser fetches itself: a tile's player and its
WebSocket, a picture, a file opened in a tab (`?key=`). That's why every response tells the browser to send no
`Referer`. A session ends after twelve hours unused, on Sign out (in Settings), or when the server stops, and a reload
asks again, since the page keeps the key in memory only. A wrong password makes the next try wait two seconds. This
keeps other programs and users on this machine out. It isn't enough for listening on the network, which the server
doesn't do: the password would cross it in the clear, so that needs HTTPS first.

**What the web build can't do yet.** Sound still needs the desktop app, and the server's folders are set in its
`settings.json` or from the desktop app.

H.264 plays in every current browser. H.265, which many cameras send on their main stream, plays only where the
browser can decode it with the graphics hardware, as Chrome and Edge can on a PC whose graphics decode H.265.
Elsewhere the page says so; the camera's H.264 sub stream plays everywhere.

## How it is organized

| File | What it holds |
| --- | --- |
| `crates/ipcamera/server/src/lib.rs` | The server's routes, serving the packaged web build, and turning away requests that don't come from this machine |
| `crates/ipcamera/server/src/api.rs` | The library for the web build, its changes and the status, and taking snapshots, recording and moving cameras |
| `crates/ipcamera/server/src/streams.rs` | One relay session per camera stream, started by a viewer, a snapshot or a recording, and stopped a while after nothing needs it; snapshots and recording, of the camera's own stream |
| `crates/ipcamera/server/src/ptz.rs` | One PTZ connection per camera, and moves that stop by themselves unless sent again |
| `crates/ipcamera/server/src/files.rs` | The saved snapshots and recordings: listed, served with range requests, deleted, and their folders |
| `crates/ipcamera/server/src/live.rs` | One viewer's WebSocket, and what goes over it |
| `crates/ipcamera/server/player/` | The page that lists the cameras and plays one, decoding with the browser's WebCodecs |
