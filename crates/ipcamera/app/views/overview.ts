import { computed, signal } from "nsh";
import { type CameraState, type DriveSpace, type EventInfo, type SnapshotInfo, Storage, type VideoInfo } from "ipcamera/native";
import { events } from "./eventlog";
import { cameraOf, states } from "./model";
import { loaded as videosLoaded, videos } from "./recordings";
import { snapshots, loaded as snapshotsLoaded } from "./snapshots";

/** The periods the dashboard's activity covers, in days, today included. */
export const PERIODS = [
  { value: "7", title: "7 days", days: 7 },
  { value: "14", title: "14 days", days: 14 },
  { value: "30", title: "30 days", days: 30 },
] as const;

export type PeriodKey = (typeof PERIODS)[number]["value"];

/** The period shown. */
export const period = signal<PeriodKey>("14");

/** How many days the period covers. */
export const periodDays = () => PERIODS.find((each) => each.value === period())?.days ?? 14;

/** Everything the dashboard counts. */
export type Summary = {
  cameras: number;
  connected: number;
  recording: number;
  snapshots: number;
  videos: number;
  /** How long the videos run, in seconds. */
  seconds: number;
  snapshotBytes: number;
  videoBytes: number;
};

const sum = <T>(list: T[], of: (each: T) => number) => list.reduce((total, each) => total + of(each), 0);

export function summarize(cameras: CameraState[], shots: SnapshotInfo[], clips: VideoInfo[]): Summary {
  return {
    cameras: cameras.length,
    connected: cameras.filter((camera) => camera.connected).length,
    recording: cameras.filter((camera) => camera.recording).length,
    snapshots: shots.length,
    videos: clips.length,
    seconds: sum(clips, (clip) => clip.seconds),
    snapshotBytes: sum(shots, (shot) => shot.bytes),
    videoBytes: sum(clips, (clip) => clip.bytes),
  };
}

export const summary = computed(() => summarize(states(), snapshots(), videos()));

/** Whether both folders have been read once, so the dashboard never shows a count of nothing while it reads. */
export const ready = () => snapshotsLoaded() && videosLoaded();

/** One day of the activity chart: when it starts (local midnight, in milliseconds since 1970) and what was saved in it. */
export type Day = { start: number; videos: number; snapshots: number; seconds: number };

function dayStart(at: number): number {
  const date = new Date(at);
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

/** The last `count` local calendar days, today last, with what was saved each; calendar days, so a clock change never splits one. */
export function daysOf(shots: SnapshotInfo[], clips: VideoInfo[], count: number, now = Date.now()): Day[] {
  const today = new Date(dayStart(now));
  const days: Day[] = [];
  for (let back = count - 1; back >= 0; back--) {
    const date = new Date(today.getFullYear(), today.getMonth(), today.getDate() - back);
    days.push({ start: date.getTime(), videos: 0, snapshots: 0, seconds: 0 });
  }
  const byStart = new Map(days.map((day) => [day.start, day]));
  for (const clip of clips) {
    const day = byStart.get(dayStart(clip.startedAt));
    if (!day) continue;
    day.videos += 1;
    day.seconds += clip.seconds;
  }
  for (const shot of shots) {
    const day = byStart.get(dayStart(shot.takenAt));
    if (day) day.snapshots += 1;
  }
  return days;
}

/** The time the days are counted back from. */
export const now = signal(Date.now());

/** The days of the period shown. */
export const days = computed(() => daysOf(snapshots(), videos(), periodDays(), now()));

/** One hour of one weekday (0 for Sunday, as `Date.getDay` counts) and the events that started in it over the period. */
export type HourCell = { weekday: number; hour: number; events: number };

/** Every hour of every weekday, Sunday and midnight first, with the events that started in it over the last `count` days. */
export function eventHours(list: EventInfo[], count: number, now = Date.now()): HourCell[] {
  const today = new Date(dayStart(now));
  const from = new Date(today.getFullYear(), today.getMonth(), today.getDate() - (count - 1)).getTime();
  const cells: HourCell[] = [];
  for (let weekday = 0; weekday < 7; weekday++) {
    for (let hour = 0; hour < 24; hour++) cells.push({ weekday, hour, events: 0 });
  }
  for (const event of list) {
    if (event.startedAt < from || event.startedAt > now) continue;
    const at = new Date(event.startedAt);
    cells[at.getDay() * 24 + at.getHours()].events += 1;
  }
  return cells;
}

/** One hour of the day and the events that started in it over the period, whatever the weekday. */
export type HourTotal = { hour: number; events: number };

/** The hours of `cells` added up over the weekdays, midnight first: the heatmap's columns. */
export function hoursOf(cells: HourCell[]): HourTotal[] {
  const totals = Array.from({ length: 24 }, (_, hour) => ({ hour, events: 0 }));
  for (const cell of cells) totals[cell.hour].events += cell.events;
  return totals;
}

/** The weekdays and hours of the period shown. */
export const hourCells = computed(() => eventHours(events(), periodDays(), now()));

/** The hours of the day of the period shown. */
export const hourTotals = computed(() => hoursOf(hourCells()));

const shortWhen = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });

/** How long ago `at` was: `Just now`, `12 min ago`, `5 h ago`, then the date and time. */
export function agoLabel(at: number, now: number): string {
  const minutes = Math.floor((now - at) / 60_000);
  if (minutes < 1) return "Just now";
  if (minutes < 60) return `${minutes} min ago`;
  if (minutes < 24 * 60) return `${Math.floor(minutes / 60)} h ago`;
  return shortWhen.format(new Date(at));
}

/** The space one camera's files take. Files of cameras since deleted add up under one row, as do files whose camera can't be told. */
export type CameraSpace = { key: string; name: string; videos: number; snapshots: number; total: number };

export function spaceByCamera(shots: SnapshotInfo[], clips: VideoInfo[], nameOf: (id: string) => string | undefined): CameraSpace[] {
  const rows = new Map<string, CameraSpace>();
  const rowOf = (id: string) => {
    const name = id === "" ? undefined : nameOf(id);
    const key = name !== undefined ? id : id === "" ? "unknown" : "deleted";
    let row = rows.get(key);
    if (!row) {
      row = { key, name: name ?? (key === "unknown" ? "Unknown camera" : "Deleted cameras"), videos: 0, snapshots: 0, total: 0 };
      rows.set(key, row);
    }
    return row;
  };
  for (const clip of clips) {
    const row = rowOf(clip.camera);
    row.videos += clip.bytes;
    row.total += clip.bytes;
  }
  for (const shot of shots) {
    const row = rowOf(shot.camera);
    row.snapshots += shot.bytes;
    row.total += shot.bytes;
  }
  return [...rows.values()].sort((one, other) => other.total - one.total || one.name.localeCompare(other.name));
}

export const cameraSpace = computed(() => spaceByCamera(snapshots(), videos(), (id) => cameraOf(id)?.name));

/** A drive one or both folders are on, with what the app's files take of it. */
export type Drive = DriveSpace & { videos: number; snapshots: number };

/** The drives under the two folders: one when both are on the same drive. */
export function drivesOf(videoSpace: DriveSpace, snapshotSpace: DriveSpace, sums: Pick<Summary, "videoBytes" | "snapshotBytes">): Drive[] {
  if (videoSpace.drive === snapshotSpace.drive) return [{ ...videoSpace, videos: sums.videoBytes, snapshots: sums.snapshotBytes }];
  return [
    { ...videoSpace, videos: sums.videoBytes, snapshots: 0 },
    { ...snapshotSpace, videos: 0, snapshots: sums.snapshotBytes },
  ];
}

const space = signal<{ videos: DriveSpace; snapshots: DriveSpace } | { problem: string } | undefined>(undefined);

/** The drives under the folders, `[]` until they are known; `spaceProblem` says why they aren't. */
export const drives = computed(() => {
  const known = space();
  return known && "videos" in known ? drivesOf(known.videos, known.snapshots, summary()) : [];
});
export const spaceProblem = () => {
  const known = space();
  return known && "problem" in known ? known.problem : undefined;
};

let asking = 0;

/** Asks the system again how full the drives are. An answer that comes back after a newer one is dropped. */
export function reloadSpace() {
  const ticket = ++asking;
  Promise.all([Storage.space("videos"), Storage.space("snapshots")])
    .then(([videoSpace, snapshotSpace]) => ticket === asking && space.set({ videos: videoSpace, snapshots: snapshotSpace }))
    .catch((error) => ticket === asking && space.set({ problem: String((error as { message?: string })?.message ?? error) }));
}
