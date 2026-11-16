// What the buttons, rows, tabs, keys and menus do. A change to the projects
// goes through Rust, which saves it in `projects.json`; the projects' listener
// (`listen` in `model.ts`) then shows it everywhere. The tabs change here and
// are saved as they do (`setTabs`). A failure is a toast that says what to do.
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

/** Says why something failed, as Rust worded it. */
export function failed(error: unknown) {
  const message = (error as { message?: string })?.message;
  notify(String(message ?? error), "warn");
}

/** Asks for a folder with the system's picker and opens it as a project. */
export function chooseFolder() {
  Projects.choose().catch(failed);
}

/** Opens the first folder of what was dropped on the window (`onFileDrop`'s `uris`) as a project. */
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

/** Opens Settings in its dialog over whatever is open, on the section it was left on. */
export function showSettings() {
  settingsOpen.set(true);
}

/** Opens file `path` of the open project in front: as the preview (a single press, a link), or kept in a tab of its own. */
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

/** Gives the search's results a tab of their own, in front, and closes the drop-down. */
export function openResultsTab() {
  resultsTab.set(true);
  resultsInFront.set(true);
  searchOpen.set(false);
}

/** Closes the results' tab; the file behind it comes back to the front. */
export function closeResultsTab() {
  resultsTab.set(false);
  resultsInFront.set(false);
}

/** Closes the tab in front: the results' while they are in front, else the file's (Ctrl+W). */
export function closeFront() {
  if (resultsInFront()) closeResultsTab();
  else closeTab(openPath());
}

/** The files closed in this window, the last closed last, for Ctrl+Shift+T. */
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

/** Brings the next tab (`by` 1) or the one before (`by` -1) to the front, around the ends. */
export function stepTab(by: number) {
  const next = step(tabSet(), by);
  if (next !== "") showTab(next);
}

/** Keeps the tab of `path`: it stops being the preview. */
export const keepTab = (path: string) => setTabs(keep(tabSet(), path));

/** Pins the tab of `path` (`pinned`), last of the pinned ones, or unpins it, first of the others. */
export const pinTab = (path: string, pinned: boolean) => setTabs(setPinned(tabSet(), path, pinned));

/** Moves the tab at `from` to `to`, among the pinned or the others as it is. */
export function moveTab(from: number, to: number): Promise<boolean> {
  const before = tabSet();
  setTabs(move(before, from, to));
  return Promise.resolve(tabSet() !== before);
}

/**
 * Takes the reader to the next match of the search (`by` 1, F3) or the one
 * before (-1, Shift+F3), across the project's files: a match in another file
 * opens it, in its tab if it has one, else as the preview.
 */
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

/** Ends the search: the field empties, and the marks go. */
export function clearSearch() {
  searchText.set("");
}

/**
 * Goes to a row of the search's results (`rowId` in `search.ts`): a match, or
 * a file's first match. One in another project opens that project first.
 */
export function goToResult(id: string) {
  const row = rowOf(id);
  resultsInFront.set(false);
  if (row?.kind === "match") showMatch(row.project, row.file, row.index, (file) => openFile(file));
  else if (row?.kind === "file") showMatch(row.project, row.file, 0, (file) => openFile(file));
}

/** Shows the search's results in the files panel, which opens if it was hidden, and closes the drop-down. */
export function showResultsInFiles() {
  resultsInFiles.set(true);
  filesOpen.set(true);
  searchOpen.set(false);
}

/** Shows the file in front as `mode`; new tabs keep showing as Settings says. */
export function setModeInFront(mode: ViewMode) {
  setTabs(setMode(tabSet(), openPath(), mode));
}

/** Asks, then takes `shown` off the projects. Its folder and files stay as they are. */
async function removeProject(shown: ProjectInfo) {
  const sure = await confirmDelete({
    title: `Remove ${shown.name}?`,
    description: "It leaves the list of projects. Its folder and its files stay as they are.",
    action: "Remove",
  });
  if (sure) Projects.remove(shown.path).catch(failed);
}

/** What a project's menu offers, in the sidebar. */
export function projectMenu(shown: ProjectInfo): MenuEntries {
  return [
    { label: "Open", onSelect: () => openProject(shown.path) },
    { label: "Show in Explorer", onSelect: () => Projects.showInExplorer(shown.path, "").catch(failed) },
    { type: "separator" as const },
    { label: "Remove from projects…", onSelect: () => removeProject(shown) },
  ];
}

/** Explorer, on file `path` of the open project. */
function showInExplorer(path: string) {
  const shown = project();
  if (shown) Projects.showInExplorer(shown.path, path).catch(failed);
}

/**
 * What a file's menu offers, in the files panel. `picked` are the files
 * picked there: a right click on one of several acts on them all.
 */
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

/** What a tab's menu offers. */
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

/**
 * What a folder's breadcrumb offers, under the tabs: the files in folder
 * `folder` of the open project (`""` for its own), not those of the folders
 * under it, the one in front checked. A file opens as the preview, as a press
 * in the files panel opens it.
 */
export function folderMenu(folder: string): MenuEntries {
  return (listing()?.files ?? [])
    .filter((file) => file.folder === folder)
    .map((file) => ({ label: file.name, checked: file.path === openPath(), onSelect: () => openFile(file.path) }));
}

/** What the breadcrumb of the heading the reader is under offers: the file's headings, that one checked; one goes there, as the outline does. */
export function headingMenu(): MenuEntries {
  return headings().map((heading, index) => ({ label: heading.title === "" ? "Untitled" : heading.title, checked: index === reading(), onSelect: () => goToHeading(index) }));
}

/**
 * Follows a link pressed on the page: a heading of this file goes there
 * (`anchors` are the page's, in the outline's order), another file of the
 * project opens as the preview, or in a tab of its own with `newTab` (a
 * Ctrl+click), and anything else opens outside the app.
 */
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
