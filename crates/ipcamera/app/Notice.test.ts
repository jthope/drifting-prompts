import { expect, test } from "nsh/test";
import type { CameraState, RunningStream } from "ipcamera/native";
import { pictureNotice } from "./views/model";

function camera(more: Partial<CameraState> = {}): CameraState {
  return {
    id: "a",
    name: "Driveway",
    streamLabel: "",
    address: "",
    streams: [],
    connected: true,
    ready: true,
    status: "Live.",
    tone: "ok",
    stats: "",
    ptzStatus: "",
    muted: false,
    volume: 80,
    recording: false,
    recordProblem: "",
    motion: false,
    detected: [],
    spotlight: false,
    spotlightOn: false,
    watchStatus: "",
    ...more,
  };
}

function stream(more: Partial<RunningStream> = {}): RunningStream {
  return {
    stream: "",
    stats: "1920×1080 · 25.0 fps · TCP",
    brief: "1920×1080 · 25 fps",
    waiting: false,
    trouble: "fine",
    problem: "",
    lastPictureAt: 0,
    ...more,
  };
}

test("a camera that isn't connected says why", () => {
  expect(pictureNotice(camera({ connected: false, status: "Idle.", tone: "normal" }), undefined)).toEqual({ kind: "off", title: "Not connected", text: "Idle." });
});

test("nothing covers a picture while pictures come, or while its stream first connects", () => {
  expect(pictureNotice(camera(), stream())).toBeUndefined();
  expect(pictureNotice(camera(), stream({ waiting: true }))).toBeUndefined();
  expect(pictureNotice(camera(), undefined)).toBeUndefined();
});

test("a stream that lost the camera says it reconnects, and when its last picture came", () => {
  const at = new Date(2026, 10, 13, 14, 32, 7).getTime();
  const notice = pictureNotice(camera(), stream({ trouble: "reconnecting", problem: "Reconnecting (attempt 3)…", lastPictureAt: at }));
  expect(notice?.kind).toBe("reconnecting");
  expect(notice?.title).toBe("Connection lost");
  expect(notice?.text).toBe("Reconnecting (attempt 3)…");
  // As this machine writes a time, to the second.
  expect(notice?.detail).toMatch(/^Last picture .*32.07/);
});

test("a stream that never had a picture can't have lost its connection", () => {
  const notice = pictureNotice(camera(), stream({ trouble: "reconnecting", problem: "Reconnecting (attempt 1)…", waiting: true }));
  expect(notice?.title).toBe("Can't reach the camera");
  expect(notice?.detail).toBeUndefined();
});

test("a stream that gave up says why, and one whose login was refused says so apart", () => {
  const stopped = pictureNotice(camera(), stream({ trouble: "stopped", problem: "The camera sends no H.264 or H.265 video.", lastPictureAt: Date.now() }));
  expect(stopped?.kind).toBe("stopped");
  expect(stopped?.title).toBe("Stream stopped");
  expect(stopped?.text).toBe("The camera sends no H.264 or H.265 video.");
  expect(stopped?.detail).toMatch(/^Last picture /);

  const refused = pictureNotice(camera(), stream({ trouble: "refused", problem: "The camera rejected the username or password." }));
  expect(refused).toEqual({ kind: "refused", title: "Login refused", text: "The camera rejected the username or password.", detail: undefined });
});
