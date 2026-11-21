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

const SEARCH_PAUSE = 200;

/** The projects, and the one open. */
export const projects = signal<ProjectList>(Projects.list());

/** What the window shows: a project, or New. */
export type Page = "project" | "new";

export const page = computed<Page>(() => (projects().selected !== "" ? "project" : "new"));

/** A section of Settings. */
export type SettingsPart = "files" | "appearance";

/** Whether Settings shows. */
export const settingsOpen = signal(false);

/** The section of Settings shown, kept while the dialog is closed. */
export const settingsSection = signal<SettingsPart>("files");

/** The open project; `undefined` while New shows. A new value whenever its tabs are saved. */
export const project = computed<ProjectInfo | undefined>(() => projects().projects.find((each) => each.path === projects().selected));

/** The open project's folder; `""` while New shows. Unlike `project`, saving the tabs doesn't change it. */
export const projectPath = computed(() => project()?.path ?? "");

/** The open project's Markdown files; `null` until they are first listed. */
export const listing = signal<FileList | null>(null);

/** The open project's tabs, and the file in front. */
export const tabSet = signal<TabSet>(NO_TABS);

/** The file in front, by its path in the project; `""` for none. */
export const openPath = computed(() => tabSet().front);

/** The file in front, its text and outline; `null` while it is read. */
export const opened = signal<OpenFile | null>(null);

/** The file in front's top-level headings. */
export const headings = computed<HeadingInfo[]>(() => opened()?.headings ?? []);

/** How a file opened in a new tab shows. */
export const defaultMode = signal<ViewMode>("rendered");

/** How the file in front shows. */
export const mode = computed<ViewMode>(() => tabOf(tabSet(), openPath())?.mode ?? defaultMode());

/** How the files panel lists the files. */
export const filesView = signal<FilesView>("tree");

/** What the tab strip does with more tabs than fit in one row. */
export const tabOverflow = signal<TabOverflow>("scroll");

/** Whether the outline shows. */
export const outlineOpen = signal(true);

/** Whether the files panel shows. */
export const filesOpen = signal(true);

/** Whether the map of the whole file shows. */
export const mapOpen = signal(true);

/** Whether Split's two sides scroll together; not saved. */
export const scrollTogether = signal(true);

/** The heading the reader is under, by its index in `headings`; -1 above the first. */
export const reading = signal(-1);

/** Where the caret is in the raw view; `null` while it doesn't show. */
export const caret = signal<{ line: number; column: number } | null>(null);

/** What the search field holds, as typed. */
export const searchText = signal("");

/** How the search reads its text. */
export const searchOptions = signal({ caseSensitive: false, wholeWord: false, regex: false });

/** What the search looks through. */
export const searchScope = signal<SearchScope>("project");

/** Whether the search's drop-down shows. */
export const searchOpen = signal(false);

/** Whether the files panel lists the search's results while there is a search. */
export const resultsInFiles = signal(true);

/** Whether the search's results have a tab of their own; never saved with the project's tabs. */
export const resultsTab = signal(false);

/** Whether the results' tab is in front of the file in front. */
export const resultsInFront = signal(false);

/** What the projects are searched for once typing pauses; `null` while the field is empty. */
export const searchQuery = signal<SearchQuery | null>(null);

/** What `searchQuery` found in `searchScope`; `null` until the first answer. */
export const searchResults = signal<SearchResults | null>(null);

/** What `searchQuery` found in the open project. */
export const found = computed(() => inProject(searchResults(), projectPath()));

// Strings, so saving a project's tabs (a new list) doesn't search again.
const projectFolders = computed(() => projects().projects.map((each) => each.path).join("\n"));

const tabFolders = computed(() =>
  tabSet()
    .tabs.map((tab) => tab.path)
    .join("\n"),
);

// JSON, so it changes only when the places do.
const searchPlaces = computed(() => {
  const scope = searchScope();
  const front = scope === "file" ? openPath() : "";
  const tabs = scope === "tabs" ? tabFolders().split("\n").filter(Boolean) : [];
  const folders = scope === "all" ? projectFolders().split("\n").filter(Boolean) : [];
  return JSON.stringify(placesFor(scope, projectPath(), front, tabs, folders));
});

let waiting: { project: string; place: MatchPlace } | null = null;

/** Takes the reader to match `index` of `file` in `project`, opening that project first. */
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

/** The match the reader is on; `null` before the first step. */
export const currentMatch = signal<MatchPlace | null>(null);

/** The matches in the file in front. */
export const matchesInFront = computed<MatchInfo[]>(() => matchesIn(found(), openPath()));

/** The outline's lines: each heading, its depth, and its count of matches. */
export const outlineLines = computed(() => {
  const shown = headings();
  const top = Math.min(6, ...shown.map((heading) => heading.level));
  const counts = headingCounts(matchesInFront(), shown);
  return shown.map((heading, index) => ({ title: heading.title, depth: heading.level - top, count: counts[index] }));
});

/** The index of the match the reader is on among the file in front's; -1 when none. */
export const currentInFront = computed(() => {
  const at = currentMatch();
  return at && at.file === openPath() ? at.index : -1;
});

/** Shows `next` as the open project's tabs, and saves them when they changed. */
export function setTabs(next: TabSet) {
  if (next === untracked(tabSet)) return;
  tabSet.set(next);
  const path = untracked(projectPath);
  if (path !== "") Projects.rememberTabs(path, saved(next), next.front);
}

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

/** Keeps the signals above current from Rust's listeners; call once. */
export function listen() {
  const stopProjects = Projects.onChange((next) => projects.set(next));
  cleanup(stopProjects);

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

  effect(() => {
    const path = projectPath();
    const file = openPath();
    opened.set(null);
    reading.set(-1);
    if (path === "" || file === "") return;
    return Documents.onChange(path, file, (next) => opened.set(next));
  });

  // Once typing pauses: a search per key would search a large project for every half-typed word.
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

  let searchedFor: SearchQuery | null = null;
  effect(() => {
    const query = searchQuery();
    untracked(() => {
      if (!searchedFor && query) resultsInFiles.set(true);
      if (!query) {
        resultsTab.set(false);
        resultsInFront.set(false);
      }
      searchedFor = query;
      searchResults.set(null);
      currentMatch.set(null);
    });
  });

  // What was found stays until the new answer comes, so the marks don't blink as a tab opens.
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
