// Each function returns the same object when nothing changes, so `setTabs` saves only real changes.
import type { TabInfo, ViewMode } from "mdviewer/native";

/** A file open in a tab. */
export type Tab = {
  /** The file's path in the project. */
  path: string;
  /** Kept first, and left open by closing the others. */
  pinned: boolean;
  /** The tab the next file opened with a single press takes over. */
  preview: boolean;
  /** How it shows. */
  mode: ViewMode;
};

/** The open project's tabs, pinned first, and the file in front (`""` with none). */
export type TabSet = { tabs: Tab[]; front: string };

/** No tabs. */
export const NO_TABS: TabSet = { tabs: [], front: "" };

/** How a file opens: as the preview, or kept in a tab of its own. */
export type OpenAs = "preview" | "keep";

/** How many tabs are pinned. */
export const pinnedCount = (tabs: Tab[]) => tabs.filter((tab) => tab.pinned).length;

const grouped = (tabs: Tab[]) => [...tabs.filter((tab) => tab.pinned), ...tabs.filter((tab) => !tab.pinned)];

/** The tab of `path`, if one is open. */
export const tabOf = (set: TabSet, path: string) => set.tabs.find((tab) => tab.path === path);

function changed(set: TabSet, path: string, change: (tab: Tab) => Tab): TabSet {
  const at = set.tabs.findIndex((tab) => tab.path === path);
  if (at < 0) return set;
  const tabs = set.tabs.slice();
  tabs[at] = change(tabs[at]);
  return { ...set, tabs };
}

/** Opens `path` in front; a preview takes over the preview tab, else a new tab opens after the one in front. */
export function open(set: TabSet, path: string, as: OpenAs, mode: ViewMode): TabSet {
  if (tabOf(set, path)) {
    const shown = as === "keep" ? keep(set, path) : set;
    return shown.front === path ? shown : { ...shown, front: path };
  }
  const tab: Tab = { path, pinned: false, preview: as === "preview", mode };
  const preview = set.tabs.findIndex((each) => each.preview);
  if (as === "preview" && preview >= 0) {
    const tabs = set.tabs.slice();
    tabs[preview] = tab;
    return { tabs, front: path };
  }
  const after = set.tabs.findIndex((each) => each.path === set.front) + 1;
  const at = Math.max(after, pinnedCount(set.tabs));
  return { tabs: [...set.tabs.slice(0, at), tab, ...set.tabs.slice(at)], front: path };
}

/** Opens `paths` in tabs of their own, kept, the first in front. */
export function openAll(set: TabSet, paths: string[], mode: ViewMode): TabSet {
  if (paths.length === 0) return set;
  let next = set;
  for (const path of paths) next = open(next, path, "keep", mode);
  return next.front === paths[0] ? next : { ...next, front: paths[0] };
}

/** Brings the tab of `path` to the front. */
export function bringToFront(set: TabSet, path: string): TabSet {
  return set.front === path || !tabOf(set, path) ? set : { ...set, front: path };
}

/** Keeps the tab of `path`, so it stops being the preview. */
export function keep(set: TabSet, path: string): TabSet {
  return tabOf(set, path)?.preview ? changed(set, path, (tab) => ({ ...tab, preview: false })) : set;
}

/** The front moves to the nearest tab left open, after it, else before. */
function closing(set: TabSet, stays: (tab: Tab, index: number) => boolean): TabSet {
  const tabs = set.tabs.filter(stays);
  if (tabs.length === set.tabs.length) return set;
  if (tabs.some((tab) => tab.path === set.front)) return { tabs, front: set.front };
  const was = set.tabs.findIndex((tab) => tab.path === set.front);
  const after = set.tabs.slice(was + 1).find((tab) => tabs.includes(tab));
  const before = set.tabs.slice(0, Math.max(was, 0)).reverse().find((tab) => tabs.includes(tab));
  return { tabs, front: (after ?? before)?.path ?? "" };
}

/** Closes the tab of `path`, pinned or not. */
export const close = (set: TabSet, path: string) => closing(set, (tab) => tab.path !== path);

/** Closes every tab but `path`'s and the pinned ones, and brings `path`'s to the front. */
export const closeOthers = (set: TabSet, path: string) => bringToFront(closing(set, (tab) => tab.pinned || tab.path === path), path);

/** Closes the tabs after `path`'s that aren't pinned. */
export function closeToTheRight(set: TabSet, path: string): TabSet {
  const at = set.tabs.findIndex((tab) => tab.path === path);
  return at < 0 ? set : closing(set, (tab, index) => tab.pinned || index <= at);
}

/** Closes every tab that isn't pinned. */
export const closeUnpinned = (set: TabSet) => closing(set, (tab) => tab.pinned);

/** Pins the tab of `path`, last of the pinned, or unpins it, first of the others. */
export function setPinned(set: TabSet, path: string, pinned: boolean): TabSet {
  const tab = tabOf(set, path);
  if (!tab || tab.pinned === pinned) return set;
  const rest = set.tabs.filter((each) => each !== tab);
  const moved: Tab = { ...tab, pinned, preview: false };
  const at = pinnedCount(rest);
  return { ...set, tabs: [...rest.slice(0, at), moved, ...rest.slice(at)] };
}

/** The indexes the tab at `index` can be dragged between: among the pinned or among the others. */
export function movesWithin(tabs: Tab[], index: number): [number, number] {
  const pinned = pinnedCount(tabs);
  return tabs[index]?.pinned ? [0, pinned - 1] : [pinned, tabs.length - 1];
}

/** Moves the tab at `from` to `to`, kept among the pinned or the others as it is. */
export function move(set: TabSet, from: number, to: number): TabSet {
  if (from < 0 || from >= set.tabs.length) return set;
  const [low, high] = movesWithin(set.tabs, from);
  const target = Math.max(low, Math.min(high, to));
  if (target === from) return set;
  const tabs = set.tabs.slice();
  const [tab] = tabs.splice(from, 1);
  tabs.splice(target, 0, tab);
  return { ...set, tabs };
}

/** Shows the tab of `path` as `mode`. */
export function setMode(set: TabSet, path: string, mode: ViewMode): TabSet {
  return tabOf(set, path)?.mode === mode ? set : changed(set, path, (tab) => ({ ...tab, mode }));
}

/** Closes the tabs whose files aren't `present`. */
export const prune = (set: TabSet, present: (path: string) => boolean) => closing(set, (tab) => present(tab.path));

/** The tab `by` places from the one in front, wrapping around; `""` with none. */
export function step(set: TabSet, by: number): string {
  const count = set.tabs.length;
  if (count === 0) return "";
  const at = Math.max(0, set.tabs.findIndex((tab) => tab.path === set.front));
  return set.tabs[(((at + by) % count) + count) % count].path;
}

/** A tab's name, and its folder when another tab has a file of the same name. */
export function label(tabs: Tab[], tab: Tab): { name: string; folder: string } {
  const name = tab.path.split("/").pop() ?? tab.path;
  const shared = tabs.some((each) => each !== tab && (each.path.split("/").pop() ?? each.path) === name);
  return { name, folder: shared ? tab.path.split("/").slice(0, -1).join("/") : "" };
}

/** A step of the way to the file in front. */
export type PathStep = {
  /** The project's own folder, a folder in it, or the file. */
  kind: "project" | "folder" | "file";
  /** Its name. */
  label: string;
  /** Its folder in the project; for the file, the folder it is in. */
  folder: string;
};

/** The way to file `path`: the project, each folder, then the file. */
export function pathSteps(project: string, path: string): PathStep[] {
  const folders = path.split("/");
  const name = folders.pop() ?? path;
  return [
    { kind: "project", label: project, folder: "" },
    ...folders.map((folder, index): PathStep => ({ kind: "folder", label: folder, folder: folders.slice(0, index + 1).join("/") })),
    { kind: "file", label: name, folder: folders.join("/") },
  ];
}

/** The tabs Rust saved for a project, `front` in front when it is one of them. */
export function restore(saved: TabInfo[], front: string): TabSet {
  const tabs = grouped(saved.map((tab) => ({ path: tab.file, pinned: tab.pinned, preview: tab.preview, mode: tab.mode })));
  // One preview at most: a hand-edited file may have more.
  let previews = 0;
  const once = tabs.map((tab) => (tab.preview && previews++ > 0 ? { ...tab, preview: false } : tab));
  return { tabs: once, front: once.some((tab) => tab.path === front) ? front : (once[0]?.path ?? "") };
}

/** The tabs as Rust saves them. */
export const saved = (set: TabSet): TabInfo[] => set.tabs.map((tab) => ({ file: tab.path, pinned: tab.pinned, preview: tab.preview, mode: tab.mode }));
