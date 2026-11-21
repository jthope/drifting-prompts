// The headless runner gets throwaway settings and reaches no camera.
import { expect, open, test } from "nsh/test";
import type { App } from "nsh/test";
import { Cameras, Groups, Preferences } from "ipcamera/native";

/** Without `platform`, a headless run is a touch screen and shows no tooltips. */
const launch = (options?: { clock: object }) => open("./App.tsx", { platform: "windows", width: 1100, height: 760, ...options });

/** Starts on the Default view: the app opens on the page it closed on, which a run's tests share. */
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

const cameraId = (name: string) => Cameras.list().find((camera) => camera.name === name)?.id ?? "";

const groupId = (name: string) => Groups.list().find((group) => group.name === name)?.id ?? "";

async function deleteNamed(cameras: string[], groups: string[]) {
  for (const name of groups) if (groupId(name) !== "") await Groups.remove(groupId(name));
  for (const name of cameras) if (cameraId(name) !== "") await Cameras.delete(cameraId(name));
}

function agreeToDelete(app: App) {
  app.onDialog((dialog) => dialog.buttons.find((button) => button.startsWith("Delete")) ?? null);
}

test("the Default view is 2 × 2 with the camera in its first tile, which says what it is doing and where, and the header follows the selected tile", async () => {
  // On a clock the test moves, so a second press can come after the double click's time.
  const app = await start({ clock: {} });
  expect(app.texts()).toContain("Default");
  expect(app.count("label:Tile 4")).toBe(1);
  expect(app.count("label:Tile 5")).toBe(0);
  expect(app.count("label:Add a camera to tile 1")).toBe(0);
  expect(app.count("label:Add a camera to tile 2")).toBe(1);

  expect(app.texts()).toContain("Camera 1");
  expect(app.texts()).toContain("Not connected");
  expect(app.texts()).toContain("Add the camera's password in Settings to connect.");
  expect(app.texts()).toContain("192.0.2.10");
  expect(app.texts()).toContain("0 of 1 live");
  expect(app.texts()).toContain("0 events today");

  expect(app.count("label:Volume")).toBe(0);
  await app.press("label:Tile 2");
  expect(app.count("label:Volume")).toBe(0);
  for (const action of ["label:Snapshot", "label:Record"]) expect(app.count(action)).toBe(1);
  await app.press("label:Tile 1");
  await app.waitFor("label:Volume");
  // Waits out the double click's time, which would fill the stage with the tile.
  await app.wait(500);
  await app.press("label:Tile 1");
  await app.waitFor("label:Volume", { state: "gone" });
});

const EDITOR = "id:section:connection";

async function openSettings(app: App) {
  await app.press("text:Camera settings");
  await app.waitFor(EDITOR);
}

const closed = { state: "gone" } as const;

test("a camera on its own has what a view's tile has over its picture: its name, and its menu", async () => {
  const app = await start();
  await app.press("label:Camera 1");
  await app.waitFor("label:Live picture of Camera 1");
  expect(app.count("label:Camera 1's picture options")).toBe(0);
  await app.hover("label:Live picture of Camera 1");
  await app.waitFor("label:Camera 1's picture options");
  await app.press("label:Camera 1's picture options");
  await app.chooseMenuItem("Edit camera…");
  await app.waitFor(EDITOR);
  expect(app.value("label:Name")).toBe("Camera 1");
  await app.press("text:Cancel");
  await app.waitFor(EDITOR, closed);

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
    for (const [other, theirs] of parts) if (other !== part) expect(app.texts()).not.toContain(theirs[theirs.length - 1]);
  }

  const logs = "label:Log what it sees on the Events page, with alerts and what each event saves";
  const ownVideos = "label:Its own event video settings, in place of Settings → Automation's";
  await app.press("id:section:events");
  await app.waitFor(logs);
  expect(app.count(ownVideos)).toBe(1);
  await app.press(logs);
  await app.waitFor(ownVideos, { state: "gone" });
  await app.press(logs);
  await app.waitFor(ownVideos);

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
  expect(app.texts()).not.toContain("Detect");
  expect(app.count("label:Copy")).toBe(0);

  await app.type("label:Username", "2");
  await app.waitFor("label:Connect to the camera");
  expect(app.enabled("text:Save")).toBe(true);
  for (const part of ["stream", "ptz", "features"]) expect(app.enabled(`id:section:${part}`)).toBe(true);
  await app.press("label:Connect to the camera");
  await app.waitFor("text:This run of the app reaches no camera, so there is nothing to connect to.");
  expect(app.count("label:Copy")).toBe(1);
  expect(app.texts().indexOf("This run of the app reaches no camera, so there is nothing to connect to.")).toBeGreaterThan(app.texts().indexOf("Brand"));
  expect(app.enabled("text:Save")).toBe(true);

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

  await app.press("id:section:stream");
  await app.press("label:Stream quality");
  await app.waitFor("text:Sub stream (lighter, lower latency)");
  await app.keyboard.press("escape");
  await app.waitFor("text:Sub stream (lighter, lower latency)", closed);
  expect(app.count(EDITOR)).toBe(1);

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
  expect(app.count("label:Stream, has a problem")).toBe(1);

  await app.clear("label:RTSP port");
  await app.waitFor("text:Use a port from 1 to 65535.", closed);
  expect(app.enabled("text:Save")).toBe(true);
  expect(app.count("label:Stream, has a problem")).toBe(0);

  await app.press("id:section:connection");
  await app.clear("label:Name");
  await app.waitFor("text:Give the camera a name.");
  expect(app.enabled("text:Save")).toBe(false);

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
  await app.press("label:Tile 1");
  await app.waitFor("label:Volume");
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

  await first.press("label:Tile 1");
  await first.press("label:Show PTZ");
  await first.waitFor("text:Zoom");
  expect(first.texts()).toContain("PTZ: not checked yet");
  expect(first.texts()).toContain("0.50");

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

const SETTINGS = "id:section:general";

async function openAppSettings(app: App, section: "general" | "automation" | "appearance") {
  await app.press("label:Settings");
  await app.waitFor(SETTINGS);
  await app.press(`id:section:${section}`);
}

test("Settings opens in a dialog over the page on where media is saved, and its Automation section has events, on until turned off, their alerts, what each saves and how long it stays open, and the storage limits", async () => {
  const app = await start();
  await openAppSettings(app, "general");
  await app.waitFor("text:Storage");

  for (const section of ["general", "automation", "appearance"]) expect(app.count(`id:section:${section}`)).toBe(1);
  for (const shown of ["text:Snapshots", "text:Videos", "label:Change where snapshots are saved", "label:Change where videos are saved", "label:Open the videos folder"]) {
    expect(app.count(shown)).toBeGreaterThan(0);
  }
  expect(app.texts().some((text) => /[\\/]data[\\/]snapshots$/.test(text))).toBe(true);
  expect(app.texts().some((text) => /[\\/]data[\\/]videos$/.test(text))).toBe(true);
  for (const kind of ["snapshots", "videos"]) expect(app.count(`label:Save ${kind} in the default folder`)).toBe(0);

  await app.press("id:section:automation");
  await app.waitFor("text:Group within");
  for (const shown of ["label:Log events", "label:Show alerts", "label:What each event saves", "label:Group events within", "label:Limit for all media", "label:Largest video file"]) {
    expect(app.count(shown)).toBe(1);
  }
  expect(app.count("label:Delete the oldest to make room")).toBe(0);
  // Off and on again: settings outlive a test.
  await app.press("label:Show alerts");
  await app.press("label:Show alerts");
  await app.press("label:Log events");
  await app.waitFor("label:Show alerts", { state: "gone" });
  expect(app.count("label:What each event saves")).toBe(0);
  expect(app.count("label:Group events within")).toBe(0);
  await app.press("label:Log events");
  await app.waitFor("label:What each event saves");
  await app.press("label:What each event saves");
  await app.press("text:Picture and video");
  await app.waitFor("text:Save videos of");
  for (const kind of ["People", "Vehicles", "Animals", "Faces", "Packages"]) expect(app.count(`text:${kind}`)).toBe(1);
  await app.press("label:What each event saves");
  await app.press("text:A picture");
  await app.waitFor("text:Save videos of", { state: "gone" });

  await app.press("id:section:general");
  await app.waitFor("text:Storage");
  await app.press("text:Done");
  await app.waitFor(SETTINGS, closed);
  expect(app.count("text:Storage")).toBe(0);
  expect(app.count("label:Tile 4")).toBe(1);

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
  await app.waitFor("text:No events yet");
  for (const gone of ["label:Snapshot", "label:Tile 1"]) expect(app.count(gone)).toBe(0);

  await app.press("label:Default");
  await app.waitFor("label:Tile 4");

  await app.press("label:Videos");
  await app.waitFor("text:No videos yet");
  for (const gone of ["label:Snapshot", "label:Record", "label:Show PTZ", "label:Hide PTZ", "label:Camera filter", "label:Tile 1"]) expect(app.count(gone)).toBe(0);
  expect(app.count("text:Open folder")).toBe(1);

  await app.press("label:Default");
  await app.waitFor("label:Tile 4");
  expect(app.count("text:No videos yet")).toBe(0);

  await app.press("label:Snapshots");
  await app.waitFor("text:No snapshots yet");
  for (const gone of ["label:Snapshot", "label:Show PTZ", "label:Hide PTZ", "label:Camera filter", "label:Tile 1"]) expect(app.count(gone)).toBe(0);

  await app.press("label:Default");
  await app.waitFor("label:Tile 4");
  expect(app.count("text:No snapshots yet")).toBe(0);
});

test("Appearance picks a dark theme and a light one, and the mode shows one or follows the system; the texture and elevation go over either", async () => {
  const app = await start();
  await openAppSettings(app, "appearance");
  const oneDefault = () => {
    expect(app.count("label:Default theme")).toBe(1);
    for (const gone of ["Default Dark", "Default Light"]) expect(app.count(`label:${gone} theme`)).toBe(0);
    expect(app.texts().some((text) => /Default (Dark|Light)/.test(text))).toBe(false);
  };
  await app.waitFor("text:Default, dark or light as the system is.");
  oneDefault();
  for (const shown of ["Dark Modern", "Dark High Contrast", "Light Modern", "Light High Contrast"]) expect(app.count(`label:${shown} theme`)).toBe(1);
  for (const name of ["Mode", "Texture", "Elevation", "Theme"]) expect(app.count(`text:${name}`)).toBe(1);
  const under = (text: string) => {
    const texts = app.texts();
    return texts.slice(texts.indexOf("Theme")).indexOf(text);
  };

  await app.press("label:Dark");
  await app.waitFor("text:Always dark, in the dark theme picked below.");
  oneDefault();
  for (const shown of ["Dark Modern", "Monokai", "Dark High Contrast"]) expect(app.count(`label:${shown} theme`)).toBe(1);
  for (const hidden of ["Light Modern", "Light High Contrast"]) expect(app.count(`label:${hidden} theme`)).toBe(0);
  await app.press("label:Monokai theme");
  await app.waitFor("text:Monokai, the dark theme the app shows.");

  await app.press("label:Light");
  await app.waitFor("text:Default, the light theme the app shows.");
  expect(app.count("label:Monokai theme")).toBe(0);
  await app.press("label:Solarized Light theme");
  await app.waitFor("text:Solarized Light, the light theme the app shows.");

  await app.press("label:System");
  await app.waitFor("text:Monokai while the system is dark, Solarized Light while it is light.");
  oneDefault();
  expect(app.count("text:In use")).toBe(1);
  expect(under("Default")).toBeGreaterThan(0);
  expect(under("Default")).toBeLessThan(under("Dark"));
  expect(under("Dark High Contrast")).toBeLessThan(under("Light"));
  expect(under("Light")).toBeLessThan(under("Light Modern"));

  await app.press("label:Dark");
  await app.waitFor("text:Monokai, the dark theme the app shows.");

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

  await app.press("label:Dark High Contrast theme");
  await app.waitFor(`text:${raised} High contrast themes stay flat: their outlines mark every state.`);
  await app.press("label:Default theme");
  await app.waitFor("text:Default, the dark theme the app shows.");
  await app.press("label:No elevation");
  await app.waitFor(flat);

  await app.press("label:System");
  await app.waitFor("text:Default while the system is dark, Solarized Light while it is light.");
  await app.press("label:Default theme");
  await app.waitFor("text:Default, dark or light as the system is.");
  oneDefault();

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
    for (const handle of ["top left corner", "top edge", "top right corner", "right edge", "bottom right corner", "bottom edge", "bottom left corner", "left edge"]) {
      expect(app.count(`label:Resize tile 1 from its ${handle}`)).toBe(1);
    }
    await app.press("label:Remove tile 4");
    await app.waitFor("label:Remove tile 4", { state: "gone" });
    await app.press("text:Done");
    await app.waitFor("label:Tile 3");
    expect(app.count("label:Tile 4")).toBe(0);

    await app.press("label:Layout");
    await app.press("text:Edit layout…");
    await app.press("text:Add tile");
    await app.waitFor("label:Remove tile 4");
    await app.press("text:Cancel");
    await app.waitFor("label:Tile 3");
    expect(app.count("label:Tile 4")).toBe(0);

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
  expect(app.texts()).toContain("Auto · Main stream");

  await app.press("label:Camera 1's stream");
  await app.chooseMenuItem("Sub stream");
  await app.waitFor("text:Sub stream");
  await app.press("label:Camera 1's stream");
  await app.chooseMenuItem("Auto");
  await app.waitFor("text:Sub stream", closed);
  await app.waitFor("text:Auto · Main stream");

  await app.press("label:Camera 1's fit: Original");
  await app.chooseMenuItem("Fill");
  await app.waitFor("label:Camera 1's fit: Fill");
  await app.press("label:Tile 1 options");
  await app.chooseMenuItem("Fit > Original");
  await app.waitFor("label:Camera 1's fit: Original");

  await app.hover("label:Tile 2");
  await app.waitFor("label:Tile 1 options", closed);
  expect(app.count("label:Tile 2 options")).toBe(0);
});

test("a tile on Auto plays the stream its size calls for, and follows the window as it is resized", async () => {
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

    await app.hover("label:Tile 2");
    await app.waitFor("text:Auto · 896×512");
    await app.setViewport(3600, 2400);
    await app.hover("label:Tile 2");
    await app.waitFor("text:Auto · 3840×2160");
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
    await app.press("label:Add a camera to tile 2");
    await app.waitFor("text:New camera…");
    expect(app.menu()).toBe(null);
    await app.press("id:menu >> text:Camera 1");
    await app.waitFor("label:Add a camera to tile 2", closed);
    await app.waitFor("id:menu", closed);

    await app.rightClick("label:Tile 2");
    await app.press("id:menu >> text:Show camera");
    await app.waitFor("text:New camera…");
    await app.press("id:menu >> text:Empty tile");
    await app.waitFor("label:Add a camera to tile 2");
  } finally {
    if (app.count("label:Default") > 0 && app.count("label:Add a camera to tile 2") === 0) {
      await app.press("label:Default");
      await app.hover("label:Tile 2");
      await app.press("label:Tile 2 options");
      await app.press("id:menu >> text:Empty tile");
    }
  }
});

test("a new camera is entered once and can be in two groups", async () => {
  const app = await start();
  await app.press("label:Add camera");
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
    await app.waitUntil(() => app.count("label:Porch") === 3);

    await app.press("label:Porch options >> nth=0");
    await app.chooseMenuItem("Groups > Outside");
    await app.waitUntil(() => app.count("label:Porch") === 2);
  } finally {
    await deleteNamed([], ["Outside", "Garage door"]);
    agreeToDelete(app);
    await app.press("label:Porch options >> nth=0");
    await app.chooseMenuItem("Delete camera…");
    await app.waitFor("label:Porch", closed);
  }
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
  expect(app.texts()).toContain("No videos or snapshots yet.");
  expect(app.texts()).toContain("Nothing in the last 14 days");
  expect(app.texts()).toContain("No events in the last 14 days");
  expect(app.texts()).toContain("Events show here as the cameras see them.");
  expect(app.texts()).toContain("0 events today");
  for (const gone of ["label:Snapshot", "label:Show PTZ", "label:Hide PTZ", "label:Tile 1"]) expect(app.count(gone)).toBe(0);

  await app.press("text:7 days");
  await app.waitFor("text:Nothing in the last 7 days");
  await app.press("text:14 days");

  await app.press("id:tile:disk");
  await app.waitFor("label:Change where snapshots are saved");
  await app.press("text:Done");
  await app.waitFor(SETTINGS, closed);
  expect(app.count("id:dashboard")).toBe(1);

  await app.press("id:tile:videos");
  await app.waitFor("text:No videos yet");

  await app.press("label:Dashboard");
  await app.waitFor("text:Latest events");
  await app.press("text:Show all");
  await app.waitFor("text:No events yet");
});

async function openCameras(app: App) {
  await app.press("label:Cameras");
  await app.waitFor("id:cameras");
}

test("Cameras, beside the videos, lists every camera in a table, and the dashboard's cameras open it", async () => {
  const app = await start();
  await openCameras(app);
  for (const text of ["1 camera", "Name", "Address", "Stream", "Login", "Camera 1", "192.0.2.10:554", "Ch 1  ·  Main  ·  TCP", "No password"]) {
    expect(app.texts()).toContain(text);
  }
  for (const gone of ["label:Snapshot", "label:Show PTZ", "label:Hide PTZ", "label:Tile 1"]) expect(app.count(gone)).toBe(0);
  for (const text of ["1–1 of 1 camera", "Rows per page", "Page 1 of 1"]) expect(app.texts()).toContain(text);
  for (const button of ["label:First page", "label:Previous page", "label:Next page", "label:Last page"]) expect(app.enabled(button)).toBe(false);

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
    await app.press("label:Connect to the camera");
    await app.waitFor("text:This run of the app reaches no camera, so there is nothing to connect to.");
    await app.press("text:Add");
    await app.waitFor(EDITOR, closed);
    await app.waitFor("text:10.0.0.7:554");
    await app.waitFor("text:1 selected");

    await app.keyboard.press("mod+a");
    await app.waitFor("text:3 selected");
    await app.keyboard.down("ctrl");
    await app.press("id:row:Camera 1");
    await app.keyboard.up("ctrl");
    await app.waitFor("text:2 selected");

    await app.press("label:Edit 2 cameras");
    await app.waitFor("text:Apply to 2");
    expect(app.texts()).toContain("They differ: each keeps its own.");
    expect(app.value("label:Username")).toBe("admin");
    expect(app.enabled("label:Apply the changes to 2 cameras")).toBe(false);
    expect(app.count("id:section:features")).toBe(0);
    await app.press("id:section:stream");
    await app.clear("label:RTSP port");
    await app.type("label:RTSP port", "8554");
    await app.press("label:Apply the changes to 2 cameras");
    await app.waitFor("text:Apply to 2", closed);
    await app.waitFor("text:10.0.0.7:8554");
    expect(app.texts()).toContain("10.0.0.8:8554");
    expect(app.texts()).toContain("192.0.2.10:554");
    for (const name of ["Porch", "Yard"]) expect(app.texts()).toContain(name);

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

async function openGroups(app: App) {
  await app.press("label:Groups");
  await app.waitFor("id:groups");
}

test("Groups, beside the cameras, makes a group of cameras in its dialog, and edits its name and cameras", async () => {
  const app = await start();
  await openGroups(app);
  try {
    await app.waitFor("text:No groups yet");
    for (const gone of ["label:Snapshot", "label:Show PTZ", "label:Hide PTZ", "label:Tile 1"]) expect(app.count(gone)).toBe(0);

    await app.press("text:New group");
    await app.waitFor("text:In this group");
    expect(app.enabled("text:Create")).toBe(false);
    await app.type("label:Name", "Outside");
    await app.press("label:Put Camera 1 in");
    await app.waitFor("label:Take Camera 1 out");
    expect(app.texts()).toContain("Every camera is in this group.");
    await app.press("text:Create");
    await app.waitFor("text:In this group", closed);

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

    await app.keyboard.press("mod+d");
    await app.waitFor("id:group:Recorder copy");
    expect(app.texts()).toContain("1 selected");

    await app.keyboard.press("mod+a");
    await app.waitFor("text:2 selected");
    await app.press("label:Cameras in 2 groups");
    await app.chooseMenuItem("Camera 1");
    await app.waitUntil(() => app.count("label:Camera 1") === 3);
    for (const name of ["Recorder", "Recorder copy"]) expect(app.count(`label:${name}, 1 camera`)).toBe(1);

    agreeToDelete(app);
    await app.press("label:Delete 2 groups");
    await app.waitFor("text:No groups yet");
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
