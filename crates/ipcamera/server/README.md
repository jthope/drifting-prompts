# ipcamera-server

Part of [IP Camera](../README.md): the app without the desktop, in a browser. The server connects to the cameras and
relays their video, still compressed, over a WebSocket, and the browser decodes it with WebCodecs. It also keeps the
library the web build reads and changes, in the app's files and by its rules. The app's side is in
[its README](../app/README.md#the-web).

```powershell
cargo run -p ipcamera-server
```

Then open `http://127.0.0.1:8770/` (nsh takes 8790 and the web preview 8787). With the packaged web build beside it,
that's the whole app; without, the page lists the cameras and plays each at `/watch?camera=<id>`, plus
`&stream=<key>` for another stream (`main`, `sub`, or a path the camera listed).

## Options and files

It reads `settings.json` beside its executable (`target\debug` under `cargo run`) and keeps its cameras in
`data\settings` beside it. Snapshots go to `data\snapshots` and recordings to `data\videos`, unless `storage` names
other folders.

- `--settings <file>`: another `settings.json`; the desktop app's
  (`crates\ipcamera\app\target\debug\.nsh\settings.json`) shares its cameras, snapshots and videos.
- `--data <folder>`: another cameras folder only.
- `--port <port>`
- `--app <folder>`: the web build, in place of `web` beside the executable.
- `--allow-origin <origin>`: a page origin to let in, once per origin.
- `--set-password`, `--clear-password`: set or remove [its password](#only-this-machine), then exit.

The app and the server can run on the same files at once; each picks up the other's changes within a few seconds, and
two changes to one camera, group or view within those seconds end with the later. They share the cameras' passwords
only as the same user account ([why](../README.md#the-cameras-passwords)).

## The cameras

- A camera connects when its first viewer arrives and disconnects 30 seconds after the last leaves; viewers of one
  stream share a connection. A late viewer starts from the last keyframe, and one who falls behind skips to the next.
- A snapshot decodes the pictures since the last keyframe: well under a second for H.264, up to two seconds for H.265.
- A recording copies the camera's own stream, with AAC sound, into a fragmented MP4, and keeps the camera connected
  until it stops.
- A PTZ move stops unless sent again within a second; the web build resends a held one every 400 ms.
- A watched camera (a Reolink, a Dahua over its HTTP API, or one over ONVIF, as on the desktop) is followed about once
  a second once its stream is live, with the stream's login; a refused login ends the watch.
- Nothing tries a camera with no saved password, since failed logins can lock its account. A rejected login or a stream
  the server can't play isn't tried again until the camera's settings change; Connect tries either once.
- Ctrl+C stops the server once moving cameras have their stop and recordings are finished, waiting at most eight
  seconds for a camera that doesn't answer.

## The web build

`cargo package-ipcamera-web`, from the repository root, builds it into `crates\ipcamera\app\.nsh\out\web`. Copy that to
`web` beside the server's executable (or name it with `--app`), and the server serves it at `/`. It picks up other
browsers' changes and new files within a second, and says so within a few seconds when the server stops answering.

To work on it, `cargo ipcamera-web` from the repository root starts the server on the desktop app's settings and
`nsh run --target web` at `http://localhost:8787/`. Ctrl+C stops both, and arguments go to the server. The two commands
on their own:

```powershell
cargo run -p ipcamera-server -- --allow-origin http://localhost:8787 --allow-origin http://127.0.0.1:8787
```

```powershell
$env:IPCAMERA_SERVER = 'http://127.0.0.1:8770'; nsh run --target web
```

To try it without a real camera, run the pretend camera and add a camera at `127.0.0.1`, RTSP port `8554`, path
`/stream`:

```powershell
cargo run -p ipcamera-media --features pretend-camera --example pretend_camera
```

The web build has no sound and can't choose the server's folders. H.264 plays in every current browser; H.265 only
where the browser decodes it in hardware, and elsewhere the page says so.

## The API

A refusal is a 4xx whose body is a sentence saying why. The JSON is `ipcamera_core::api`'s and
`ipcamera_core::change`'s types.

| Route | What it does |
| --- | --- |
| `GET /api/library` | The library, without passwords, and which cameras have one |
| `GET /api/cameras` | Every camera's id and name, for the player page |
| `POST /api/changes` | Makes one `Change` and answers with what it added |
| `GET /api/status` | The library's revision, the files' counters, recording, PTZ and what watched cameras see |
| `POST /api/snapshots` | Snapshots of the cameras on screen |
| `POST`, `DELETE /api/recording` | Starts recording the cameras on screen; stops every recording |
| `POST /api/cameras/<id>/ptz`, `…/ptz/stop` | Moves a camera; stops it |
| `POST /api/cameras/<id>/connect` | Tries a camera once |
| `POST /api/cameras/<id>/spotlight` | Switches a watched camera's spotlight |
| `POST /api/detect` | Which camera is at an address, with the edited camera's saved password |
| `GET /api/discover` | Cameras on the network (ONVIF's discovery and RTSP's port; no login) |
| `GET /api/snapshots`, `GET /api/videos` | Lists the files |
| `DELETE /api/snapshots/<name>`, `DELETE /api/videos/<name>` | Deletes one |
| `GET /api/storage` | Where the files are and how full the drives are |
| `GET /files/snapshots/<name>`, `GET /files/videos/<name>` | A file, videos with range requests |
| `GET /files/snapshots/thumbnails/<name>`, `GET /files/videos/thumbnails/<name>` | Its thumbnail |
| `GET /live/<id>` | The camera's video over a WebSocket (`?stream=<key>` for another stream) |
| `GET`, `POST`, `DELETE /api/session` | Whether there's a password; sign in; sign out |
| `GET /watch`, `GET /player.js` | The player page and its script |
| `GET /` | The web build, or the player page without one |

## Only this machine

The server listens on 127.0.0.1 only, and turns away requests naming another host or from an origin that's neither its
own nor an `--allow-origin`. A tile's player is served under this machine's other name (`127.0.0.1` for `localhost`,
and the other way round), since a frame of the app's own origin loses its origin and the browser refuses its
connections. The cameras' passwords never leave the server.

By default the server has no password, and any program on this machine can use it. To set one:

```powershell
cargo run -p ipcamera-server -- --set-password
```

It asks on the terminal and saves a salted Argon2 hash under `server` in `settings.json`. The server then answers only
its pages' files until someone signs in; the session key goes with every request (`Authorization: Bearer`) and as
`?key=` in addresses the browser fetches itself. A session ends after twelve hours unused, on Sign out, or when the
server stops. This keeps out other users on this machine, not the network, which would need HTTPS first.

## How it is organized

| File | What it holds |
| --- | --- |
| `src/main.rs` | The command line |
| `src/lib.rs` | The routes, serving the web build, and turning away requests from elsewhere |
| `src/api.rs` | The library, its changes and the status; snapshots, recording, PTZ, spotlight and Detect requests |
| `src/streams.rs` | One relay session per camera stream, and snapshots and recording |
| `src/ptz.rs` | One PTZ connection per camera, and moves that stop by themselves |
| `src/access.rs` | Its password and who signed in |
| `src/closing.rs` | Closing PTZ and recordings as the server ends |
| `src/files.rs` | The saved files and their folders |
| `src/live.rs` | One viewer's WebSocket |
| `src/page.rs`, `player/` | The built-in player page |
