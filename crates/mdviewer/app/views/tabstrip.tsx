import { computed } from "nsh";
import { type Crumb, FileTabs, IconButton } from "../../../dp/ui";
import { closeResultsTab, closeTab, folderMenu, headingMenu, keepTab, moveTab, pinTab, showTab, tabMenu } from "./actions";
import { filesOpen, found, headings, listing, openPath, outlineOpen, project, reading, resultsInFront, resultsTab, searchQuery, searchResults, tabOverflow, tabSet } from "./model";
import { countIn } from "./search";
import { label, movesWithin, pathSteps } from "./tabs";

// No file's path starts with a colon.
const RESULTS = ":results";

/** The tabs over the reader and the breadcrumbs under them. */
export function TabStrip(props: { filesShown: boolean; outlineShown: boolean }) {
  // A computed, so saving the tabs (a new `project`) makes no new breadcrumbs.
  const projectName = computed(() => project()?.name ?? "");
  const holding = computed(() => new Set((listing()?.files ?? []).map((file) => file.folder)));
  const folderCrumb = (folder: string, name: string): Crumb => ({
    id: `folder:${folder}`,
    label: name,
    menu: holding().has(folder) ? () => folderMenu(folder) : undefined,
    menuLabel: `Files in ${name}`,
  });
  const pathCrumbs = computed<Crumb[]>(() => {
    const path = openPath();
    if (path === "") return [];
    return pathSteps(projectName(), path).map(
      (step): Crumb => (step.kind === "file" ? { id: `file:${path}`, label: step.label, icon: "file-text", current: true } : folderCrumb(step.folder, step.label)),
    );
  });
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
