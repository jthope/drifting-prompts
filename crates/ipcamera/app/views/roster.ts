// What the Cameras page lists: every camera with what it is set to, narrowed by
// the search and the filters, in the chosen order, and which of them are
// selected. Like `library.ts`'s, these are module-level signals, so the page's
// header, its table, its selection bar and the editor for several cameras share
// them, and coming back to the page finds them as they were left. The order,
// the selection and the pages are the table's (`listing.ts`), which the Groups
// page's is too.
import { computed, signal, untracked } from "nsh";
import { type CameraChanges, type CameraForm, Cameras, type CameraState } from "ipcamera/native";
import { byText, copyName, listingOf } from "./listing";
import { groups, states } from "./model";

export { PAGE_SIZES, type Pick, pickOf } from "./listing";

/** A camera's saved settings, as the table reads them: the form, with only whether it has a password. */
export type Saved = Omit<CameraForm, "password" | "passwordNote"> & { id: string; hasPassword: boolean };

/** A camera as a row of the table: its settings, what it is doing, and the names of its groups in the sidebar's order. */
export type CameraRow = Saved & { state: CameraState; groups: string[] };

/** Every camera's saved settings, as `reload` last read them. */
export const saved = signal<Saved[]>([]);

/**
 * `form` as the table keeps it: the password stays in Rust, and only whether
 * there is one comes here. On the web the form never holds a saved password,
 * and its note says one is kept instead.
 */
export function savedOf(id: string, form: CameraForm): Saved {
  const { password, passwordNote, ...rest } = form;
  return { ...rest, id, hasPassword: password !== "" || passwordNote !== "" };
}

/** Camera `id`'s settings as Rust has them now; none for a camera deleted since its id was read. */
function read(id: string): Saved | undefined {
  try {
    return savedOf(id, Cameras.form(id));
  } catch {
    return undefined;
  }
}

/** Reads every camera's settings again: when the page opens, and after any saved change. */
export function reload() {
  saved.set(untracked(states).flatMap((camera) => read(camera.id) ?? []));
}

/** Every camera, in the saved order, with what it is doing and its groups. */
export const rows = computed((): CameraRow[] => {
  const settings = new Map(saved().map((each) => [each.id, each]));
  const all = groups();
  return states().flatMap((state) => {
    // A camera added a moment ago is read on its own until the next `reload`, so it is never missing.
    const found = settings.get(state.id) ?? read(state.id);
    if (!found) return [];
    // The name the camera goes by now, which the states follow a moment after a save.
    return [{ ...found, name: state.name, state, groups: all.filter((group) => group.cameras.includes(state.id)).map((group) => group.name) }];
  });
});

// The filters.

export type StatusFilter = "all" | "password" | "problem" | "connected" | "recording";
export type StreamFilter = "all" | "main" | "sub";

export const STATUSES: { key: StatusFilter; title: string }[] = [
  { key: "all", title: "Any status" },
  { key: "password", title: "Needs a password" },
  { key: "problem", title: "Has a problem" },
  { key: "connected", title: "Connected" },
  { key: "recording", title: "Recording" },
];

export const STREAMS: { key: StreamFilter; title: string }[] = [
  { key: "all", title: "Any stream" },
  { key: "main", title: "Main stream" },
  { key: "sub", title: "Sub stream" },
];

/** What the search box holds: every word of it must be in a camera's name, address, username, path or groups. */
export const query = signal("");
/** The group whose cameras show: `null` for every camera, `""` for those in no group. */
export const groupFilter = signal<string | null>(null);
export const statusFilter = signal<StatusFilter>("all");
export const streamFilter = signal<StreamFilter>("all");

/** Whether the search or a filter leaves anything out. */
export const filtered = computed(() => query().trim() !== "" || groupFilter() !== null || statusFilter() !== "all" || streamFilter() !== "all");

export function clearFilters() {
  query.set("");
  groupFilter.set(null);
  statusFilter.set("all");
  streamFilter.set("all");
}

/** Where a camera's stream is: `192.0.2.10:554`, the port as saving gives it when it is blank. */
export const addressOf = (row: Saved) => (row.host === "" ? "" : `${row.host}:${row.rtspPort.trim() || "554"}`);

/** Which stream it plays, and how: `Ch 1 · Main · TCP`. */
export const streamOf = (row: Saved) => `Ch ${row.channel.trim() || "1"}  ·  ${row.subStream === 0 ? "Main" : "Sub"}  ·  ${row.useTcp ? "TCP" : "UDP"}`;

function matchesQuery(row: CameraRow, words: string[]): boolean {
  const text = [row.name, addressOf(row), row.user, row.rtspPath, ...row.groups].join(" ").toLowerCase();
  return words.every((word) => text.includes(word));
}

function matchesStatus(row: CameraRow, status: StatusFilter): boolean {
  switch (status) {
    case "all":
      return true;
    case "password":
      return !row.hasPassword;
    case "problem":
      return row.state.tone === "warn";
    case "connected":
      return row.state.connected;
    case "recording":
      return row.state.recording;
  }
}

function matchesGroup(row: CameraRow, group: string | null): boolean {
  if (group === null) return true;
  const all = groups();
  if (group === "") return !all.some((each) => each.cameras.includes(row.id));
  return all.some((each) => each.id === group && each.cameras.includes(row.id));
}

// The order.

export type SortKey = "name" | "address" | "stream" | "login" | "groups";

/** How two rows compare by `key`, before the direction is applied. */
function compare(a: CameraRow, b: CameraRow, key: SortKey): number {
  switch (key) {
    case "name":
      return byText(a.name, b.name);
    case "address":
      return byText(addressOf(a), addressOf(b));
    case "stream":
      return Number(a.channel || 1) - Number(b.channel || 1) || a.subStream - b.subStream || Number(b.useTcp) - Number(a.useTcp);
    case "login":
      // The cameras still waiting for a password first: they are the ones to fix.
      return Number(a.hasPassword) - Number(b.hasPassword) || byText(a.user, b.user);
    case "groups":
      return byText(a.groups.join(", "), b.groups.join(", "));
  }
}

/** Blanks go last either way, so they are found in one place. */
const blank = (row: CameraRow, key: SortKey) => (key === "address" && row.host === "") || (key === "groups" && row.groups.length === 0);

/** The rows the search and the filters keep, in the order the cameras were added. */
const kept = computed((): CameraRow[] => {
  const words = query().toLowerCase().split(/\s+/).filter((word) => word !== "");
  const group = groupFilter();
  const status = statusFilter();
  const stream = streamFilter();
  return rows().filter(
    (row) => matchesQuery(row, words) && matchesGroup(row, group) && matchesStatus(row, status) && (stream === "all" || (stream === "main") === (row.subStream === 0)),
  );
});

/** What the filters keep, for the header: "Outside, Needs a password", or `""` for nothing left out. */
export const summary = computed(() => {
  const group = groupFilter();
  const parts = [
    query().trim() === "" ? "" : `“${query().trim()}”`,
    group === null ? "" : group === "" ? "In no group" : (groups().find((each) => each.id === group)?.name ?? ""),
    statusFilter() === "all" ? "" : (STATUSES.find((each) => each.key === statusFilter())?.title ?? ""),
    streamFilter() === "all" ? "" : (STREAMS.find((each) => each.key === streamFilter())?.title ?? ""),
  ];
  return parts.filter((part) => part !== "").join(", ");
});

/** The table's order, selection and pages: `shown` is the rows the search and filters keep, in the chosen order. */
export const listing = listingOf({ kept, compare, blank, filters: () => [query(), groupFilter(), statusFilter(), streamFilter()] });

export const {
  sort,
  sortBy,
  shown,
  selected,
  chosen,
  pick,
  togglePage,
  selectAll,
  selectOnly,
  clearSelection,
  keepShown,
  pageSize,
  pageCount,
  currentPage,
  turnTo,
  onPage,
  pageSpan,
} = listing;

// Copies.

/** What a copy needs to know of the other cameras: the names taken, and the channels each address uses. */
export type Taken = { name: string; host: string; channel: string };

/** `host` as two cameras on one recorder share it, whatever its case or the spaces around it. */
const sameHost = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

/**
 * A copy of `form`, as Duplicate makes one: the same address, login, stream
 * and ports, on the next channel the address doesn't use yet, so copying one
 * of a recorder's channels makes the next; and named after it with "copy",
 * numbered when that name is taken ("Garage copy", "Garage copy 2"). `taken`
 * is every camera there is, the original included.
 */
export function copyOf<F extends Taken>(form: F, taken: Taken[]): F {
  const used = new Set(taken.filter((each) => sameHost(each.host, form.host)).map((each) => Number(each.channel.trim() || 1)));
  let channel = Number(form.channel.trim() || 1) + 1;
  while (used.has(channel)) channel += 1;

  return { ...form, name: copyName(form.name, taken.map((each) => each.name)), channel: String(channel) };
}

// Several cameras at once.

/** The settings several cameras can be given at once (`CameraChanges`): all but the name and the channel, each camera's own, and the password, which no one reads back. */
export type SharedField = Exclude<keyof CameraChanges, "password">;

export const SHARED_FIELDS: SharedField[] = ["host", "user", "rtspPort", "subStream", "rtspPath", "useTcp", "autoReconnect", "onvifPort", "dvripPort", "onvifPath"];

/** What every one of `list` has for each field: the value when they agree, absent when they differ. */
export type Common = { [K in SharedField]?: NonNullable<CameraChanges[K]> };

export function commonOf(list: Saved[]): Common {
  const common: Record<string, string | number | boolean> = {};
  const [first, ...rest] = list;
  if (!first) return common;
  for (const field of SHARED_FIELDS) {
    if (rest.every((each) => each[field] === first[field])) common[field] = first[field];
  }
  return common as Common;
}

/**
 * What to save: `edits` less what every camera already has (`common`), and
 * less an empty password, which keeps each camera's own. Nothing left means
 * nothing to save.
 */
export function changesToSave(edits: CameraChanges, common: Common): CameraChanges {
  const changes: Record<string, unknown> = {};
  for (const [field, value] of Object.entries(edits)) {
    if (value === undefined || value === null) continue;
    if (field === "password" ? value === "" : common[field as SharedField] === value) continue;
    changes[field] = value;
  }
  return changes as CameraChanges;
}
