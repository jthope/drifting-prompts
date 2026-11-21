import { openUrl } from "nsh";
import { type ProjectInfo, Projects, type ViewMode } from "mdviewer/native";
import { confirmDelete, type MenuEntries, notify } from "../../../dp/ui";
import { linkTarget } from "./document";
import {
  currentMatch,
  defaultMode,
  filesOpen,
  found,
  goToHeading,
  headings,
  listing,
  openPath,
  project,
  reading,
  resultsInFiles,
  resultsInFront,
  resultsTab,
  searchOpen,
  searchText,
  settingsOpen,
  setTabs,
  showMatch,
  tabSet,
} from "./model";
import { rowOf, stepFrom } from "./search";
import {
  bringToFront,
  close,
  closeOthers,
  closeToTheRight,
  closeUnpinned,
  keep,
  move,
  type OpenAs,
  open,
  openAll,
  setMode,
  setPinned,
  step,
  tabOf,
} from "./tabs";

/** Shows why something failed in a toast. */
export function failed(error: unknown) {
  const message = (error as { message?: string })?.message;
  notify(String(message ?? error), "warn");
}

/** Picks a folder and opens it as a project. */
export function chooseFolder() {
  Projects.choose().catch(failed);
}

/** Opens the first dropped folder as a project. */
export function openDropped(uris: string[]) {
  Projects.openDropped(uris).catch(failed);
}

/** Opens the project of folder `path`. */
export function openProject(path: string) {
  Projects.select(path).catch(failed);
}

/** Shows New, with no project open. */
export function showNew() {
  Projects.select("").catch(failed);
}

/** Opens Settings. */
export function showSettings() {
  settingsOpen.set(true);
}

/** Opens file `path` of the open project in front, as the preview or kept. */
export function openFile(path: string, as: OpenAs = "preview") {
  if (!project()) return;
  resultsInFront.set(false);
  setTabs(open(tabSet(), path, as, defaultMode()));
}

/** Opens `paths` in tabs of their own, the first in front. */
export function openFiles(paths: string[]) {
  resultsInFront.set(false);
  setTabs(openAll(tabSet(), paths, defaultMode()));
}

/** Opens the search's results in a tab of their own, in front. */
export function openResultsTab() {
  resultsTab.set(true);
  resultsInFront.set(true);
  searchOpen.set(false);
}

/** Closes the results' tab. */
export function closeResultsTab() {
  resultsTab.set(false);
  resultsInFront.set(false);
}

/** Closes the tab in front. */
export function closeFront() {
  if (resultsInFront()) closeResultsTab();
  else closeTab(openPath());
}

const closed: string[] = [];

/** Closes the tab of `path`. */
export function closeTab(path: string) {
  if (!tabOf(tabSet(), path)) return;
  closed.push(path);
  setTabs(close(tabSet(), path));
}

/** Opens the file closed last that is still there, kept. */
export function reopenClosed() {
  const present = new Set(listing()?.files.map((file) => file.path) ?? []);
  for (let path = closed.pop(); path !== undefined; path = closed.pop()) {
    if (present.has(path)) {
      openFile(path, "keep");
      return;
    }
  }
}

/** Brings the tab of `path` to the front. */
export function showTab(path: string) {
  resultsInFront.set(false);
  setTabs(bringToFront(tabSet(), path));
}

/** Brings the next tab (`by` 1) or the one before (-1) to the front, wrapping around. */
export function stepTab(by: number) {
  const next = step(tabSet(), by);
  if (next !== "") showTab(next);
}

/** Keeps the tab of `path`, so it stops being the preview. */
export const keepTab = (path: string) => setTabs(keep(tabSet(), path));

/** Pins or unpins the tab of `path`. */
export const pinTab = (path: string, pinned: boolean) => setTabs(setPinned(tabSet(), path, pinned));

/** Moves the tab at `from` to `to`. */
export function moveTab(from: number, to: number): Promise<boolean> {
  const before = tabSet();
  setTabs(move(before, from, to));
  return Promise.resolve(tabSet() !== before);
}

/** Takes the reader to the next (`by` 1) or previous (-1) match across the project's files. */
export function stepMatch(by: 1 | -1) {
  const results = found();
  if (!results) return;
  const order = listing()?.files.map((file) => file.path) ?? [];
  const next = stepFrom(results, currentMatch(), openPath(), order, by);
  if (!next) return;
  resultsInFront.set(false);
  currentMatch.set(next);
  if (next.file !== openPath()) openFile(next.file);
}

/** Ends the search. */
export function clearSearch() {
  searchText.set("");
}

/** Goes to a row of the search's results: a match, or a file's first match. */
export function goToResult(id: string) {
  const row = rowOf(id);
  resultsInFront.set(false);
  if (row?.kind === "match") showMatch(row.project, row.file, row.index, (file) => openFile(file));
  else if (row?.kind === "file") showMatch(row.project, row.file, 0, (file) => openFile(file));
}

/** Shows the search's results in the files panel. */
export function showResultsInFiles() {
  resultsInFiles.set(true);
  filesOpen.set(true);
  searchOpen.set(false);
}

/** Shows the file in front as `mode`. */
export function setModeInFront(mode: ViewMode) {
  setTabs(setMode(tabSet(), openPath(), mode));
}

async function removeProject(shown: ProjectInfo) {
  const sure = await confirmDelete({
    title: `Remove ${shown.name}?`,
    description: "It leaves the list of projects. Its folder and its files stay as they are.",
    action: "Remove",
  });
  if (sure) Projects.remove(shown.path).catch(failed);
}

/** A project's menu. */
export function projectMenu(shown: ProjectInfo): MenuEntries {
  return [
    { label: "Open", onSelect: () => openProject(shown.path) },
    { label: "Show in Explorer", onSelect: () => Projects.showInExplorer(shown.path, "").catch(failed) },
    { type: "separator" as const },
    { label: "Remove from projects…", onSelect: () => removeProject(shown) },
  ];
}

function showInExplorer(path: string) {
  const shown = project();
  if (shown) Projects.showInExplorer(shown.path, path).catch(failed);
}

/** A file's menu; on one of several `picked` files, it acts on them all. */
export function fileMenu(path: string, picked: string[]): MenuEntries {
  if (picked.length > 1 && picked.includes(path)) {
    return [{ label: `Open ${picked.length} files in tabs`, onSelect: () => openFiles(picked) }];
  }
  return [
    { label: "Open", onSelect: () => openFile(path) },
    { label: "Open in a new tab", onSelect: () => openFile(path, "keep") },
    { type: "separator" as const },
    { label: "Show in Explorer", onSelect: () => showInExplorer(path) },
  ];
}

/** A tab's menu. */
export function tabMenu(path: string): MenuEntries {
  const tab = tabOf(tabSet(), path);
  if (!tab) return [];
  return [
    tab.preview && { label: "Keep open", onSelect: () => keepTab(path) },
    { label: tab.pinned ? "Unpin" : "Pin", onSelect: () => pinTab(path, !tab.pinned) },
    { type: "separator" as const },
    { label: "Close", onSelect: () => closeTab(path) },
    { label: "Close others", onSelect: () => setTabs(closeOthers(tabSet(), path)) },
    { label: "Close to the right", onSelect: () => setTabs(closeToTheRight(tabSet(), path)) },
    { label: "Close all but pinned", onSelect: () => setTabs(closeUnpinned(tabSet())) },
    { type: "separator" as const },
    {
      label: "Show in files",
      onSelect: () => {
        showTab(path);
        filesOpen.set(true);
      },
    },
    { label: "Show in Explorer", onSelect: () => showInExplorer(path) },
  ];
}

/** A folder breadcrumb's menu: the files directly in `folder`. */
export function folderMenu(folder: string): MenuEntries {
  return (listing()?.files ?? [])
    .filter((file) => file.folder === folder)
    .map((file) => ({ label: file.name, checked: file.path === openPath(), onSelect: () => openFile(file.path) }));
}

/** The heading breadcrumb's menu: the file's headings. */
export function headingMenu(): MenuEntries {
  return headings().map((heading, index) => ({ label: heading.title === "" ? "Untitled" : heading.title, checked: index === reading(), onSelect: () => goToHeading(index) }));
}

/** Follows a link pressed on the page; `anchors` are the page's, in the outline's order. */
export function followLink(url: string, anchors: string[], newTab = false) {
  const target = linkTarget(openPath(), url);
  switch (target.kind) {
    case "anchor": {
      const index = anchors.indexOf(target.anchor);
      if (index >= 0) goToHeading(index);
      return;
    }
    case "file":
      if (listing()?.files.some((file) => file.path === target.path)) openFile(target.path, newTab ? "keep" : "preview");
      else notify(`${target.path} isn't a Markdown file of this project.`, "warn");
      return;
    case "outside":
      openUrl(target.url).catch(failed);
      return;
    case "none":
      return;
  }
}
