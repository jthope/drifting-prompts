// The IP Camera player: one camera's live video, decoded by WebCodecs, or the list of cameras.
// Without a `description` in its set-up, WebCodecs reads the server's Annex-B pictures as sent.
// The session's key is kept only in this page's memory.

"use strict";

const MOST_WAITING = 8;

const RETRY_AFTER_MS = [1000, 2000, 5000, 10000, 15000];

const asked = new URLSearchParams(location.search);
const camera = asked.get("camera");
const stream = asked.get("stream") ?? "";
let key = asked.get("key");
if (camera) {
  play(camera);
} else {
  list();
}

function ask(url, options = {}) {
  const headers = { ...(options.headers ?? {}) };
  if (key) headers.Authorization = `Bearer ${key}`;
  return fetch(url, { ...options, headers });
}

function keyed(url) {
  if (!key) return url;
  return `${url}${url.includes("?") ? "&" : "?"}key=${encodeURIComponent(key)}`;
}

async function list() {
  const main = document.createElement("main");
  const title = document.createElement("h1");
  title.textContent = "IP Camera";
  main.append(title);
  document.body.append(main);
  await fill(main);
}

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
  let setUp = null;
  let awaitingKeyframe = true;
  let cannotPlay = null;
  let tries = 0;
  /** One message at a time, since setting a decoder up waits on the browser. */
  let handling = Promise.resolve();

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
      // A key the server doesn't know closes at once, so trying again can't help.
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
      // Nothing decodes without its keyframe; a decoder falling behind catches up by skipping.
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
    // The decoder closes itself after an error; a new one starts at the next keyframe.
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

function sentence(reason) {
  const text = (reason ?? "The camera's stream stopped").trim();
  const capitalized = text.charAt(0).toUpperCase() + text.slice(1);
  return /[.!?]$/.test(capitalized) ? capitalized : `${capitalized}.`;
}
