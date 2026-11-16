// How an event's videos are made, as Settings → Automation offers it for every
// camera and a camera's Events part offers it for that camera alone, in place
// of the app's (`ownEventVideo`): the choices each menu holds, how they read,
// and the kinds that save a video. Rust brings any number outside its ranges
// back within them as it saves (`EventVideo::within_limits`).
import type { CameraForm, EventVideoSettings } from "ipcamera/native";

/** How long a video keeps recording after the camera last saw something, in seconds. */
export const AFTER_SECONDS = [10, 15, 20, 30, 45, 60, 90, 120];

/** How much from before the camera saw something a video starts with, in seconds: 0 for none. */
export const BEFORE_SECONDS = [0, 5, 10, 15, 20, 30];

/** The longest a video runs before it goes on in a new file, in minutes: 0 for no limit. */
export const LONGEST_MINUTES = [0, 1, 2, 5, 10, 15, 30, 60];

/** What a camera can tell apart, by `DetectionInfo` key, as the kinds that save a video list them. */
export const KINDS: { key: string; title: string }[] = [
  { key: "person", title: "People" },
  { key: "vehicle", title: "Vehicles" },
  { key: "animal", title: "Animals" },
  { key: "face", title: "Faces" },
  { key: "package", title: "Packages" },
];

/** The kinds as Save videos of lays them out, a column each: people over animals, vehicles over faces, then packages alone. */
export const KIND_COLUMNS = [
  ["person", "animal"],
  ["vehicle", "face"],
  ["package"],
].map((keys) => KINDS.filter((kind) => keys.includes(kind.key)));

/** A number of seconds as a menu says it: `10 seconds`, `1 minute`, `90 seconds`, `2 minutes`. */
export function secondsLabel(seconds: number): string {
  if (seconds >= 60 && seconds % 60 === 0) {
    const minutes = seconds / 60;
    return minutes === 1 ? "1 minute" : `${minutes} minutes`;
  }
  return seconds === 1 ? "1 second" : `${seconds} seconds`;
}

/** A pre-roll as its menu says it: `Off` for none. */
export function beforeLabel(seconds: number): string {
  return seconds <= 0 ? "Off" : secondsLabel(seconds);
}

/** The longest video as its menu says it: `No limit` for 0. */
export function longestLabel(minutes: number): string {
  if (minutes <= 0) return "No limit";
  return minutes === 1 ? "1 minute" : `${minutes} minutes`;
}

/** The items of a menu of `values`, each said as `label` says it, with `current` among them when a hand-edited file holds another. */
export function numberItems(values: number[], current: number, label: (value: number) => string) {
  const all = values.includes(current) ? values : [...values, current].sort((a, b) => a - b);
  return all.map((value) => ({ value: String(value), title: label(value) }));
}

/** The streams Settings → Automation offers every camera's videos: each camera's own, or the main or sub stream its brand gives. */
export const APP_STREAMS = [
  { value: "own", title: "Each camera's own" },
  { value: "main", title: "Main stream" },
  { value: "sub", title: "Sub stream" },
];

/**
 * The streams a camera's Events part offers its videos: its own, then each
 * stream it listed when Connect asked it, or, for a camera that listed none,
 * the main and sub streams its brand gives; and the saved one when it is none
 * of those, so opening the form loses nothing.
 */
export function cameraStreams(form: Pick<CameraForm, "streams" | "eventVideo">): { value: string; title: string }[] {
  const listed =
    form.streams.length > 0
      ? form.streams.map((stream) => ({ value: stream.path, title: stream.lens ? `${stream.title} · ${stream.lens}` : stream.title }))
      : APP_STREAMS.slice(1);
  const all = [{ value: "own", title: "Its own (Stream's choice)" }, ...listed];
  const saved = form.eventVideo.stream;
  return saved === "" || all.some((each) => each.value === saved) ? all : [...all, { value: saved, title: `Saved: ${saved}` }];
}

/** `settings` with kind `key` saving videos or not, the kinds in `KINDS`' order. */
export function withKind(settings: EventVideoSettings, key: string, on: boolean): EventVideoSettings {
  const kinds = KINDS.map((kind) => kind.key).filter((each) => (each === key ? on : settings.kinds.includes(each)));
  return { ...settings, kinds };
}
