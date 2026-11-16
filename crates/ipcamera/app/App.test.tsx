// `nsh test` runs this file in a headless runner it builds with the crate. The
// video component is not registered there, so this covers the shell around it.
//
// That runner gets throwaway settings and cannot reach a camera (`is_test_runner`
// in `src/camera.rs`), so these tests start the same way on every machine.
// `nsh test --target windows` runs in the real app, which can: set
// `IPCAMERA_SOURCE=pattern` for that.
//
// Each step here costs the runner a tenth of a second at least, as it waits
// for the window to settle, so these check that the pages are wired to the
// logic the model tests (`*.test.ts`) and the crate's Rust tests check,
// rather than walking it again. What a test needs but doesn't test, a camera
// or a group, it
// makes and deletes through the bridge, as `Sidebar.test.tsx` does.
import { expect, open, test } from "nsh/test";
import type { App } from "nsh/test";
import { Cameras, Groups, Preferences } from "ipcamera/native";

/** A desktop window, on the page it last showed. Without `platform`, a headless run is a touch screen and shows no tooltips. */
const launch = (options?: { clock: object }) => open("./App.tsx", { platform: "windows", width: 1100, height: 760, ...options });

/** A desktop window on the Default view. The app opens on the page it closed on, and a run's tests share it, so each test starts here itself. */
async function start(options?: { clock: object }) {
  const app = await launch(options);
  await app.press("label:Default");
  await app.waitFor("label:Tile 4");
  return app;
}

/** Preferences outlive a test, so the panel can start either way. */
async function closePtz(app: App) {
  if (app.count("label:Collapse PTZ panel") > 0) await app.press("label:Collapse PTZ panel");
  await app.waitFor("label:Show PTZ");
}

/** Camera `name`'s id, or `""` when there is none. */
const cameraId = (name: string) => Cameras.list().find((camera) => camera.name === name)?.id ?? "";

/** Group `name`'s id, or `""` when there is none. */
const groupId = (name: string) => Groups.list().find((group) => group.name === name)?.id ?? "";

/** Deletes the cameras and groups named, those that are there, through the bridge: a test's own, made for it. */
async function deleteNamed(cameras: string[], groups: string[]) {
  for (const name of groups) if (groupId(name) !== "") await Groups.remove(groupId(name));
  for (const name of cameras) if (cameraId(name) !== "") await Cameras.delete(cameraId(name));
}

/** Answers the next delete question with its destructive button, whatever it is called. */
function agreeToDelete(app: App) {
  app.onDialog((dialog) => dialog.buttons.find((button) => button.startsWith("Delete")) ?? null);
}

test("the Default view is 2 × 2 with the camera in its first tile, which says what it is doing and where, and the header follows the selected tile", async () => {
  const app = await start();
  expect(app.texts()).toContain("Default");
  expect(app.count("label:Tile 4")).toBe(1);
  expect(app.count("label:Tile 5")).toBe(0);
  expect(app.count("label:Add a camera to tile 1")).toBe(0);
  expect(app.count("label:Add a camera to tile 2")).toBe(1);

  expect(app.texts()).toContain("Camera 1");
  // Over its picture, why it isn't live and the way forward.
  expect(app.texts()).toContain("Not connected");
  expect(app.texts()).toContain("Add the camera's password in Settings to connect.");
  // Its address, under its name on its tile.
  expect(app.texts()).toContain("192.0.2.10");
  // The status bar sums up the cameras.
  expect(app.texts()).toContain("0 of 1 live");
  expect(app.texts()).toContain("0 events today");

  // An empty tile has no camera to hear, but snapshot and record stay in reach: they take every camera on screen,
  // the one in tile 1 included.
  await app.press("label:Tile 2");
  await app.waitFor("label:Volume", { state: "gone" });
  for (const action of ["label:Snapshot", "label:Record"]) expect(app.count(action)).toBe(1);
  await app.press("label:Tile 1");
  await app.waitFor("label:Volume");
});

/** A camera's settings dialog, or the one for several cameras, by the first part down its side. */
const EDITOR = "id:section:connection";

/** Opens the settings the way a new user reaches them: from the video. */
async function openSettings(app: App) {
  await app.press("text:Camera settings");
  await app.waitFor(EDITOR);
}

const closed = { state: "gone" } as const;

test("a camera on its own has what a view's tile has over its picture: its name, and its menu", async () => {
  const app = await start();
  await app.press("label:Camera 1");
  // Its toolbar shows while the pointer is over the picture, More at its end, as a tile's does.
  await app.waitFor("label:Live picture of Camera 1");
  expect(app.count("label:Camera 1's picture options")).toBe(0);
  await app.hover("label:Live picture of Camera 1");
  await app.waitFor("label:Camera 1's picture options");
  // Its menu, as a tile's More opens the tile's: the camera's settings from the picture.
  await app.press("label:Camera 1's picture options");
  await app.chooseMenuItem("Edit camera…");
  await app.waitFor(EDITOR);
  expect(app.value("label:Name")).toBe("Camera 1");
  await app.press("text:Cancel");
  await app.waitFor(EDITOR, closed);

  // Its stream, as a tile's, from its menu or its toolbar's picker: on the sub stream, it says so, then on its default again.
  await app.hover("label:Live picture of Camera 1");
  await app.press("label:Camera 1's picture options");
  await app.chooseMenuItem("Stream > Sub stream");
  await app.waitFor("text:Sub stream");
  await app.hover("label:Live picture of Camera 1");
  await app.press("label:Camera 1's stream");
  await app.chooseMenuItem("Main stream (default)");
  await app.waitFor("text:Sub stream", closed);
});

test("the settings show every field, a part at a time, chosen down the dialog's side; a Dahua's PTZ and light go over its HTTP API with its switch on, and a Reolink brings its own path, ports and API", async () => {
  const app = await start();
  await openSettings(app);

  const parts: [string, string[]][] = [
    ["connection", ["Name", "Brand", "Camera address / hostname", "Username", "Password"]],
    ["stream", ["RTSP port", "Channel", "Stream quality", "RTSP path override", "Force RTSP over TCP (recommended)", "Reconnect automatically"]],
    ["ptz", ["ONVIF port", "DVRIP port", "ONVIF device path"]],
    [
      "features",
      [
        "PTZ goes over DVRIP, or over its HTTP API with the switch below, which switches its light too: Amcrest's light bulb cameras have no DVRIP. Motion and what the camera tells apart can come from its ONVIF events. All with this login.",
        "PTZ and its light over its HTTP API, not DVRIP",
        "Watch it over ONVIF: motion, and its light where it has one",
      ],
    ],
  ];
  for (const [part, fields] of parts) {
    await app.press(`id:section:${part}`);
    await app.waitFor(`text:${fields[0]}`);
    for (const field of fields) expect(app.texts()).toContain(field);
    // The other parts' fields wait for their own.
    for (const [other, theirs] of parts) if (other !== part) expect(app.texts()).not.toContain(theirs[theirs.length - 1]);
  }

  // A camera's events are on until its Events part turns them off, which takes its event videos away.
  const logs = "label:Log what it sees on the Events page, with alerts and what each event saves";
  const ownVideos = "label:Its own event video settings, in place of Settings → Automation's";
  await app.press("id:section:events");
  await app.waitFor(logs);
  expect(app.count(ownVideos)).toBe(1);
  await app.press(logs);
  await app.waitFor(ownVideos, { state: "gone" });
  await app.press(logs);
  await app.waitFor(ownVideos);

  // Each opening starts on the first part.
  await app.press("text:Cancel");
  await app.waitFor(EDITOR, closed);
  await openSettings(app);
  expect(app.texts()).toContain("Camera address / hostname");
  await app.press("text:Cancel");
  await app.waitFor(EDITOR, closed);

  await openSettings(app);
  await app.press("id:section:features");
  await app.waitFor("text:PTZ and its light over its HTTP API, not DVRIP");
  expect(app.texts()).not.toContain("API port");

  await app.press("label:PTZ and its light over its HTTP API, not DVRIP");
  await app.waitFor("text:API port");
  expect(app.value("label:API port")).toBe("80");
  expect(app.texts()).toContain("The API is HTTPS");

  await app.press("id:section:ptz");
  await app.waitFor("text:PTZ goes over its HTTP API, on the port Features names.");
  expect(app.texts()).not.toContain("DVRIP port");
  await app.press("text:Cancel");
  await app.waitFor(EDITOR, closed);

  await openSettings(app);
  await app.press("id:section:stream");
  expect(app.texts()).toContain("Blank uses /cam/realmonitor?channel=1&subtype=0 for channel 1's main stream");

  await app.press("id:section:connection");
  await app.press("label:Brand");
  await app.press("text:Reolink");
  await app.press("id:section:stream");
  await app.waitFor("text:Blank uses /Preview_01_main for channel 1's main stream");

  await app.press("id:section:ptz");
  await app.waitFor("text:PTZ goes over Reolink's API, on the port Features names.");
  expect(app.texts()).not.toContain("DVRIP port");
  expect(app.value("label:ONVIF port")).toBe("8000");

  await app.press("id:section:features");
  await app.waitFor("text:API port");
  expect(app.value("label:API port")).toBe("443");
  expect(app.texts()).toContain("The API is HTTPS (Reolink's default)");
  expect(app.texts()).not.toContain("Watch it over ONVIF: motion, and its light where it has one");
  await app.press("text:Cancel");
  await app.waitFor(EDITOR, closed);
});

test("Connection reads name, address, login, brand, then what the camera said; Connect asks a saved camera again, leaving Save on, and the search opens the cameras on the network over the form", async () => {
  const app = await start();
  await openSettings(app);
  const order = ["Name", "Camera address / hostname", "Username", "Password", "Brand", "Connect", "Cancel", "Save"].map((text) => app.texts().lastIndexOf(text));
  expect(order.every((at) => at >= 0)).toBe(true);
  expect([...order].sort((a, b) => a - b)).toEqual(order);
  // Asking the camera is the footer's Connect, and nothing else's.
  expect(app.texts()).not.toContain("Detect");
  expect(app.count("label:Copy")).toBe(0);

  // A saved camera was reached when it was added: another login closes nothing and leaves Save on.
  await app.type("label:Username", "2");
  await app.waitFor("label:Connect to the camera");
  expect(app.enabled("text:Save")).toBe(true);
  for (const part of ["stream", "ptz", "features"]) expect(app.enabled(`id:section:${part}`)).toBe(true);
  await app.press("label:Connect to the camera");
  await app.waitFor("text:This run of the app reaches no camera, so there is nothing to connect to.");
  // What the camera said comes after the brand, and can be copied.
  expect(app.count("label:Copy")).toBe(1);
  expect(app.texts().indexOf("This run of the app reaches no camera, so there is nothing to connect to.")).toBeGreaterThan(app.texts().indexOf("Brand"));
  expect(app.enabled("text:Save")).toBe(true);

  // The search in the form's header opens over the form, searching at once, and closing it leaves the form as it was.
  await app.press("text:Search network for devices");
  await app.waitFor("text:Cameras on the network");
  await app.waitFor("text:This run of the app reaches no camera, so there is nothing to find.");
  expect(app.texts()).toContain("Search again");
  await app.press("text:Close");
  await app.waitFor("text:Cameras on the network", closed);
  expect(app.texts()).toContain("Camera address / hostname");
  await app.press("text:Cancel");
  await app.waitFor(EDITOR, closed);
});

test("Cancel discards the edits, and so does Escape, after closing a list that is open", async () => {
  const app = await start();
  await openSettings(app);
  await app.clear("label:Name");
  await app.type("label:Name", "Driveway");
  await app.press("text:Cancel");
  await app.waitFor(EDITOR, closed);

  await openSettings(app);
  expect(app.value("label:Name")).toBe("Camera 1");
  await app.press("text:Cancel");
  await app.waitFor(EDITOR, closed);

  await openSettings(app);
  await app.clear("label:Name");
  await app.type("label:Name", "Driveway");

  // A list open in the dialog takes the first Escape.
  await app.press("id:section:stream");
  await app.press("label:Stream quality");
  await app.waitFor("text:Sub stream (lighter, lower latency)");
  await app.keyboard.press("escape");
  await app.waitFor("text:Sub stream (lighter, lower latency)", closed);
  expect(app.count(EDITOR)).toBe(1);

  // From a text field too.
  await app.press("id:section:connection");
  await app.type("label:Name", " gate");
  await app.keyboard.press("escape");
  await app.waitFor(EDITOR, closed);

  await openSettings(app);
  expect(app.value("label:Name")).toBe("Camera 1");
  await app.press("text:Cancel");
});

test("a bad value explains itself, and stops Save until it is fixed", async () => {
  const app = await start();
  await openSettings(app);
  expect(app.enabled("text:Save")).toBe(true);

  await app.press("id:section:stream");
  await app.clear("label:RTSP port");
  await app.type("label:RTSP port", "70000");
  await app.waitFor("text:Use a port from 1 to 65535.");
  expect(app.enabled("text:Save")).toBe(false);
  // Its part is marked down the side, to be found from the others.
  expect(app.count("label:Stream, has a problem")).toBe(1);

  await app.clear("label:RTSP port");
  await app.waitFor("text:Use a port from 1 to 65535.", closed);
  expect(app.enabled("text:Save")).toBe(true);
  expect(app.count("label:Stream, has a problem")).toBe(0);

  await app.press("id:section:connection");
  await app.clear("label:Name");
  await app.waitFor("text:Give the camera a name.");
  expect(app.enabled("text:Save")).toBe(false);

  // Return in another part shows the one with something to fix.
  await app.press("id:section:stream");
  await app.waitFor("text:Give the camera a name.", closed);
  await app.submit("label:RTSP port");
  await app.waitFor("text:Give the camera a name.");
  expect(app.count("label:Connection, has a problem")).toBe(1);

  await app.press("text:Cancel");
});

test("Save keeps the settings, and Return does the same", async () => {
  const app = await start();
  try {
    await openSettings(app);
    await app.clear("label:Name");
    await app.type("label:Name", "Driveway");
    await app.press("text:Save");
    await app.waitFor(EDITOR, closed);
    await app.waitFor("text:Driveway");
  } finally {
    // The settings outlive the test, and the others expect the default name.
    await openSettings(app);
    await app.clear("label:Name");
    await app.type("label:Name", "Camera 1");
    await app.submit("label:Name");
    await app.waitFor(EDITOR, closed);
  }
  expect(app.texts()).toContain("Camera 1");
});

test("a tile's toolbar and its menu connect its camera, which in a test reaches none; the header leaves it to them", async () => {
  const app = await start();
  await app.press("label:Tile 1");
  // The header has none, and the camera has no password, so its picture offers its settings instead.
  expect(app.count("text:Connect")).toBe(0);
  await app.hover("label:Tile 1");
  await app.press("label:Connect Camera 1");
  await app.waitFor("text:this run has no camera");
  await app.press("label:Tile 1 options");
  await app.chooseMenuItem("Connect");
});

test("a snapshot or a recording with no stream explains itself in a message", async () => {
  const app = await start();
  await app.press("label:Snapshot");
  await app.waitFor("text:Connect first, then take a snapshot.");
  await app.press("label:Record");
  await app.waitFor("text:Connect first, then record.");
  expect(app.count("label:Stop recording")).toBe(0);
});

test("the mute button follows the camera's audio", async () => {
  const app = await start();
  const muted = app.count("label:Unmute") > 0;
  await app.press(muted ? "label:Unmute" : "label:Mute");
  await app.waitFor(muted ? "label:Mute" : "label:Unmute");
  await app.press(muted ? "label:Mute" : "label:Unmute");
  await app.waitFor(muted ? "label:Unmute" : "label:Mute");
});

test("the PTZ panel opens from the header, comes back as it was left, and collapses from itself", async () => {
  const first = await start();
  await closePtz(first);
  expect(first.texts()).not.toContain("Zoom");

  await first.press("label:Show PTZ");
  await first.waitFor("text:Zoom");
  expect(first.texts()).toContain("PTZ: not checked yet");
  expect(first.texts()).toContain("0.50");

  // The next window opens with it shown, as it was left.
  const second = await start();
  await second.waitFor("text:Zoom");
  await second.press("label:Collapse PTZ panel");
  await second.waitFor("text:Zoom", { state: "gone" });
});

test("the sidebar hides, by its button and its shortcut, and comes back; a narrow window gives its room to the video and keeps every action", async () => {
  const app = await start();
  expect(app.texts()).toContain("IP Camera");

  await app.press("label:Hide sidebar");
  await app.waitFor("text:IP Camera", { state: "gone" });
  await app.press("label:Show sidebar");
  await app.waitFor("text:IP Camera");

  await app.keyboard.press("mod+b");
  await app.waitFor("text:IP Camera", { state: "gone" });
  await app.keyboard.press("mod+b");
  await app.waitFor("text:IP Camera");

  await app.setViewport(500, 700);
  await app.waitFor("text:IP Camera", { state: "gone" });
  for (const action of ["label:Layout", "label:Snapshot", "label:Record", "label:Fullscreen", "label:Show sidebar"]) {
    expect(app.count(action)).toBe(1);
  }
});

/** Settings' dialog, by its first section down its side. */
const SETTINGS = "id:section:general";

/** Opens Settings from the sidebar, on the section `section`: the dialog opens on the one it was left on. */
async function openAppSettings(app: App, section: "general" | "automation" | "appearance") {
  await app.press("label:Settings");
  await app.waitFor(SETTINGS);
  await app.press(`id:section:${section}`);
}

test("Settings opens in a dialog over the page on where media is saved, and its Automation section has events, on until turned off, their alerts, what each saves and how long it stays open, and the storage limits", async () => {
  const app = await start();
  await openAppSettings(app, "general");
  await app.waitFor("text:Storage");

  // Its sections down its side, Automation on the desktop.
  for (const section of ["general", "automation", "appearance"]) expect(app.count(`id:section:${section}`)).toBe(1);
  for (const shown of ["text:Snapshots", "text:Videos", "label:Change where snapshots are saved", "label:Change where videos are saved", "label:Open the videos folder"]) {
    expect(app.count(shown)).toBeGreaterThan(0);
  }
  // A fresh install saves in the folders beside the app, so there is nothing to go back to.
  expect(app.texts().some((text) => /[\\/]data[\\/]snapshots$/.test(text))).toBe(true);
  expect(app.texts().some((text) => /[\\/]data[\\/]videos$/.test(text))).toBe(true);
  for (const kind of ["snapshots", "videos"]) expect(app.count(`label:Save ${kind} in the default folder`)).toBe(0);

  // Automation.
  await app.press("id:section:automation");
  await app.waitFor("text:Group within");
  for (const shown of ["label:Log events", "label:Show alerts", "label:What each event saves", "label:Group events within", "label:Limit for all media", "label:Largest video file"]) {
    expect(app.count(shown)).toBe(1);
  }
  // Making room means something only under a limit.
  expect(app.count("label:Delete the oldest to make room")).toBe(0);
  // Off and on again: settings outlive a test.
  await app.press("label:Show alerts");
  await app.press("label:Show alerts");
  // With no events logged there is nothing to tell, save or group.
  await app.press("label:Log events");
  await app.waitFor("label:Show alerts", { state: "gone" });
  expect(app.count("label:What each event saves")).toBe(0);
  expect(app.count("label:Group events within")).toBe(0);
  await app.press("label:Log events");
  await app.waitFor("label:What each event saves");
  // Saving videos shows how they are made, each kind a video is saved for named beside its box; back to a picture after.
  await app.press("label:What each event saves");
  await app.press("text:Picture and video");
  await app.waitFor("text:Save videos of");
  for (const kind of ["People", "Vehicles", "Animals", "Faces", "Packages"]) expect(app.count(`text:${kind}`)).toBe(1);
  await app.press("label:What each event saves");
  await app.press("text:A picture");
  await app.waitFor("text:Save videos of", { state: "gone" });

  await app.press("id:section:general");
  await app.waitFor("text:Storage");
  // Done closes it, and the view behind it is as it was.
  await app.press("text:Done");
  await app.waitFor(SETTINGS, closed);
  expect(app.count("text:Storage")).toBe(0);
  expect(app.count("label:Tile 4")).toBe(1);

  // So does Escape, and the close button.
  await openAppSettings(app, "general");
  await app.keyboard.press("escape");
  await app.waitFor(SETTINGS, closed);
  await openAppSettings(app, "general");
  await app.press("label:Close");
  await app.waitFor(SETTINGS, closed);
  expect(app.count("label:Tile 4")).toBe(1);
});

test("Events, Videos and Snapshots each open in the video's place, saying what fills them", async () => {
  const app = await start();
  await app.press("label:Events");
  // The test runner reaches no camera, so its log is empty.
  await app.waitFor("text:No events yet");
  for (const gone of ["label:Snapshot", "label:Tile 1"]) expect(app.count(gone)).toBe(0);

  await app.press("label:Default");
  await app.waitFor("label:Tile 4");

  await app.press("label:Videos");
  await app.waitFor("text:No videos yet");
  // Nothing records, so the live video's controls, recording's included, give way to the page,
  // and with nothing to filter there are no filters.
  for (const gone of ["label:Snapshot", "label:Record", "label:Show PTZ", "label:Hide PTZ", "label:Camera filter", "label:Tile 1"]) expect(app.count(gone)).toBe(0);
  expect(app.count("text:Open folder")).toBe(1);

  await app.press("label:Default");
  await app.waitFor("label:Tile 4");
  expect(app.count("text:No videos yet")).toBe(0);

  await app.press("label:Snapshots");
  await app.waitFor("text:No snapshots yet");
  // The live video's controls give way to the gallery's, and with nothing to filter there are no filters.
  for (const gone of ["label:Snapshot", "label:Show PTZ", "label:Hide PTZ", "label:Camera filter", "label:Tile 1"]) expect(app.count(gone)).toBe(0);

  await app.press("label:Default");
  await app.waitFor("label:Tile 4");
  expect(app.count("text:No snapshots yet")).toBe(0);
});

test("Appearance picks a dark theme and a light one, and the mode shows one or follows the system; the texture and elevation go over either", async () => {
  const app = await start();
  await openAppSettings(app, "appearance");
  /** One Default tile, whatever the mode, and nothing names Default after an appearance. */
  const oneDefault = () => {
    expect(app.count("label:Default theme")).toBe(1);
    for (const gone of ["Default Dark", "Default Light"]) expect(app.count(`label:${gone} theme`)).toBe(0);
    expect(app.texts().some((text) => /Default (Dark|Light)/.test(text))).toBe(false);
  };
  // A fresh install follows the system, in Default for both: every theme's tile, in view with nothing to open first.
  await app.waitFor("text:Default, dark or light as the system is.");
  oneDefault();
  for (const shown of ["Dark Modern", "Dark High Contrast", "Light Modern", "Light High Contrast"]) expect(app.count(`label:${shown} theme`)).toBe(1);
  // Every setting is in the one panel, by name.
  for (const name of ["Mode", "Texture", "Elevation", "Theme"]) expect(app.count(`text:${name}`)).toBe(1);
  /** Where `text` is among the texts under the Theme setting. */
  const under = (text: string) => {
    const texts = app.texts();
    return texts.slice(texts.indexOf("Theme")).indexOf(text);
  };

  // Dark shows the dark theme files alone, and a tile picks one, which the Theme setting names.
  await app.press("label:Dark");
  await app.waitFor("text:Always dark, in the dark theme picked below.");
  oneDefault();
  for (const shown of ["Dark Modern", "Monokai", "Dark High Contrast"]) expect(app.count(`label:${shown} theme`)).toBe(1);
  for (const hidden of ["Light Modern", "Light High Contrast"]) expect(app.count(`label:${hidden} theme`)).toBe(0);
  await app.press("label:Monokai theme");
  await app.waitFor("text:Monokai, the dark theme the app shows.");

  // Light shows the light themes, and its own pick; the dark pick waits for the dark.
  await app.press("label:Light");
  await app.waitFor("text:Default, the light theme the app shows.");
  expect(app.count("label:Monokai theme")).toBe(0);
  await app.press("label:Solarized Light theme");
  await app.waitFor("text:Solarized Light, the light theme the app shows.");

  // System names both picks and marks the one in use: one Default first, then the dark files under Dark and the light
  // ones under Light.
  await app.press("label:System");
  await app.waitFor("text:Monokai while the system is dark, Solarized Light while it is light.");
  oneDefault();
  expect(app.count("text:In use")).toBe(1);
  expect(under("Default")).toBeGreaterThan(0);
  expect(under("Default")).toBeLessThan(under("Dark"));
  expect(under("Dark High Contrast")).toBeLessThan(under("Light"));
  expect(under("Light")).toBeLessThan(under("Light Modern"));

  // Back in Dark, the dark pick is still Monokai: a mode never gives a pick up.
  await app.press("label:Dark");
  await app.waitFor("text:Monokai, the dark theme the app shows.");

  // The texture and the elevation go over whichever theme; a fresh install has neither.
  const plain = "text:Page headers in the theme's plain color.";
  const flat = "text:Flat: colors and lines alone set things apart.";
  const raised = "Shadows, with light catching the top edges of what is raised, and fields pressed in.";
  expect(app.count(plain)).toBe(1);
  expect(app.count(flat)).toBe(1);
  await app.press("label:Haze");
  await app.waitFor("text:A soft light falling on page headers from the top right.");
  await app.press("label:No texture");
  await app.waitFor(plain);
  await app.press("label:Raised");
  await app.waitFor(`text:${raised}`);

  // A high contrast theme takes no elevation, and says so.
  await app.press("label:Dark High Contrast theme");
  await app.waitFor(`text:${raised} High contrast themes stay flat: their outlines mark every state.`);
  // Default in Dark picks it for the dark alone: the light pick stays.
  await app.press("label:Default theme");
  await app.waitFor("text:Default, the dark theme the app shows.");
  await app.press("label:No elevation");
  await app.waitFor(flat);

  // Default in System picks it for both appearances, and the system then decides which shows: a fresh install's again.
  await app.press("label:System");
  await app.waitFor("text:Default while the system is dark, Solarized Light while it is light.");
  await app.press("label:Default theme");
  await app.waitFor("text:Default, dark or light as the system is.");
  oneDefault();

  // Settings opens again on the section it was left on, over whatever page shows.
  await app.press("text:Done");
  await app.waitFor(SETTINGS, closed);
  await app.press("label:Videos");
  await app.waitFor("text:No videos yet");
  await app.press("label:Settings");
  await app.waitFor("text:Default, dark or light as the system is.");
  await app.press("id:section:general");
  await app.waitFor("label:Change where snapshots are saved");
  await app.keyboard.press("escape");
  await app.waitFor(SETTINGS, closed);
  expect(app.count("text:No videos yet")).toBe(1);
});

test("a button explains itself when the pointer rests on it", async () => {
  const app = await start({ clock: {} });
  await app.hover("label:Fullscreen");
  await app.wait(700);
  expect(app.texts()).toContain("Fullscreen");

  await app.hover("label:Snapshot");
  await app.wait(700);
  expect(app.texts()).toContain("Snapshot");
  expect(app.texts()).not.toContain("Fullscreen");
});

test("a view is made, divided, filled, renamed and deleted", async () => {
  const app = await start();
  await app.press("label:New view");
  await app.waitFor("text:View 2");
  await app.waitFor("label:Add a camera to tile 4");

  try {
    await app.press("label:Layout");
    await app.waitFor("text:3 × 3");
    await app.press("text:3 × 3");
    await app.waitFor("label:Tile 9");

    await app.press("label:Add a camera to tile 5");
    await app.chooseMenuItem("Camera 1");
    await app.waitFor("label:Add a camera to tile 5", { state: "gone" });

    await app.hover("label:Tile 5");
    await app.press("label:Tile 5 options");
    await app.chooseMenuItem("Empty tile");
    await app.waitFor("label:Add a camera to tile 5");

    await app.press("label:View 2 options");
    await app.chooseMenuItem("Rename view…");
    await app.waitFor("text:Rename view");
    await app.clear("label:Name");
    await app.type("label:Name", "Yard");
    await app.press("text:Rename");
    await app.waitFor("text:Yard");
  } finally {
    agreeToDelete(app);
    const name = app.count("label:Yard options") > 0 ? "Yard" : "View 2";
    await app.press(`label:${name} options`);
    await app.chooseMenuItem("Delete view…");
    await app.waitFor(`text:${name}`, closed);
  }
  // The stage goes back to the first view.
  await app.waitFor("label:Tile 4");
});

test("a view's layout is edited tile by tile: a tile removed and another added, saved or dropped", async () => {
  const app = await start();
  await app.press("label:New view");
  await app.waitFor("text:View 2");
  await app.waitFor("label:Add a camera to tile 4");

  try {
    await app.press("label:Layout");
    await app.press("text:Edit layout…");
    await app.waitFor("label:Editing the layout of View 2");
    // Every frame resizes by each of its corners and edges.
    for (const handle of ["top left corner", "top edge", "top right corner", "right edge", "bottom right corner", "bottom edge", "bottom left corner", "left edge"]) {
      expect(app.count(`label:Resize tile 1 from its ${handle}`)).toBe(1);
    }
    // A 2 × 2 view fills the grid: there is no room for another tile until one goes.
    await app.press("label:Remove tile 4");
    await app.waitFor("label:Remove tile 4", { state: "gone" });
    await app.press("text:Done");
    await app.waitFor("label:Tile 3");
    expect(app.count("label:Tile 4")).toBe(0);

    // Added, then dropped with Cancel: the view keeps its three.
    await app.press("label:Layout");
    await app.press("text:Edit layout…");
    await app.press("text:Add tile");
    await app.waitFor("label:Remove tile 4");
    await app.press("text:Cancel");
    await app.waitFor("label:Tile 3");
    expect(app.count("label:Tile 4")).toBe(0);

    // A layout puts it back as it was.
    await app.press("label:Layout");
    await app.press("text:2 × 2");
    await app.waitFor("label:Tile 4");
  } finally {
    agreeToDelete(app);
    await app.press("label:View 2 options");
    await app.chooseMenuItem("Delete view…");
    await app.waitFor("text:View 2", closed);
  }
  await app.waitFor("label:Tile 4");
});

test("with a mouse, a tile's toolbar shows while the pointer is over it, and switches its stream", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1100, height: 760 });
  await app.press("label:Default");
  await app.waitFor("label:Tile 4");
  expect(app.count("label:Tile 1 options")).toBe(0);

  await app.hover("label:Tile 1");
  await app.waitFor("label:Tile 1 options");
  expect(app.count("label:Snapshot of Camera 1")).toBe(1);
  expect(app.count("label:Open Camera 1 on its own")).toBe(1);
  // A tile starts on Auto, which plays the camera's own: it lists no sizes to pick by.
  expect(app.texts()).toContain("Auto · Main stream");

  await app.press("label:Camera 1's stream");
  await app.chooseMenuItem("Sub stream");
  await app.waitFor("text:Sub stream");
  await app.press("label:Camera 1's stream");
  await app.chooseMenuItem("Auto");
  await app.waitFor("text:Sub stream", closed);
  await app.waitFor("text:Auto · Main stream");

  // Fit picks how the tile's picture fills it, and says which; the tile's menu has it too.
  await app.press("label:Camera 1's fit: Original");
  await app.chooseMenuItem("Fill");
  await app.waitFor("label:Camera 1's fit: Fill");
  await app.press("label:Tile 1 options");
  await app.chooseMenuItem("Fit > Original");
  await app.waitFor("label:Camera 1's fit: Original");

  // Tile 2 is empty: it has no toolbar, and tile 1's goes with the pointer.
  await app.hover("label:Tile 2");
  await app.waitFor("label:Tile 1 options", closed);
  expect(app.count("label:Tile 2 options")).toBe(0);
});

test("a tile on Auto plays the stream its size calls for, and follows the window as it is resized", async () => {
  // A camera as Detect saves one that sends a 4K main stream and an 896×512 sub stream.
  const listed = (name: string, path: string, detail: string) => ({ name, path, detail, title: "", lens: "" });
  const yard = await Cameras.add({
    ...Cameras.withBrand({ ...Cameras.blankForm(), name: "Yard", host: "10.0.0.64", user: "admin" }, "reolink"),
    streams: [listed("main", "/Preview_01_main", "3840×2160, H265"), listed("sub", "/Preview_01_sub", "896×512, H264")],
  });
  try {
    const app = await start();
    await app.press("label:Add a camera to tile 2");
    await app.chooseMenuItem("Yard");
    await app.waitFor("label:Add a camera to tile 2", closed);

    // A quarter of a small window: the sub stream fills it.
    await app.hover("label:Tile 2");
    await app.waitFor("text:Auto · 896×512");
    // A large window: the main stream, once the size has held.
    await app.setViewport(3600, 2400);
    await app.hover("label:Tile 2");
    await app.waitFor("text:Auto · 3840×2160");
    // Small again: back to the sub stream.
    await app.setViewport(1100, 760);
    await app.hover("label:Tile 2");
    await app.waitFor("text:Auto · 896×512");
  } finally {
    await Cameras.delete(yard);
  }
});

test("in a browser, which has no menus of its own, the app draws them", async () => {
  const app = await open("./App.tsx", { platform: "web", width: 1100, height: 760 });
  await app.press("label:Default");
  await app.waitFor("label:Add a camera to tile 2");
  try {
    // A button's opens under it.
    await app.press("label:Add a camera to tile 2");
    await app.waitFor("text:New camera…");
    expect(app.menu()).toBe(null);
    await app.press("id:menu >> text:Camera 1");
    await app.waitFor("label:Add a camera to tile 2", closed);
    await app.waitFor("id:menu", closed);

    // A right click's opens where it was, a submenu beside it.
    await app.rightClick("label:Tile 2");
    await app.press("id:menu >> text:Show camera");
    await app.waitFor("text:New camera…");
    await app.press("id:menu >> text:Empty tile");
    await app.waitFor("label:Add a camera to tile 2");
  } finally {
    if (app.count("label:Default") > 0 && app.count("label:Add a camera to tile 2") === 0) {
      await app.press("label:Default");
      // A tile's More is on its toolbar, which shows while the pointer is over it.
      await app.hover("label:Tile 2");
      await app.press("label:Tile 2 options");
      await app.press("id:menu >> text:Empty tile");
    }
  }
});

test("a new camera is entered once and can be in two groups", async () => {
  const app = await start();
  await app.press("label:Add camera");
  // A new camera is asked what it is first, which needs its address, and is added only once it has answered.
  await app.waitFor("label:Connect to the camera");
  expect(app.enabled("label:Connect to the camera")).toBe(false);
  expect(app.enabled("id:section:stream")).toBe(false);
  expect(app.enabled("text:Add")).toBe(false);
  await app.type("label:Name", "Porch");
  await app.type("label:Camera address / hostname", "10.0.0.7");
  expect(app.enabled("text:Add")).toBe(false);
  await app.press("label:Connect to the camera");
  await app.waitFor("text:This run of the app reaches no camera, so there is nothing to connect to.");
  expect(app.enabled("id:section:stream")).toBe(true);
  expect(app.enabled("text:Add")).toBe(true);
  await app.press("text:Add");
  await app.waitFor(EDITOR, closed);
  await app.waitFor("label:Porch");

  try {
    // A group from the sidebar's button, and another made for the test.
    await app.press("label:New group");
    await app.waitFor("text:Create");
    await app.type("label:Name", "Outside");
    await app.press("text:Create");
    await app.waitFor("text:Outside");
    await Groups.add("Garage door", []);
    await app.waitFor("text:Garage door");
    for (const group of ["Outside", "Garage door"]) {
      await app.press("label:Porch options >> nth=0");
      await app.chooseMenuItem(`Groups > ${group}`);
    }
    // All cameras, Outside and Garage door each list it, and it is one camera.
    await app.waitUntil(() => app.count("label:Porch") === 3);

    await app.press("label:Porch options >> nth=0");
    await app.chooseMenuItem("Groups > Outside");
    await app.waitUntil(() => app.count("label:Porch") === 2);
  } finally {
    await deleteNamed([], ["Outside", "Garage door"]);
    // The camera from its menu, as a user deletes one.
    agreeToDelete(app);
    await app.press("label:Porch options >> nth=0");
    await app.chooseMenuItem("Delete camera…");
    await app.waitFor("label:Porch", closed);
  }
  // Deleting the camera on the stage goes back to the first view.
  await app.waitFor("label:Tile 4");
});

test("Dashboard, above the views, sums up the cameras and their files in the video's place", async () => {
  const app = await start();
  await app.press("label:Dashboard");
  await app.waitFor("id:dashboard");
  await app.waitFor("text:Saved per day");

  for (const text of ["Cameras", "Videos", "Snapshots", "On disk", "Storage", "Space by camera", "Events by hour", "Busiest hours", "Latest events"]) {
    expect(app.texts()).toContain(text);
  }
  // Nothing saved or seen yet: each chart says what will fill it.
  expect(app.texts()).toContain("No videos or snapshots yet.");
  expect(app.texts()).toContain("Nothing in the last 14 days");
  expect(app.texts()).toContain("No events in the last 14 days");
  expect(app.texts()).toContain("Events show here as the cameras see them.");
  expect(app.texts()).toContain("0 events today");
  // The live video's controls give way to the dashboard.
  for (const gone of ["label:Snapshot", "label:Show PTZ", "label:Hide PTZ", "label:Tile 1"]) expect(app.count(gone)).toBe(0);

  // The period in the header's bar.
  await app.press("text:7 days");
  await app.waitFor("text:Nothing in the last 7 days");
  await app.press("text:14 days");

  // What is on disk opens Settings over the dashboard, on where the folders are.
  await app.press("id:tile:disk");
  await app.waitFor("label:Change where snapshots are saved");
  await app.press("text:Done");
  await app.waitFor(SETTINGS, closed);
  expect(app.count("id:dashboard")).toBe(1);

  // A number opens the page it sums up. (The Cameras number's page is checked with that page.)
  await app.press("id:tile:videos");
  await app.waitFor("text:No videos yet");

  // The latest events' button opens all of them.
  await app.press("label:Dashboard");
  await app.waitFor("text:Latest events");
  await app.press("text:Show all");
  await app.waitFor("text:No events yet");
});

/** Opens the Cameras page from the sidebar. */
async function openCameras(app: App) {
  await app.press("label:Cameras");
  await app.waitFor("id:cameras");
}

test("Cameras, beside the videos, lists every camera in a table, and the dashboard's cameras open it", async () => {
  const app = await start();
  await openCameras(app);
  // A window this wide leaves out the groups' column; the rest show.
  for (const text of ["1 camera", "Name", "Address", "Stream", "Login", "Camera 1", "192.0.2.10:554", "Ch 1  ·  Main  ·  TCP", "No password"]) {
    expect(app.texts()).toContain(text);
  }
  // The live video's controls give way to the table.
  for (const gone of ["label:Snapshot", "label:Show PTZ", "label:Hide PTZ", "label:Tile 1"]) expect(app.count(gone)).toBe(0);
  // The footer under the rows: which rows show of how many, and the pager, with nowhere to turn.
  for (const text of ["1–1 of 1 camera", "Rows per page", "Page 1 of 1"]) expect(app.texts()).toContain(text);
  for (const button of ["label:First page", "label:Previous page", "label:Next page", "label:Last page"]) expect(app.enabled(button)).toBe(false);

  // The dashboard's cameras open it too.
  await app.press("label:Dashboard");
  await app.waitFor("id:dashboard");
  await app.press("id:tile:cameras");
  await app.waitFor("id:cameras");
});

test("several cameras are selected as in Explorer, changed at once keeping their own names, grouped, and deleted together", async () => {
  const app = await start();
  await openCameras(app);
  try {
    await Cameras.add({ ...Cameras.blankForm(), name: "Yard", host: "10.0.0.8" });
    await app.waitFor("text:10.0.0.8:554");
    await app.press("text:Add camera");
    await app.waitFor(EDITOR);
    await app.type("label:Name", "Porch");
    await app.type("label:Camera address / hostname", "10.0.0.7");
    // A new camera is asked what it is before it is added.
    await app.press("label:Connect to the camera");
    await app.waitFor("text:This run of the app reaches no camera, so there is nothing to connect to.");
    await app.press("text:Add");
    await app.waitFor(EDITOR, closed);
    // Added here, a camera stays on the page, selected.
    await app.waitFor("text:10.0.0.7:554");
    await app.waitFor("text:1 selected");

    // Control and A selects them all, and Control with a press leaves one out.
    await app.keyboard.press("mod+a");
    await app.waitFor("text:3 selected");
    await app.keyboard.down("ctrl");
    await app.press("id:row:Camera 1");
    await app.keyboard.up("ctrl");
    await app.waitFor("text:2 selected");

    await app.press("label:Edit 2 cameras");
    await app.waitFor("text:Apply to 2");
    // Their addresses differ, so each keeps its own; their username is the same, and shows.
    expect(app.texts()).toContain("They differ: each keeps its own.");
    expect(app.value("label:Username")).toBe("admin");
    expect(app.enabled("label:Apply the changes to 2 cameras")).toBe(false);
    // The camera's own parts, less what each camera has.
    expect(app.count("id:section:features")).toBe(0);
    await app.press("id:section:stream");
    await app.clear("label:RTSP port");
    await app.type("label:RTSP port", "8554");
    await app.press("label:Apply the changes to 2 cameras");
    await app.waitFor("text:Apply to 2", closed);
    await app.waitFor("text:10.0.0.7:8554");
    expect(app.texts()).toContain("10.0.0.8:8554");
    // The camera left out is as it was, and the names are each camera's own.
    expect(app.texts()).toContain("192.0.2.10:554");
    for (const name of ["Porch", "Yard"]) expect(app.texts()).toContain(name);

    // A group of the selection, which the sidebar lists them under.
    await app.press("label:Groups for 2 cameras");
    await app.chooseMenuItem("New group…");
    await app.waitFor("text:Create");
    await app.type("label:Name", "Outside");
    await app.press("text:Create");
    await app.waitFor("text:Outside");
    await app.waitUntil(() => app.count("label:Porch") === 2 && app.count("label:Yard") === 2);

    agreeToDelete(app);
    await app.press("label:Delete 2 cameras");
    await app.waitFor("id:row:Porch", closed);
    expect(app.count("id:row:Yard")).toBe(0);
    expect(app.count("id:row:Camera 1")).toBe(1);
  } finally {
    await deleteNamed(["Porch", "Yard"], ["Outside"]);
  }
});

test("Duplicate copies a camera onto the next channel, in its groups, by its menu or Control and D", async () => {
  const app = await start();
  try {
    await Groups.add("Recorder", [cameraId("Camera 1")]);
    await openCameras(app);
    await app.press("id:row:Camera 1");
    await app.keyboard.press("mod+d");
    await app.waitFor("id:row:Camera 1 copy");
    // The copy is selected, on the next channel, and in the original's group as well as under All cameras.
    expect(app.texts()).toContain("1 selected");
    expect(app.texts()).toContain("Ch 2  ·  Main  ·  TCP");
    await app.waitUntil(() => app.count("label:Camera 1 copy") === 2);

    await app.press("id:cameras >> label:Camera 1 options");
    await app.chooseMenuItem("Duplicate");
    await app.waitFor("id:row:Camera 1 copy 2");
    expect(app.texts()).toContain("Ch 3  ·  Main  ·  TCP");
  } finally {
    await deleteNamed(["Camera 1 copy", "Camera 1 copy 2"], ["Recorder"]);
  }
});

/** Opens the Groups page from the sidebar. */
async function openGroups(app: App) {
  await app.press("label:Groups");
  await app.waitFor("id:groups");
}

test("Groups, beside the cameras, makes a group of cameras in its dialog, and edits its name and cameras", async () => {
  const app = await start();
  await openGroups(app);
  try {
    await app.waitFor("text:No groups yet");
    // The live video's controls give way to the page.
    for (const gone of ["label:Snapshot", "label:Show PTZ", "label:Hide PTZ", "label:Tile 1"]) expect(app.count(gone)).toBe(0);

    await app.press("text:New group");
    await app.waitFor("text:In this group");
    expect(app.enabled("text:Create")).toBe(false);
    await app.type("label:Name", "Outside");
    await app.press("label:Put Camera 1 in");
    await app.waitFor("label:Take Camera 1 out");
    // The other cameras' list, beside the group's, is left empty.
    expect(app.texts()).toContain("Every camera is in this group.");
    await app.press("text:Create");
    await app.waitFor("text:In this group", closed);

    // Made here, it is in the table, selected, and the sidebar lists its camera under it.
    await app.waitFor("id:group:Outside");
    expect(app.count("label:Outside, 1 camera")).toBe(1);
    for (const text of ["1 group", "Name", "Cameras", "Members", "Status", "1 selected", "1–1 of 1 group"]) expect(app.texts()).toContain(text);
    await app.waitUntil(() => app.count("label:Camera 1") === 2);

    await app.press("label:Edit Outside");
    await app.waitFor("text:Edit group");
    expect(app.value("label:Name")).toBe("Outside");
    await app.clear("label:Name");
    await app.type("label:Name", "Yard");
    await app.press("label:Take Camera 1 out");
    await app.waitFor("label:Put Camera 1 in");
    await app.press("text:Save");
    await app.waitFor("text:Edit group", closed);
    await app.waitFor("label:Yard, 0 cameras");
    expect(app.texts()).toContain("No cameras");
    await app.waitUntil(() => app.count("label:Camera 1") === 1);

  } finally {
    await deleteNamed([], ["Outside", "Yard"]);
  }
  await app.waitFor("text:No groups yet");
});

test("several groups are selected, duplicated, given a camera together, and deleted in one go", async () => {
  const app = await start();
  await openGroups(app);
  try {
    await Groups.add("Recorder", []);
    await app.waitFor("id:group:Recorder");
    await app.press("id:group:Recorder");
    await app.waitFor("text:1 selected");

    // Control and D duplicates the selection; the copy is selected.
    await app.keyboard.press("mod+d");
    await app.waitFor("id:group:Recorder copy");
    expect(app.texts()).toContain("1 selected");

    // Both get Camera 1 at once, from the selection's bar.
    await app.keyboard.press("mod+a");
    await app.waitFor("text:2 selected");
    await app.press("label:Cameras in 2 groups");
    await app.chooseMenuItem("Camera 1");
    await app.waitUntil(() => app.count("label:Camera 1") === 3);
    for (const name of ["Recorder", "Recorder copy"]) expect(app.count(`label:${name}, 1 camera`)).toBe(1);

    agreeToDelete(app);
    await app.press("label:Delete 2 groups");
    await app.waitFor("text:No groups yet");
    // Their cameras stay.
    expect(app.count("label:Camera 1")).toBe(1);
  } finally {
    await deleteNamed([], ["Recorder", "Recorder copy"]);
  }
});

test("a group's menu shows its cameras on the Cameras page, and moves it among the groups", async () => {
  const app = await start();
  try {
    for (const group of ["First", "Second"]) await Groups.add(group, []);
    await openGroups(app);
    await app.waitFor("id:group:Second");
    const order = () => app.texts().filter((text) => text === "First" || text === "Second");

    await app.press("label:Second options >> nth=1");
    await app.chooseMenuItem("Move up");
    await app.waitUntil(() => order().join(" ").startsWith("Second First"));

    await app.press("label:First options >> nth=1");
    await app.chooseMenuItem("Show on the Cameras page");
    await app.waitFor("id:cameras");
    // Camera 1 is in neither group.
    await app.waitFor("text:No cameras match the search and filters.");
    await app.press("text:Clear filters >> nth=0");
    await app.waitFor("text:192.0.2.10:554");
  } finally {
    await deleteNamed([], ["First", "Second"]);
  }
});

test("the app opens on the page it closed on", async () => {
  const first = await start();
  await first.press("label:Dashboard");
  await first.waitFor("id:dashboard");
  // The page is saved off the main thread.
  await first.waitUntil(() => Preferences.lastPage() === "dashboard");

  const second = await launch();
  await second.waitFor("id:dashboard");
  await second.press("label:Snapshots");
  await second.waitFor("text:No snapshots yet");
  await second.waitUntil(() => Preferences.lastPage() === "snapshots");

  const third = await launch();
  await third.waitFor("text:No snapshots yet");
});
