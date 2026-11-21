import { type Accessor, computed, type Signal, signal, untracked } from "nsh";
import type { IconName, TileSize } from "../../../dp/ui";
import type { MenuEntries } from "./actions";
import { type Pick, pickOf } from "./listing";
import { cameraOf, detectionsLabel } from "./model";

export type Order = "newest" | "oldest";
export type Grouping = "month" | "day" | "camera" | "none";
export type Period = "all" | "today" | "week" | "month" | "year";

export const ORDERS: { key: Order; title: string; short: string }[] = [
  { key: "newest", title: "Newest first", short: "Newest" },
  { key: "oldest", title: "Oldest first", short: "Oldest" },
];

export const GROUPINGS: { key: Grouping; title: string; short: string }[] = [
  { key: "month", title: "Month", short: "By month" },
  { key: "day", title: "Day", short: "By day" },
  { key: "camera", title: "Camera", short: "By camera" },
  { key: "none", title: "None", short: "Ungrouped" },
];

/** The sizes a page shows its files at, as the View menu offers them. */
export const SIZES: { key: TileSize; title: string; icon: IconName }[] = [
  { key: "large", title: "Large", icon: "rectangle-horizontal" },
  { key: "medium", title: "Medium", icon: "layout-grid" },
  { key: "small", title: "Small", icon: "grid-3x3" },
  { key: "list", title: "List", icon: "list" },
];

export const PERIODS: { key: Period; title: string }[] = [
  { key: "all", title: "All time" },
  { key: "today", title: "Today" },
  { key: "week", title: "Last 7 days" },
  { key: "month", title: "Last 30 days" },
  { key: "year", title: "This year" },
];

/** A file a page lists: its name, its camera, what an event tagged it with, and that event's id (`""` for none). */
export type Item = { name: string; camera: string; tags: string[]; event: string };

/** Which files the Tag filter keeps: every file, those taken by hand, those an event saved, or those with one tag (`vehicle`). */
export type TagFilter = "all" | "hand" | "event" | (string & {});

/** A tag in words, and how many files have it. */
export type TagCount = { key: string; title: string; count: number };

/** What the Tag menu offers: how many files were taken by hand and saved by events, and each tag with its count. */
export type TagCounts = { hand: number; event: number; each: TagCount[] };

/** A tag in words: `Vehicle`, or `Motion`. */
export const tagTitle = (key: string) => (key === "motion" ? "Motion" : detectionsLabel([key]));

function tagged(item: Item, filter: TagFilter): boolean {
  if (filter === "all") return true;
  if (filter === "hand") return item.tags.length === 0;
  if (filter === "event") return item.tags.length > 0;
  return item.tags.includes(filter);
}

/** A run of a page's files under one heading. `title` is `""` when the page isn't grouped. */
export type Section<T> = { key: string; title: string; items: T[] };

/** A row of a page's list: a section's heading, or one row of its pictures. */
export type Row<T> = { kind: "heading"; key: string; section: string; title: string; count: number; open: boolean } | { kind: "tiles"; key: string; items: T[] };

/** The keys held at a press, as a press's event has them. */
export type Keys = { shift: boolean; control: boolean; platform: boolean };

/** How much of a section is selected, as its heading's checkbox shows it. */
export type Picked = "all" | "some" | "none";

/** What a page offers in its camera filter: a camera, by name, and how many of its files there are. */
export type CameraCount = { id: string; name: string; count: number };

/** One page's files and how they are narrowed, sorted and grouped. */
export type Library<T extends Item> = {
  order: Signal<Order>;
  grouping: Signal<Grouping>;
  /** `null` for every camera, `""` for files that name none. */
  cameraFilter: Signal<string | null>;
  tagFilter: Signal<TagFilter>;
  /** The event whose files show, by id; `null` for every file. */
  eventFilter: Signal<string | null>;
  period: Signal<Period>;
  /** Whether a filter leaves anything out. */
  filtered: Accessor<boolean>;
  clearFilters: () => void;
  /** The files the filters keep, in the chosen order. */
  visible: Accessor<T[]>;
  /** The visible files under their headings. */
  sections: Accessor<Section<T>[]>;
  /** The page's order, sections and all. */
  ordered: Accessor<T[]>;
  /** The cameras that have files, by name, with how many. */
  cameras: Accessor<CameraCount[]>;
  /** How many files were taken by hand and by events, and each tag's count. */
  tags: Accessor<TagCounts>;
  /** When a file was made, as much as its section's title doesn't say. */
  timeLabel: (item: T) => string;
  /** The dates the visible files span, then what the filters keep: "Oct 2 – Nov 7  ·  Driveway, Today". */
  summary: Accessor<string>;
  /** How big the pictures show, or a list. */
  size: Signal<TileSize>;
  /** The keys of the folded sections. */
  collapsed: Signal<string[]>;
  isCollapsed: (key: string) => boolean;
  toggleCollapsed: (key: string) => void;
  /** The names of the selected files; only files the page shows stay selected. */
  selected: Signal<string[]>;
  /** The selected files, in the page's order. */
  chosen: Accessor<T[]>;
  isSelected: (name: string) => boolean;
  /** Whether any file is selected: a press then selects rather than opens. */
  selecting: Accessor<boolean>;
  /** Picks the file `name` as `how` says; a range runs from the file last picked. */
  pick: (name: string, how: Pick) => void;
  /** A press of `item`: opens it while nothing is selected and no key is held, else picks it. */
  press: (item: T, keys: Keys, open: () => void) => void;
  sectionPicked: (key: string) => Picked;
  /** Selects every file of section `key`, or takes them out when all are. */
  toggleSection: (key: string) => void;
  /** Selects every file the filters keep. */
  selectAll: () => void;
  clearSelection: () => void;
  /** Drops from the selection what the page no longer shows. */
  keepShown: () => void;
};

const DETECTION_ORDER = ["person", "vehicle", "animal", "face", "package"];

/** The Tag filter `filter` in words; `all` as `every`. */
export function tagFilterLabel(filter: TagFilter, every: string): string {
  if (filter === "all") return every;
  if (filter === "hand") return "Taken by hand";
  if (filter === "event") return "From events";
  return tagTitle(filter);
}

/** What a page calls a camera, including those it no longer has. */
export function cameraName(id: string): string {
  if (id === "") return "Unknown camera";
  return cameraOf(id)?.name ?? "Deleted camera";
}

/** `1536` → `"1.5 KB"`, in binary units. */
export function sizeLabel(bytes: number): string {
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let size = bytes;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  if (unit === 0) return `${size} ${units[0]}`;
  return `${size < 10 ? size.toFixed(1) : Math.round(size)} ${units[unit]}`;
}

/** `1234` → `"1,234"`, in the user's locale. */
export const countLabel = (count: number) => count.toLocaleString();

const DAY = 86_400_000;

function since(chosen: Period, now: Date): number {
  switch (chosen) {
    case "all":
      return Number.NEGATIVE_INFINITY;
    case "today":
      return new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    case "week":
      return now.getTime() - 7 * DAY;
    case "month":
      return now.getTime() - 30 * DAY;
    case "year":
      return new Date(now.getFullYear(), 0, 1).getTime();
  }
}

const monthTitle = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" });
const weekdayTitle = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric" });
const weekdayYearTitle = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric", year: "numeric" });
const timeOnly = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const dayAndTime = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
const fullDate = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric", hour: "numeric", minute: "2-digit" });
const shortDay = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });
const shortDayYear = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" });

const sameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

function dayTitle(at: Date, now: Date): string {
  if (sameDay(at, now)) return "Today";
  if (sameDay(at, new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1))) return "Yesterday";
  return (at.getFullYear() === now.getFullYear() ? weekdayTitle : weekdayYearTitle).format(at);
}

function spanLabel(times: number[]): string {
  if (times.length === 0) return "";
  const first = new Date(Math.min(...times));
  const last = new Date(Math.max(...times));
  const format = first.getFullYear() === last.getFullYear() && last.getFullYear() === new Date().getFullYear() ? shortDay : shortDayYear;
  return sameDay(first, last) ? format.format(last) : `${format.format(first)} – ${format.format(last)}`;
}

/** Narrowing, sorting and grouping for one page's files; `timeOf` gives a file's time in milliseconds since 1970. */
export function libraryOf<T extends Item>(items: Accessor<T[]>, timeOf: (item: T) => number, firstGrouping: Grouping, firstSize: TileSize = "medium"): Library<T> {
  const order = signal<Order>("newest");
  const grouping = signal<Grouping>(firstGrouping);
  const cameraFilter = signal<string | null>(null);
  const tagFilter = signal<TagFilter>("all");
  const eventFilter = signal<string | null>(null);
  const period = signal<Period>("all");

  const filtered = computed(() => cameraFilter() !== null || tagFilter() !== "all" || eventFilter() !== null || period() !== "all");

  const clearFilters = () => {
    cameraFilter.set(null);
    tagFilter.set("all");
    eventFilter.set(null);
    period.set("all");
  };

  const visible = computed(() => {
    const from = since(period(), new Date());
    const camera = cameraFilter();
    const tag = tagFilter();
    const event = eventFilter();
    const kept = items().filter(
      (item) => timeOf(item) >= from && (camera === null || item.camera === camera) && tagged(item, tag) && (event === null || item.event === event),
    );
    const sign = order() === "newest" ? -1 : 1;
    // The name breaks ties, so two made in the same second keep one order.
    return kept.sort((a, b) => sign * (timeOf(a) - timeOf(b)) || sign * (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  });

  const sections = computed((): Section<T>[] => {
    const list = visible();
    const chosen = grouping();
    if (chosen === "none") return list.length > 0 ? [{ key: "all", title: "", items: list }] : [];

    const now = new Date();
    const found = new Map<string, Section<T>>();
    for (const item of list) {
      const at = new Date(timeOf(item));
      let key: string;
      let title: string;
      if (chosen === "camera") {
        key = `camera:${item.camera}`;
        title = cameraName(item.camera);
      } else if (chosen === "month") {
        key = `${at.getFullYear()}-${at.getMonth()}`;
        title = monthTitle.format(at);
      } else {
        key = `${at.getFullYear()}-${at.getMonth()}-${at.getDate()}`;
        title = dayTitle(at, now);
      }
      const section = found.get(key) ?? { key, title, items: [] };
      section.items.push(item);
      found.set(key, section);
    }

    const result = [...found.values()];
    // Date sections come out in order already; cameras go by name, unknown last.
    if (chosen === "camera") result.sort((a, b) => (a.key === "camera:") !== (b.key === "camera:") ? (a.key === "camera:" ? 1 : -1) : a.title.localeCompare(b.title));
    return result;
  });

  const ordered = computed(() => sections().flatMap((section) => section.items));

  const size = signal<TileSize>(firstSize);

  // Keys that no longer show stay, so their sections come back folded.
  const collapsed = signal<string[]>([]);
  const collapsedKeys = computed(() => new Set(collapsed()));
  const isCollapsed = (key: string) => collapsedKeys().has(key);
  const toggleCollapsed = (key: string) => collapsed.set(isCollapsed(key) ? collapsed().filter((each) => each !== key) : [...collapsed(), key]);

  const selected = signal<string[]>([]);
  const selectedNames = computed(() => new Set(selected()));
  let anchor: string | null = null;

  const chosen = computed(() => ordered().filter((item) => selectedNames().has(item.name)));
  const selecting = computed(() => chosen().length > 0);
  const isSelected = (name: string) => selectedNames().has(name);

  const pick = (name: string, how: Pick) => {
    const list = ordered().map((item) => item.name);
    if (how === "range" && anchor !== null && list.includes(anchor)) {
      const from = list.indexOf(anchor);
      const to = list.indexOf(name);
      // Added to what is selected: a range never drops a file picked by hand.
      selected.set([...new Set([...selected(), ...list.slice(Math.min(from, to), Math.max(from, to) + 1)])]);
      return;
    }
    anchor = name;
    if (how === "only") selected.set([name]);
    else selected.set(isSelected(name) ? selected().filter((each) => each !== name) : [...selected(), name]);
  };

  const press = (item: T, keys: Keys, open: () => void) => {
    const how = pickOf(keys);
    if (how === "only" && !selecting()) open();
    else pick(item.name, how === "only" ? "toggle" : how);
  };

  const namesIn = (key: string) => (sections().find((section) => section.key === key)?.items ?? []).map((item) => item.name);

  const sectionPicked = (key: string): Picked => {
    const names = namesIn(key);
    const count = names.filter(isSelected).length;
    return count === 0 ? "none" : count === names.length ? "all" : "some";
  };

  const toggleSection = (key: string) => {
    const names = namesIn(key);
    const all = names.length > 0 && names.every(isSelected);
    const those = new Set(names);
    selected.set(all ? selected().filter((name) => !those.has(name)) : [...new Set([...selected(), ...names])]);
  };

  const selectAll = () => selected.set(ordered().map((item) => item.name));

  const clearSelection = () => {
    selected.set([]);
    anchor = null;
  };

  const keepShown = () => {
    const names = new Set(ordered().map((item) => item.name));
    const now = untracked(selected);
    if (now.some((name) => !names.has(name))) selected.set(now.filter((name) => names.has(name)));
  };

  const cameras = computed(() => {
    const counts = new Map<string, number>();
    for (const item of items()) counts.set(item.camera, (counts.get(item.camera) ?? 0) + 1);
    return [...counts.entries()]
      .map(([id, count]) => ({ id, name: cameraName(id), count }))
      .sort((a, b) => (a.id === "") !== (b.id === "") ? (a.id === "" ? 1 : -1) : a.name.localeCompare(b.name));
  });

  const tags = computed((): TagCounts => {
    const counts = new Map<string, number>();
    let hand = 0;
    for (const item of items()) {
      if (item.tags.length === 0) hand += 1;
      for (const tag of item.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);
    }
    const order = [...DETECTION_ORDER, "motion"];
    const rank = (key: string) => (order.indexOf(key) < 0 ? order.length : order.indexOf(key));
    const each = [...counts.entries()]
      .map(([key, count]) => ({ key, title: tagTitle(key), count }))
      .sort((a, b) => rank(a.key) - rank(b.key) || a.title.localeCompare(b.title));
    return { hand, event: items().length - hand, each };
  });

  const timeLabel = (item: T) => {
    const at = new Date(timeOf(item));
    switch (grouping()) {
      case "day":
        return timeOnly.format(at);
      case "month":
        return dayAndTime.format(at);
      default:
        return at.getFullYear() === new Date().getFullYear() ? dayAndTime.format(at) : fullDate.format(at);
    }
  };

  const summary = computed(() => {
    const camera = cameraFilter();
    const narrowed = [
      eventFilter() === null ? "" : "One event",
      camera === null ? "" : cameraName(camera),
      tagFilterLabel(tagFilter(), ""),
      period() === "all" ? "" : (PERIODS.find((each) => each.key === period())?.title ?? ""),
    ];
    return [spanLabel(visible().map(timeOf)), narrowed.filter((part) => part !== "").join(", ")].filter((part) => part !== "").join("  ·  ");
  });

  return {
    order,
    grouping,
    cameraFilter,
    tagFilter,
    eventFilter,
    period,
    filtered,
    clearFilters,
    visible,
    sections,
    ordered,
    cameras,
    tags,
    timeLabel,
    summary,
    size,
    collapsed,
    isCollapsed,
    toggleCollapsed,
    selected,
    chosen,
    isSelected,
    selecting,
    pick,
    press,
    sectionPicked,
    toggleSection,
    selectAll,
    clearSelection,
    keepShown,
  };
}

/** The sections as the list's rows; an ungrouped section never folds, having no heading to open it by. */
export function rowsOf<T extends Item>(list: Section<T>[], columns: number, folded: (key: string) => boolean = () => false): Row<T>[] {
  const rows: Row<T>[] = [];
  for (const section of list) {
    if (section.title !== "") {
      const open = !folded(section.key);
      rows.push({ kind: "heading", key: `heading:${section.key}`, section: section.key, title: section.title, count: section.items.length, open });
      if (!open) continue;
    }
    for (let start = 0; start < section.items.length; start += columns) {
      const items = section.items.slice(start, start + columns);
      rows.push({ kind: "tiles", key: `tiles:${section.key}:${items[0].name}`, items });
    }
  }
  return rows;
}

/** In a file's menu: narrow the page to its camera, or, when it is already, show them all again. */
export function cameraMenuEntry<T extends Item>(library: Library<T>, item: T) {
  return library.cameraFilter() === item.camera
    ? { label: "Show every camera", onSelect: () => library.cameraFilter.set(null) }
    : { label: `Only ${cameraName(item.camera)}`, onSelect: () => library.cameraFilter.set(item.camera) };
}

/** What deletes a selection of `count` says: `Delete (3)`. */
export const deleteLabel = (count: number) => `Delete (${countLabel(count)})`;

/** A file's menu: for one of several selected, what can be done to them all, else its own. */
export function menuOf<T extends Item>(library: Library<T>, item: T, own: () => MenuEntries, remove: (items: T[]) => void): MenuEntries {
  const chosen = library.chosen();
  if (!library.isSelected(item.name) || chosen.length < 2) return own();
  return [{ label: `${deleteLabel(chosen.length)}…`, onSelect: () => remove(chosen) }, { type: "separator" as const }, { label: "Clear selection", onSelect: library.clearSelection }];
}

/** Deletes each of `names` in turn, so a failure stops none of the rest; resolves with those gone and the first failure. */
export async function deleteEach(names: string[], remove: (name: string) => Promise<unknown>): Promise<{ gone: string[]; failure?: unknown }> {
  const gone: string[] = [];
  let failure: unknown;
  for (const name of names) {
    try {
      await remove(name);
      gone.push(name);
    } catch (error) {
      failure ??= error;
    }
  }
  return { gone, failure };
}
