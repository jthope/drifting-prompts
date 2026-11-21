import { computed, createAnimatedValue, signal } from "nsh";
import { type EventInfo, Events, Preferences } from "ipcamera/native";
import { confirmDelete, notify } from "../../../dp/ui";
import { failed, type MenuEntries } from "./actions";
import { cameraMenuEntry, cameraName, countLabel, libraryOf, menuOf } from "./library";
import { cameraOf, detectionsLabel, show } from "./model";

/** An event as the page lists it, known by `name` and `event` (its id), tagged with what the camera saw. */
export type EventItem = EventInfo & { name: string; tags: string[]; event: string };

/** Every event, newest first, as Rust last read it. */
export const events = signal<EventItem[]>([]);
/** Whether the first read has arrived, so the page doesn't say there are none meanwhile. */
export const loaded = signal(false);
/** How the page is narrowed, sorted, grouped and shown. */
export const eventLibrary = libraryOf(events, (event) => event.startedAt, "day", "list");
export const { filtered, clearFilters, visible, sections, ordered } = eventLibrary;
/** The event whose picture is open in the viewer, by its id. */
export const viewing = signal<string | null>(null);

/** The event the preview shows, by id, as picked; `null` until one is. */
export const current = signal<string | null>(null);

/** The events the table shows rows for, in its order: those of folded sections left out. */
export const shownRows = computed(() =>
  eventLibrary
    .sections()
    .filter((section) => section.title === "" || !eventLibrary.isCollapsed(section.key))
    .flatMap((section) => section.items),
);

/** The event the preview shows: the one picked while it still shows, else the first row; `null` with no rows. */
export const previewed = computed(() => {
  const list = shownRows();
  return list.find((event) => event.name === current()) ?? list[0] ?? null;
});

/** Moves the preview `by` rows, returning the event it moved to, or `null` where it didn't. */
export function stepPreview(by: number): string | null {
  const list = shownRows();
  const at = list.findIndex((event) => event.name === previewed()?.name);
  const next = list[at + by];
  if (at < 0 || next === undefined) return null;
  current.set(next.name);
  return next.name;
}

/** The page's scroll offset, fed natively by its list. */
export const eventsScroll = createAnimatedValue(0);

/** Whether this app keeps an event log. */
export const kept = Events.kept();

/** Whether a new event brings a message. */
export const alertsOn = signal(Preferences.alerts());

/** Turns the messages on or off, and saves it. */
export function setAlerts(on: boolean) {
  alertsOn.set(on);
  Preferences.saveAlerts(on);
}

let reading = 0;

/** Reads the log again. A read that finishes after a newer one is dropped. */
export function reload() {
  const ticket = ++reading;
  Events.list()
    .then((list) => {
      if (ticket !== reading) return;
      const items = list.map((event) => ({ ...event, name: event.id, tags: event.detected.length > 0 ? event.detected : ["motion"], event: event.id }));
      tellNew(items);
      events.set(items);
      loaded.set(true);
    })
    .catch(failed);
}

/** Follows the log for as long as the app runs, where the app keeps one. */
export function followEvents() {
  if (kept) Events.onChange(() => reload());
}

const told = new Set<string>();
let firstRead = true;

/** Tells each new event still going on; what the log held as the app opened is no news. */
function tellNew(list: EventItem[]) {
  const fresh = list.filter((event) => !told.has(event.id));
  for (const event of fresh) told.add(event.id);
  if (firstRead) {
    firstRead = false;
    return;
  }
  if (!alertsOn()) return;
  // Oldest first, so the newest lands on top.
  for (const event of fresh.reverse()) {
    if (!event.ongoing) continue;
    const camera = event.camera;
    const action = cameraOf(camera) ? { label: "Show", onPress: () => show({ kind: "camera", id: camera }) } : undefined;
    notify(`${whatLabel(event)} at ${cameraName(camera)}.`, "warn", action);
  }
}

/** What a camera saw: what its AI told apart (`Person, Vehicle`), else `Motion`. */
export function whatLabel(event: EventInfo): string {
  return event.detected.length > 0 ? detectionsLabel(event.detected) : "Motion";
}

/** When an event started, as much as its section's title doesn't say. */
export const startedLabel = eventLibrary.timeLabel;

const longDate = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric", year: "numeric", hour: "numeric", minute: "2-digit", second: "2-digit" });

/** When an event started, in full. */
export const startedInFull = (event: EventInfo) => longDate.format(new Date(event.startedAt));

/** How long the camera saw an event: `38 s`, `2 min 48 s`; `Going on now` while it sees it. */
export function lastedLabel(event: EventInfo): string {
  if (event.ongoing) return "Going on now";
  const seconds = Math.max(0, Math.round((event.endedAt - event.startedAt) / 1000));
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return seconds % 60 === 0 ? `${minutes} min` : `${minutes} min ${seconds % 60} s`;
  const hours = Math.floor(minutes / 60);
  return minutes % 60 === 0 ? `${hours} h` : `${hours} h ${minutes % 60} min`;
}

/** "1 event", "12 events". */
export const eventsLabel = (count: number) => `${countLabel(count)} ${count === 1 ? "event" : "events"}`;

/** Shows the camera that saw it, on its own, closing the viewer if it is open. */
export function showCamera(event: EventInfo) {
  viewing.set(null);
  if (cameraOf(event.camera)) show({ kind: "camera", id: event.camera });
  else notify("The camera that saw this has been deleted.");
}

/** What `event` saved, in words, for the question that deletes it: `its picture and 6 videos`; `""` when it saved nothing. */
export function savedLabel(event: EventInfo): string {
  const picture = event.snapshot !== "" ? "its picture" : "";
  const videos = event.videos.length === 0 ? "" : event.videos.length === 1 ? "its video" : `its ${countLabel(event.videos.length)} videos`;
  return [picture, videos].filter((part) => part !== "").join(" and ");
}

/** Deletes the event and what it saved, asking first when it saved anything; resolves true once it is gone. */
export async function deleteEvent(event: EventInfo): Promise<boolean> {
  const saved = savedLabel(event);
  if (saved !== "") {
    const sure = await confirmDelete({
      title: "Delete this event?",
      description: `${saved.charAt(0).toUpperCase()}${saved.slice(1)} ${saved.includes(" and ") || event.videos.length > 1 ? "are" : "is"} deleted for good, from Snapshots and Videos too.`,
      action: "Delete event",
    });
    if (!sure) return false;
  }
  try {
    return (await Events.delete([event.id])) > 0;
  } catch (error) {
    failed(error);
    return false;
  } finally {
    reload();
  }
}

/** Deletes `list` with what they saved, asking first as `deleteEvent` does; those still going on stay. */
export async function deleteEvents(list: EventItem[]) {
  const ended = list.filter((event) => !event.ongoing);
  const going = list.length - ended.length;
  if (ended.length === 0) {
    if (going > 0) notify("The camera still sees this. Delete it once it has ended.");
    return;
  }
  if (ended.length === 1 && going === 0) {
    if (await deleteEvent(ended[0])) eventLibrary.selected.set(eventLibrary.selected().filter((name) => name !== ended[0].name));
    return;
  }
  if (ended.length > 1 || ended.some((event) => savedLabel(event) !== "")) {
    const staying = going === 0 ? "" : going === 1 ? " The one a camera still sees stays." : ` The ${countLabel(going)} a camera still sees stay.`;
    const sure = await confirmDelete({
      title: `Delete ${eventsLabel(ended.length)}?`,
      description: `Each is deleted with what it saved, for good, from Snapshots and Videos too.${staying}`,
      action: "Delete events",
    });
    if (!sure) return;
  }
  try {
    await Events.delete(ended.map((event) => event.id));
    const gone = new Set(ended.map((event) => event.name));
    eventLibrary.selected.set(eventLibrary.selected().filter((name) => !gone.has(name)));
  } catch (error) {
    failed(error);
  } finally {
    reload();
  }
}

/** Asks, then deletes every event that has ended. */
export async function clearEvents() {
  const sure = await confirmDelete({
    title: "Delete every event?",
    description: "Every event a camera doesn't see now is deleted, with what it saved, for good, from Snapshots and Videos too. Those a camera sees now stay.",
    action: "Delete events",
  });
  if (!sure) return;
  try {
    const count = await Events.clear();
    notify(count === 1 ? "Deleted 1 event." : `Deleted ${countLabel(count)} events.`, "ok");
  } catch (error) {
    failed(error);
  } finally {
    reload();
  }
}

/** What an event's menu offers, on the page and in the viewer, with `links` (what it saved) after Open. */
export function eventMenu(event: EventItem, links: MenuEntries = []): MenuEntries {
  return [
    { label: "Open picture", disabled: event.picture === "", onSelect: () => viewing.set(event.name) },
    ...links,
    { type: "separator" as const },
    { label: "Show camera", disabled: !cameraOf(event.camera), onSelect: () => showCamera(event) },
    { type: "separator" as const },
    cameraMenuEntry(eventLibrary, event),
    { type: "separator" as const },
    { label: savedLabel(event) === "" ? "Delete event" : "Delete event…", disabled: event.ongoing, onSelect: () => deleteEvent(event) },
  ];
}

/** An event's right press on the page: the selection's menu when it is one of several selected, else its own, with `links` after Open. */
export const tileMenu = (event: EventItem, links: MenuEntries = []) => menuOf(eventLibrary, event, () => eventMenu(event, links), deleteEvents);

const startOfToday = () => new Date(new Date().setHours(0, 0, 0, 0)).getTime();

/** What the events the filters keep add up to, for the header. */
export const totals = computed(() => {
  const list = visible();
  const today = startOfToday();
  return {
    count: list.length,
    cameras: new Set(list.map((event) => event.camera)).size,
    today: list.filter((event) => event.startedAt >= today).length,
    ongoing: list.filter((event) => event.ongoing).length,
  };
});
