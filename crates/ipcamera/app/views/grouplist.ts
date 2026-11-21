import { computed, signal } from "nsh";
import type { CameraState, GroupInfo, StatusTone } from "ipcamera/native";
import { countLabel } from "./library";
import { byText, listingOf } from "./listing";
import { groups, states } from "./model";

/** A group as a row of the table: its cameras that exist, in its order, and how many are connected, in trouble, recording or waiting for a password. */
export type GroupRow = {
  id: string;
  name: string;
  group: GroupInfo;
  cameras: CameraState[];
  connected: number;
  problems: number;
  recording: number;
  waiting: number;
};

/** `group` as a row, its cameras found in `byId`. */
export function rowOf(group: GroupInfo, byId: Map<string, CameraState>): GroupRow {
  const cameras = group.cameras.flatMap((id) => byId.get(id) ?? []);
  const count = (test: (camera: CameraState) => boolean) => cameras.filter(test).length;
  return {
    id: group.id,
    name: group.name,
    group,
    cameras,
    connected: count((camera) => camera.connected),
    problems: count((camera) => camera.tone === "warn"),
    recording: count((camera) => camera.recording),
    waiting: count((camera) => !camera.ready),
  };
}

/** Every group, in the order the sidebar lists them, with what its cameras are doing. */
export const rows = computed((): GroupRow[] => {
  const byId = new Map(states().map((camera) => [camera.id, camera]));
  return groups().map((group) => rowOf(group, byId));
});

/** The names of a group's cameras, in its order: "Porch, Yard". */
export const membersOf = (row: GroupRow) => row.cameras.map((camera) => camera.name).join(", ");

/** What a group's cameras are doing, as its row says it, and how that reads. */
export function statusOf(row: GroupRow): { text: string; tone: StatusTone } {
  const count = row.cameras.length;
  if (count === 0) return { text: "No cameras", tone: "normal" };
  if (row.problems > 0) return { text: row.problems === 1 ? "1 has a problem" : `${countLabel(row.problems)} have a problem`, tone: "warn" };
  return { text: `${countLabel(row.connected)} of ${countLabel(count)} connected`, tone: row.connected > 0 ? "ok" : "normal" };
}

export type StatusFilter = "all" | "problem" | "password" | "recording" | "empty";

export const STATUSES: { key: StatusFilter; title: string }[] = [
  { key: "all", title: "Any status" },
  { key: "problem", title: "Has a problem" },
  { key: "password", title: "Needs a password" },
  { key: "recording", title: "Recording" },
  { key: "empty", title: "Empty" },
];

/** What the search box holds: every word of it must be in a group's name or the name of one of its cameras. */
export const query = signal("");
/** The camera whose groups show: `null` for every group. */
export const cameraFilter = signal<string | null>(null);
export const statusFilter = signal<StatusFilter>("all");

/** Whether the search or a filter leaves anything out. */
export const filtered = computed(() => query().trim() !== "" || cameraFilter() !== null || statusFilter() !== "all");

export function clearFilters() {
  query.set("");
  cameraFilter.set(null);
  statusFilter.set("all");
}

function matchesQuery(row: GroupRow, words: string[]): boolean {
  const text = [row.name, ...row.cameras.map((camera) => camera.name)].join(" ").toLowerCase();
  return words.every((word) => text.includes(word));
}

function matchesStatus(row: GroupRow, status: StatusFilter): boolean {
  switch (status) {
    case "all":
      return true;
    case "problem":
      return row.problems > 0;
    case "password":
      return row.waiting > 0;
    case "recording":
      return row.recording > 0;
    case "empty":
      return row.cameras.length === 0;
  }
}

const cameraName = (id: string) => states().find((camera) => camera.id === id)?.name ?? "";

/** What the filters keep, for the header: "With Porch, Has a problem", or `""` for nothing left out. */
export const summary = computed(() => {
  const camera = cameraFilter();
  const parts = [
    query().trim() === "" ? "" : `“${query().trim()}”`,
    camera === null || cameraName(camera) === "" ? "" : `With ${cameraName(camera)}`,
    statusFilter() === "all" ? "" : (STATUSES.find((each) => each.key === statusFilter())?.title ?? ""),
  ];
  return parts.filter((part) => part !== "").join(", ");
});

const kept = computed((): GroupRow[] => {
  const words = query().toLowerCase().split(/\s+/).filter((word) => word !== "");
  const camera = cameraFilter();
  const status = statusFilter();
  return rows().filter((row) => matchesQuery(row, words) && (camera === null || row.group.cameras.includes(camera)) && matchesStatus(row, status));
});

export type SortKey = "name" | "cameras" | "members" | "status";

const share = (row: GroupRow) => (row.cameras.length === 0 ? 0 : row.connected / row.cameras.length);

function compare(a: GroupRow, b: GroupRow, key: SortKey): number {
  switch (key) {
    case "name":
      return byText(a.name, b.name);
    case "cameras":
      return a.cameras.length - b.cameras.length;
    case "members":
      return byText(membersOf(a), membersOf(b));
    case "status":
      return b.problems - a.problems || share(a) - share(b);
  }
}

const blank = (row: GroupRow, key: SortKey) => (key === "members" || key === "status") && row.cameras.length === 0;

/** The table's order, selection and pages. */
export const listing = listingOf({ kept, compare, blank, filters: () => [query(), cameraFilter(), statusFilter()] });

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

/** How many cameras are in at least one group, and how many in none. */
export const grouping = computed(() => {
  const grouped = new Set(groups().flatMap((group) => group.cameras));
  const inSome = states().filter((camera) => grouped.has(camera.id)).length;
  return { grouped: inSome, ungrouped: states().length - inSome };
});

/** How many groups have no cameras. */
export const emptyCount = computed(() => rows().filter((row) => row.cameras.length === 0).length);
