// The IP Camera player: one camera's live video from the server, decoded by the browser's own
// WebCodecs decoder and drawn on a canvas, at `/watch?camera=<id>`, or `&stream=<key>` for a
// stream a tile picks in place of the camera's own (`main`, `sub` or a path). With no camera, it
// lists the cameras instead.
//
// The server sends JSON text for the decoder's set-up and the camera's status, and one binary
// message per picture: a flags byte (bit 0: keyframe), an 8-byte big-endian timestamp in
// microseconds, then the access unit in Annex-B form (`crates/ipcamera/server/src/live.rs`). With no
// `description` in its set-up, WebCodecs reads H.264 and H.265 in exactly that form.
//
// A server with a password answers only requests that carry a session's key
// (`crates/ipcamera/server/src/access.rs`). The page takes it from its own address (`&key=<key>`), as the
// web build's tiles give it, or, for the list, asks for the password and signs in itself. The key
// is kept only in this page's memory.

"use strict";

/** Pictures waiting in the decoder past which the player skips to the next keyframe. */
const MOST_WAITING = 8;

/** How long to wait before reaching the server again, by try: then every 15 s. */
const RETRY_AFTER_MS = [1000, 2000, 5000, 10000, 15000];

const asked = new URLSearchParams(location.search);
const camera = asked.get("camera");
/** The stream a tile picks in place of the camera's own; `""` for its own. */
const stream = asked.get("stream") ?? "";
/** The session's key, when the server has a password: from the address, or from signing in. */
let key = asked.get("key");
if (camera) {
  play(camera);
} else {
  list();
}

/** `fetch`, with the session's key when there is one. */
function ask(url, options = {}) {
  const headers = { ...(options.headers ?? {}) };
  if (key) headers.Authorization = `Bearer ${key}`;
  return fetch(url, { ...options, headers });
}

/** `url` with the session's key in it, for a request no header can go with. */
function keyed(url) {
  if (!key) return url;
  return `${url}${url.includes("?") ? "&" : "?"}key=${encodeURIComponent(key)}`;
}

/** The page's list of cameras, each a link that plays it. */
async function list() {
  const main = document.createElement("main");
  const title = document.createElement("h1");
  title.textContent = "IP Camera";
  main.append(title);
  document.body.append(main);
  await fill(main);
}

/** Puts the cameras in `main`, or the password box when the server wants one first. */
async function fill(main) {
  while (main.children.length > 1) main.lastChild.remove();
  let cameras;
  try {
    const response = await ask("/api/cameras");
    if (response.status === 401) {
      main.append(signIn(main));
      return;
    }
    cameras = await response.json();
  } catch {
    main.append(note("The server didn't answer. Check that it's still running, then reload."));
    return;
  }
  if (cameras.length === 0) {
    main.append(note("This server has no cameras yet."));
    return;
  }
  const items = document.createElement("ul");
  for (const { id, name } of cameras) {
    const link = document.createElement("a");
    link.href = keyed(`/watch?camera=${encodeURIComponent(id)}`);
    link.textContent = name || "Unnamed camera";
    const item = document.createElement("li");
    item.append(link);
    items.append(item);
  }
  main.append(items);
}

/** The password box: signing in fills `main` with the cameras. */
function signIn(main) {
  const form = document.createElement("form");
  const label = document.createElement("label");
  label.textContent = "The server's password";
  const password = document.createElement("input");
  password.type = "password";
  password.autocomplete = "current-password";
  password.required = true;
  label.append(password);
  const button = document.createElement("button");
  button.textContent = "Sign in";
  const problem = note("");
  problem.setAttribute("role", "alert");
  form.append(label, button, problem);
  form.onsubmit = async (event) => {
    event.preventDefault();
    button.disabled = true;
    try {
      const response = await fetch("/api/session", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ password: password.value }),
      });
      if (response.ok) {
        ({ key } = await response.json());
        await fill(main);
        return;
      }
      problem.textContent = sentence(await response.text());
    } catch {
      problem.textContent = "The server didn't answer. Check that it's still running, then try again.";
    }
    button.disabled = false;
  };
  return form;
}

function note(text) {
  const paragraph = document.createElement("p");
  paragraph.className = "note";
  paragraph.textContent = text;
  return paragraph;
}

/** Plays camera `id` full size, reconnecting to the server if it goes away. */
function play(id) {
  document.body.classList.add("playing");
  const canvas = document.createElement("canvas");
  const status = document.createElement("div");
  status.id = "status";
  status.setAttribute("role", "status");
  document.body.append(canvas, status);
  const context = canvas.getContext("2d");

  ask("/api/cameras")
    .then((response) => response.json())
    .then((cameras) => {
      const shown = cameras.find((each) => each.id === id);
      if (shown?.name) document.title = `${shown.name} · IP Camera`;
    })
    .catch(() => {});

  if (typeof VideoDecoder === "undefined") {
    show("This browser can't decode video for the page. A current Chrome, Edge, Firefox or Safari can.");
    return;
  }

  let decoder = null;
  /** The set-up the server sent last, to set a new decoder up with after a decoding error. */
  let setUp = null;
  /** Whether pictures are skipped until a keyframe: at first, after a set-up, after skipping. */
  let awaitingKeyframe = true;
  /** A reason the video can't play here, which outranks the camera's status. */
  let cannotPlay = null;
  let tries = 0;
  /** Messages are handled one after another, since setting a decoder up waits on the browser. */
  let handling = Promise.resolve();

  // Only the newest decoded picture is kept, and drawn at the next display refresh.
  let newest = null;
  let drawing = false;

  connect();

  function connect() {
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const picked = stream ? `?stream=${encodeURIComponent(stream)}` : "";
    const socket = new WebSocket(keyed(`${scheme}://${location.host}/live/${encodeURIComponent(id)}${picked}`));
    socket.binaryType = "arraybuffer";
    socket.onopen = () => {
      tries = 0;
    };
    socket.onmessage = (event) => {
      handling = handling.then(() =>
        typeof event.data === "string" ? text(JSON.parse(event.data)) : picture(event.data),
      );
    };
    socket.onclose = async () => {
      awaitingKeyframe = true;
      // A server that wants a password closes at once on a key it doesn't know. Trying again
      // can't help: the app shows this page again with a new key once someone has signed in.
      if (await signedOut()) {
        show("Sign in to the server to watch this camera.");
        return;
      }
      const wait = RETRY_AFTER_MS[Math.min(tries, RETRY_AFTER_MS.length - 1)];
      tries += 1;
      show("The server can't be reached. Trying again…");
      setTimeout(connect, wait);
    };
  }

  /** Whether the server says this page's key is no session's; `false` when it doesn't answer. */
  async function signedOut() {
    try {
      const session = await (await ask("/api/session")).json();
      return session.required && !session.signed_in;
    } catch {
      return false;
    }
  }

  async function text(message) {
    if (message.type === "status") {
      showStatus(message);
    } else if (message.type === "config") {
      await configure(message);
    }
  }

  function showStatus({ state, attempt, reason }) {
    if (cannotPlay) return;
    switch (state) {
      case "live":
        show(null);
        break;
      case "connecting":
        show("Connecting to the camera…");
        break;
      case "reconnecting":
        show(`The camera's stream dropped. Reconnecting (try ${attempt})…`);
        break;
      case "stopped":
        show("The camera's stream stopped.");
        break;
      case "failed":
        show(sentence(reason));
        break;
    }
  }

  async function configure({ codec, width, height }) {
    const wanted = { codec, optimizeForLatency: true };
    if (width && height) {
      wanted.codedWidth = width;
      wanted.codedHeight = height;
    }
    let supported = false;
    try {
      ({ supported } = await VideoDecoder.isConfigSupported(wanted));
    } catch {
      supported = false;
    }
    if (!supported) {
      cannotPlay = codec.startsWith("hev1") || codec.startsWith("hvc1")
        ? "This browser can't decode the camera's H.265 video. Chrome or Edge can, on a computer whose graphics decode H.265; or set the camera to send H.264 on its sub stream and use that."
        : `This browser can't decode the camera's video (${codec}).`;
      show(cannotPlay);
      setUp = null;
      decoder?.close();
      decoder = null;
      return;
    }
    cannotPlay = null;
    setUp = wanted;
    makeDecoder();
  }

  function makeDecoder() {
    if (!decoder || decoder.state === "closed") {
      decoder = new VideoDecoder({ output: decoded, error: failed });
    }
    decoder.configure(setUp);
    awaitingKeyframe = true;
  }

  function picture(buffer) {
    if (!setUp) return;
    if (!decoder || decoder.state === "closed") makeDecoder();
    const view = new DataView(buffer);
    const keyframe = (view.getUint8(0) & 1) === 1;
    if (!keyframe && (awaitingKeyframe || decoder.decodeQueueSize > MOST_WAITING)) {
      // Nothing decodes without its keyframe, and a decoder falling behind catches up faster by
      // skipping to the next one than by working through every picture.
      awaitingKeyframe = true;
      return;
    }
    awaitingKeyframe = false;
    decoder.decode(
      new EncodedVideoChunk({
        type: keyframe ? "key" : "delta",
        timestamp: Number(view.getBigUint64(1)),
        data: new Uint8Array(buffer, 9),
      }),
    );
  }

  function failed(error) {
    // The decoder closes itself after an error. A new one starts at the next keyframe.
    show(`The picture couldn't be decoded (${error.message}). Starting again at the next keyframe…`);
    awaitingKeyframe = true;
  }

  function decoded(frame) {
    newest?.close();
    newest = frame;
    if (!drawing) {
      drawing = true;
      requestAnimationFrame(draw);
    }
  }

  function draw() {
    drawing = false;
    const frame = newest;
    newest = null;
    if (!frame) return;
    const scale = devicePixelRatio || 1;
    const width = Math.round(canvas.clientWidth * scale);
    const height = Math.round(canvas.clientHeight * scale);
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    // The whole picture, as large as fits, in the middle.
    const fit = Math.min(width / frame.displayWidth, height / frame.displayHeight);
    const shownWidth = frame.displayWidth * fit;
    const shownHeight = frame.displayHeight * fit;
    context.clearRect(0, 0, width, height);
    context.drawImage(frame, (width - shownWidth) / 2, (height - shownHeight) / 2, shownWidth, shownHeight);
    frame.close();
    if (!cannotPlay && status.textContent.startsWith("The picture couldn't")) show(null);
  }

  function show(message) {
    status.hidden = !message;
    status.textContent = message ?? "";
  }
}

/** A reason from the server, as a sentence: capitalized, with a full stop. */
function sentence(reason) {
  const text = (reason ?? "The camera's stream stopped").trim();
  const capitalized = text.charAt(0).toUpperCase() + text.slice(1);
  return /[.!?]$/.test(capitalized) ? capitalized : `${capitalized}.`;
}
