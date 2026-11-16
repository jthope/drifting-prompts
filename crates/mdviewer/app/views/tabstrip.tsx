// The tabs over the reader: one for each file open in the project, the shared
// `FileTabs` drawn from the tabs in `model.ts` and acting through `actions.ts`,
// each with how many matches the search found in its file; and, while the
// search's results have a tab of their own (`results.tsx`), theirs last, which
// is the window's alone and never saved. They scroll in one row or wrap onto
// more, as Settings says (`tabOverflow`). While the files panel or the outline
// is hidden, the button that shows it again waits at this strip's end on its
// side.
//
// Under the tabs, the breadcrumbs: the way to the file in front (the project,
// each folder, the file), then the heading the reader is under. A folder's
// lists the files in it, the project's those at its top, and the heading's
// the file's headings; a folder with no files of its own is plain text. While
// the results' tab is in front, the project and Search.
import { computed } from "nsh";
import { type Crumb, FileTabs, IconButton } from "../../../dp/ui";
import { closeResultsTab, closeTab, folderMenu, headingMenu, keepTab, moveTab, pinTab, showTab, tabMenu } from "./actions";
import { filesOpen, found, headings, listing, openPath, outlineOpen, project, reading, resultsInFront, resultsTab, searchQuery, searchResults, tabOverflow, tabSet } from "./model";
import { countIn } from "./search";
import { label, movesWithin, pathSteps } from "./tabs";

/** The results' tab's id: no file's path, which never holds a colon at its start. */
const RESULTS = ":results";

/** The tab strip. `filesShown` and `outlineShown` say whether those panes show, rather than only whether they are asked for. */
export function TabStrip(props: { filesShown: boolean; outlineShown: boolean }) {
  /** The project's name: a computed, so saving the tabs (a new `project`) makes no new breadcrumbs. */
  const projectName = computed(() => project()?.name ?? "");
  /** The folders that hold files of their own, whose breadcrumbs list them. */
  const holding = computed(() => new Set((listing()?.files ?? []).map((file) => file.folder)));
  /** The breadcrumb of folder `folder` (`""` for the project's own), named `name`. */
  const folderCrumb = (folder: string, name: string): Crumb => ({
    id: `folder:${folder}`,
    label: name,
    menu: holding().has(folder) ? () => folderMenu(folder) : undefined,
    menuLabel: `Files in ${name}`,
  });
  /** The breadcrumbs of the way to the file in front: the project, its folders, the file. */
  const pathCrumbs = computed<Crumb[]>(() => {
    const path = openPath();
    if (path === "") return [];
    return pathSteps(projectName(), path).map(
      (step): Crumb => (step.kind === "file" ? { id: `file:${path}`, label: step.label, icon: "file-text", current: true } : folderCrumb(step.folder, step.label)),
    );
  });
  /** The breadcrumbs under the tabs: the way to the file in front, then the heading the reader is under; the project, then Search, while the results' tab is in front. */
  const crumbs = (): Crumb[] => {
    if (resultsTab() && resultsInFront()) return [folderCrumb("", projectName()), { id: RESULTS, label: "Search", icon: "search", current: true }];
    const heading = headings()[reading()];
    if (!heading) return pathCrumbs();
    const file = openPath().split("/").pop() ?? "";
    return [...pathCrumbs(), { id: ":heading", label: heading.title === "" ? "Untitled" : heading.title, menu: headingMenu, menuLabel: `Headings in ${file}` }];
  };

  const files = () =>
    tabSet().tabs.map((tab) => {
      const said = label(tabSet().tabs, tab);
      return { id: tab.path, name: said.name, detail: said.folder, pinned: tab.pinned, preview: tab.preview, count: countIn(found(), tab.path) };
    });
  const tabs = () =>
    resultsTab()
      ? [
          ...files(),
          {
            id: RESULTS,
            name: `Search: ${searchQuery()?.text ?? ""}`,
            detail: "",
            pinned: false,
            pinnable: false,
            preview: false,
            count: searchResults()?.total ?? 0,
            icon: "search" as const,
          },
        ]
      : files();
  /** The file tabs' count: the results' tab after them moves nowhere, and nothing moves past them. */
  const fileCount = () => tabSet().tabs.length;
  return (
    <FileTabs
      tabs={tabs()}
      selected={resultsInFront() ? RESULTS : tabSet().front}
      crumbs={crumbs()}
      wrap={tabOverflow() === "wrap"}
      start={
        <If when={!props.filesShown}>
          <IconButton icon="panel-left-open" label="Show files" size="small" class="mb-0.5 ml-1 self-end" onPress={() => filesOpen.set(true)} />
        </If>
      }
      end={
        <If when={!props.outlineShown}>
          <IconButton icon="panel-right-open" label="Show outline" size="small" class="mb-0.5 mr-1 self-end" onPress={() => outlineOpen.set(true)} />
        </If>
      }
      within={(index) => (index >= fileCount() ? [index, index] : movesWithin(tabSet().tabs, index))}
      menu={(id) => (id === RESULTS ? [{ label: "Close", onSelect: closeResultsTab }] : tabMenu(id))}
      onSelect={(id) => (id === RESULTS ? resultsInFront.set(true) : showTab(id))}
      onKeep={(id) => {
        if (id !== RESULTS) keepTab(id);
      }}
      onClose={(id) => (id === RESULTS ? closeResultsTab() : closeTab(id))}
      onPin={(id, pinned) => {
        if (id !== RESULTS) pinTab(id, pinned);
      }}
      onMove={(from, to) => (from >= fileCount() || to >= fileCount() ? Promise.resolve(false) : moveTab(from, to))}
    />
  );
}
