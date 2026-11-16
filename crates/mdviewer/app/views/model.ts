// What every region of the window reads: the projects, the page shown, the
// open project's files, its tabs and the file in front, where the reader is in
// it, and what the title bar's search found. Each lives in a signal here, kept
// current by Rust's listeners (`listen`), so a region reads it rather than
// asking Rust itself.
//
// The project open is Rust's (`projects.json`'s `selected`), so it outlives the
// window; so are its tabs and the one in front, saved as they change
// (`setTabs`). The page (the project or New), whether Settings shows over it,
// where the reader is and the search are the window's own.
import { cleanup, computed, effect, signal, untracked } from "nsh";
import {
  Documents,
  type FileList,
  Files,
  type FilesView,
  type HeadingInfo,
  type MatchInfo,
  type OpenFile,
  type ProjectInfo,
  type ProjectList,
  Projects,
  Search,
  type SearchPlace,
  type SearchQuery,
  type SearchResults,
  type TabOverflow,
  type ViewMode,
} from "mdviewer/native";
import { countIn, headingCounts, inProject, type MatchPlace, matchesIn, placesFor, type SearchScope } from "./search";
import { NO_TABS, open, prune, restore, saved, type TabSet, tabOf } from "./tabs";
import { fileToOpen } from "./tree";

/** How long typing in the search field pauses before the project is searched for it, in milliseconds. */
const SEARCH_PAUSE = 200;

/** The projects, and the one open. */
export const projects = signal<ProjectList>(Projects.list());

/** What the window shows: a project, or New (no project open). Settings opens over either, in a dialog. */
export type Page = "project" | "new";

export const page = computed<Page>(() => (projects().selected !== "" ? "project" : "new"));

/** A section of Settings. */
export type SettingsPart = "files" | "appearance";

/** Whether Settings shows, in its dialog over the project or New. */
export const settingsOpen = signal(false);

/** The section of Settings shown. It outlives the dialog, so opening Settings again finds the section it was left on. */
export const settingsSection = signal<SettingsPart>("files");

/** The open project; `undefined` while New shows. A new value each time the list changes, its tabs included. */
export const project = computed<ProjectInfo | undefined>(() => projects().projects.find((each) => each.path === projects().selected));

/**
 * The open project's folder; `""` while New shows. What starts a project's
 * listeners over: it changes only when another project opens, where `project`
 * changes whenever its tabs are saved, as opening a file does.
 */
export const projectPath = computed(() => project()?.path ?? "");

/** The open project's Markdown files; `null` until they are first listed. */
export const listing = signal<FileList | null>(null);

/** The open project's tabs, and the file in front. */
export const tabSet = signal<TabSet>(NO_TABS);

/** The file in front, by its path in the project (`docs/setup.md`); `""` for none. */
export const openPath = computed(() => tabSet().front);

/** The file in front, its text and outline; `null` while it is read. */
export const opened = signal<OpenFile | null>(null);

/** The file in front's top-level headings, the outline. */
export const headings = computed<HeadingInfo[]>(() => opened()?.headings ?? []);

/** How a file opened in a new tab shows (Settings). */
export const defaultMode = signal<ViewMode>("rendered");

/** How the file in front shows: its tab's own. */
export const mode = computed<ViewMode>(() => tabOf(tabSet(), openPath())?.mode ?? defaultMode());

/** How the files panel lists the files. */
export const filesView = signal<FilesView>("tree");

/** What the tab strip does with more tabs than fit in one row: scroll sideways, or wrap onto more rows (Settings). */
export const tabOverflow = signal<TabOverflow>("scroll");

/** Whether the outline shows beside the file. */
export const outlineOpen = signal(true);

/** Whether the files panel shows. */
export const filesOpen = signal(true);

/** Whether a map of the whole file shows beside each view of it. */
export const mapOpen = signal(true);

/**
 * Whether Split's two sides scroll together (the header's toggle,
 * `views/scrollsync.ts`). On at first, and not saved yet: it is being tried.
 */
export const scrollTogether = signal(true);

/** The heading the reader is under, by its index in `headings`; -1 above the first. */
export const reading = signal(-1);

/** Where the caret is in the raw view, for the status bar; `null` while the raw view doesn't show. */
export const caret = signal<{ line: number; column: number } | null>(null);

/** What the title bar's search field holds, as typed. */
export const searchText = signal("");

/** How the search reads its text: any case or the same, any part of a word or whole words, as written or as a pattern. */
export const searchOptions = signal({ caseSensitive: false, wholeWord: false, regex: false });

/** What the search looks through: the file in front, the open tabs, the project (at first) or every project. */
export const searchScope = signal<SearchScope>("project");

/** Whether the search's drop-down shows, under the title bar. */
export const searchOpen = signal(false);

/** Whether the files panel lists the search's results, rather than every file, while there is a search. */
export const resultsInFiles = signal(true);

/** Whether the search's results have a tab of their own, which lasts as long as the search; the window's alone, never saved with the project's tabs. */
export const resultsTab = signal(false);

/** Whether the results' tab is the one in front, rather than the file in front (`openPath`), which stays as it was behind it. */
export const resultsInFront = signal(false);

/** What the projects are searched for: the field's text once typing pauses, read as `searchOptions` say; `null` while the field is empty. */
export const searchQuery = signal<SearchQuery | null>(null);

/** What `searchQuery` found in `searchScope`, every project's files in it; `null` while nothing is searched for, or until the first answer. */
export const searchResults = signal<SearchResults | null>(null);

/** What `searchQuery` found in the open project: what the marks, the counts and F3 go by. */
export const found = computed(() => inProject(searchResults(), projectPath()));

/** The projects' folders, in the list's order, as one value that changes only when a project comes or goes: saving a project's tabs makes a new list. */
const projectFolders = computed(() => projects().projects.map((each) => each.path).join("\n"));

/** The open tabs' files, as one value that changes only when a tab opens, closes or moves. */
const tabFolders = computed(() =>
  tabSet()
    .tabs.map((tab) => tab.path)
    .join("\n"),
);

/** Where the search looks now, as one value (JSON) that changes only when the places do. */
const searchPlaces = computed(() => {
  const scope = searchScope();
  const front = scope === "file" ? openPath() : "";
  const tabs = scope === "tabs" ? tabFolders().split("\n").filter(Boolean) : [];
  const folders = scope === "all" ? projectFolders().split("\n").filter(Boolean) : [];
  return JSON.stringify(placesFor(scope, projectPath(), front, tabs, folders));
});

/** A match of another project the reader asked to go to: it shows once that project has opened. */
let waiting: { project: string; place: MatchPlace } | null = null;

/** Takes the reader to match `index` of `file` in project `project` (its folder), opening the project first when it isn't the one open. */
export function showMatch(project: string, file: string, index: number, open: (file: string) => void) {
  if (project === untracked(projectPath)) {
    currentMatch.set({ file, index });
    if (file !== untracked(openPath)) open(file);
    return;
  }
  waiting = { project, place: { file, index } };
  Projects.select(project).catch(() => {
    waiting = null;
  });
}

/** The match the reader is on, which F3 steps from; `null` before the first step. */
export const currentMatch = signal<MatchPlace | null>(null);

/** The matches in the file in front. */
export const matchesInFront = computed<MatchInfo[]>(() => matchesIn(found(), openPath()));

/** The lines of the outline, and of the heading rail's card: each heading of the file in front, indented below its top level, with how many of the search's matches are under it. */
export const outlineLines = computed(() => {
  const shown = headings();
  const top = Math.min(6, ...shown.map((heading) => heading.level));
  const counts = headingCounts(matchesInFront(), shown);
  return shown.map((heading, index) => ({ title: heading.title, depth: heading.level - top, count: counts[index] }));
});

/** The match the reader is on, by its place among the file in front's matches; -1 when it is in another file, or there is none. */
export const currentInFront = computed(() => {
  const at = currentMatch();
  return at && at.file === openPath() ? at.index : -1;
});

/** Shows `next` as the open project's tabs, and saves them with the project when they changed. */
export function setTabs(next: TabSet) {
  if (next === untracked(tabSet)) return;
  tabSet.set(next);
  const path = untracked(projectPath);
  if (path !== "") Projects.rememberTabs(path, saved(next), next.front);
}

/** What goes to heading `index` in each view that shows: the raw view's and the rendered view's. */
const goers = new Set<(index: number) => void>();

/** Lets `go` take the reader to a heading while the calling view shows. */
export function goesToHeadings(go: (index: number) => void) {
  goers.add(go);
  cleanup(() => goers.delete(go));
}

/** Takes the reader to heading `index` of the file in front, in every view that shows. */
export function goToHeading(index: number) {
  reading.set(index);
  for (const go of goers) go(index);
}

/**
 * Keeps the signals above current: the projects, the open project's files,
 * and the file in front, each from its own listener, started again as the
 * project or the file in front changes. The App calls it once.
 */
export function listen() {
  const stopProjects = Projects.onChange((next) => projects.set(next));
  cleanup(stopProjects);

  // The open project's files. A project opens with the tabs it had; once its
  // files are listed, the tabs of files that have gone close, and a project
  // with no tab opens its README, else its first file, as a preview. After
  // that, a file that goes closes its tab.
  // A match of another project the reader asked to go to (`showMatch`) opens
  // in its file, as the preview, once the project's files are listed.
  effect(() => {
    const path = projectPath();
    listing.set(null);
    const shown = untracked(project);
    tabSet.set(shown ? restore(shown.tabs, shown.lastFile) : NO_TABS);
    const going = waiting?.project === path ? waiting.place : null;
    waiting = null;
    currentMatch.set(null);
    if (path === "") return;
    let first = true;
    return Files.onChange(path, (next) => {
      listing.set(next);
      const present = new Set(next.files.map((file) => file.path));
      let tabs = prune(untracked(tabSet), (file) => present.has(file));
      if (first && going && present.has(going.file)) {
        tabs = open(tabs, going.file, "preview", untracked(defaultMode));
        currentMatch.set(going);
      } else if (first && tabs.tabs.length === 0) {
        const start = fileToOpen(next.files);
        if (start !== "") tabs = open(tabs, start, "preview", untracked(defaultMode));
      }
      first = false;
      setTabs(tabs);
    });
  });

  // The file in front, read again whenever it changes on disk.
  effect(() => {
    const path = projectPath();
    const file = openPath();
    opened.set(null);
    reading.set(-1);
    if (path === "" || file === "") return;
    return Documents.onChange(path, file, (next) => opened.set(next));
  });

  // The query, once typing pauses: a search per key would search a large
  // project again and again for words half typed. An empty field ends the
  // search at once.
  effect(() => {
    const text = searchText();
    const options = searchOptions();
    if (text === "") {
      searchQuery.set(null);
      return;
    }
    const timer = setTimeout(() => searchQuery.set({ text, ...options }), SEARCH_PAUSE);
    return () => clearTimeout(timer);
  });

  // A new search starts with nothing found and no match picked, and the files
  // panel listing what it finds.
  let searchedFor: SearchQuery | null = null;
  effect(() => {
    const query = searchQuery();
    untracked(() => {
      if (!searchedFor && query) resultsInFiles.set(true);
      // The results' tab goes with the search it shows.
      if (!query) {
        resultsTab.set(false);
        resultsInFront.set(false);
      }
      searchedFor = query;
      searchResults.set(null);
      currentMatch.set(null);
    });
  });

  // What the query finds where it looks, found again as the files there
  // change; a new query or new places stop the search before. What was found
  // stays until the new answer comes, so the marks don't blink as a tab opens.
  // The match the reader is on stays while its file still has it.
  effect(() => {
    const places = JSON.parse(searchPlaces()) as SearchPlace[];
    const query = searchQuery();
    if (!query) return;
    return Search.onChange(places, query, (next) => {
      searchResults.set(next);
      const at = untracked(currentMatch);
      if (at && countIn(untracked(found), at.file) <= at.index) currentMatch.set(null);
    });
  });
}
