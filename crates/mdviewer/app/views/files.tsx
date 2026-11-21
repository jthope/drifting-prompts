import { computed, effect, signal, Text, untracked, View } from "nsh";
import type { FilesView } from "mdviewer/native";
import { Button, IconButton, type IconName, MenuOpener, PaneHeader, SearchField, SelectionBar, TreeList, type TreePress } from "../../../dp/ui";
import { fileMenu, goToResult, openFile, openFiles, openResultsTab } from "./actions";
import { folderOf } from "./document";
import { currentMatch, filesView, found, listing, openPath, project, projectPath, projects, resultsInFiles, searchQuery, searchResults } from "./model";
import { countIn, matchOf, resultTree, rowCount, rowId, rowOf } from "./search";
import { allFolders, fileList, foldersAbove, isFolder, itemsFor, matching, openFolders, pickedFiles, ROOT, shownFiles } from "./tree";

function SearchResultsList() {
  const closed = signal<string[]>([]);
  const names = computed(() => new Map(projects().projects.map((each) => [each.path, each.name])));
  const items = computed(() => resultTree(searchResults(), names()));
  const expanded = computed(() => openFolders(items(), closed()));
  const current = () => {
    const at = currentMatch();
    return at ? rowId({ kind: "match", project: projectPath(), file: at.file, index: at.index }) : null;
  };
  effect(() => {
    const at = currentMatch();
    if (!at) return;
    const file = rowId({ kind: "file", project: untracked(projectPath), file: at.file });
    const project = rowId({ kind: "project", project: untracked(projectPath) });
    if (untracked(closed).some((id) => id === file || id === project)) closed.set(untracked(closed).filter((id) => id !== file && id !== project));
  });
  const said = () => {
    const results = searchResults();
    if (!results) return "Searching…";
    if (results.problem !== "") return results.problem;
    if (results.total === 0) return "No matches.";
    const files = results.files.length;
    return `${results.total.toLocaleString()}${results.truncated ? "+" : ""} ${results.total === 1 ? "match" : "matches"} in ${files.toLocaleString()} ${files === 1 ? "file" : "files"}`;
  };
  return (
    <View class="min-h-0 flex-1">
      <Text class="px-4 pb-1 pt-3 text-xs text-muted-foreground">{said()}</Text>
      <TreeList
        label="Search results"
        testID="results"
        items={items()}
        expanded={expanded()}
        selected={current()}
        icon={(id) => {
          const row = rowOf(id);
          return row?.kind === "project" ? "folder" : row?.kind === "file" ? "file-text" : null;
        }}
        lead={(id) => {
          const found = matchOf(searchResults(), id);
          return found ? String(found.match.startLine + 1) : "";
        }}
        detail={(id) => {
          const row = rowOf(id);
          return row?.kind === "file" ? folderOf(row.file) : "";
        }}
        found={(id) => {
          const match = matchOf(searchResults(), id)?.match;
          return match ? [match.excerptStart, match.excerptEnd] : null;
        }}
        count={(id) => rowCount(searchResults(), id)}
        onExpandedChange={(next) => closed.set(allFolders(items()).filter((id) => !next.includes(id)))}
        onSelect={(id) => {
          if (rowOf(id)?.kind === "match") goToResult(id);
        }}
        onAction={goToResult}
      />
    </View>
  );
}

/** The narrowest the files panel gets, in points. */
export const FILES_MIN_WIDTH = 180;

const VIEWS: { value: FilesView; title: string; icon: IconName }[] = [
  { value: "tree", title: "View as Tree", icon: "list-tree" },
  { value: "folder", title: "View as Folder", icon: "folder" },
  { value: "list", title: "View as List", icon: "list" },
];

/** The files panel: the open project's Markdown files, or what the search found. */
export function FilesPanel(props: { onHide: () => void }) {
  const query = signal("");
  const treeClosed = signal<string[]>([]);
  const groupsClosed = signal<string[]>([]);
  const marked = signal<string[]>([]);
  const cursor = signal<string | null>(null);
  let anchor = "";

  const files = () => listing()?.files ?? [];
  const filtering = () => query().trim() !== "";
  const view = (): FilesView => (filtering() ? "list" : filesView());
  // A computed, so saving the tabs (a new `project`) makes no new items.
  const root = computed(() => project()?.name ?? "");
  const items = computed(() => (filtering() ? fileList(matching(files(), query())) : itemsFor(filesView(), files(), root())));
  const closedIn = (shown: FilesView) => (shown === "tree" ? treeClosed : groupsClosed);
  const expanded = computed(() => (view() === "list" ? [] : openFolders(items(), closedIn(view())())));
  const setExpanded = (next: string[]) => {
    if (view() !== "list") closedIn(view()).set(allFolders(items()).filter((id) => !next.includes(id)));
  };
  // The tree keeps its top open, so its folders show.
  const collapseAll = () => {
    if (view() !== "list") closedIn(view()).set(allFolders(items()).filter((id) => view() === "folder" || id !== ROOT));
  };

  effect(() => {
    projectPath();
    query.set("");
    marked.set([]);
    treeClosed.set([]);
    groupsClosed.set([]);
  });
  effect(() => {
    const path = openPath();
    const shown = filesView();
    untracked(() => {
      cursor.set(path === "" ? null : path);
      marked.set([]);
      if (shown === "list") return;
      const closed = closedIn(shown);
      const above = foldersAbove(path, shown);
      if (above.some((id) => closed().includes(id))) closed.set(closed().filter((id) => !above.includes(id)));
    });
  });

  const press = (id: string, keys: TreePress) => {
    if (isFolder(id)) {
      cursor.set(id);
      setExpanded(expanded().includes(id) ? expanded().filter((each) => each !== id) : [...expanded(), id]);
      return;
    }
    if (keys.control || keys.shift) {
      const front = openPath();
      const order = shownFiles(items(), expanded());
      const before = marked().length > 0 ? marked() : order.includes(front) ? [front] : [];
      marked.set(pickedFiles(order, before, anchor === "" ? front : anchor, id, keys));
      if (!keys.shift) anchor = id;
      cursor.set(id);
      return;
    }
    anchor = id;
    marked.set([]);
    openFile(id, keys.clickCount >= 2 ? "keep" : "preview");
  };

  const openPicked = () => {
    const order = shownFiles(items(), expanded());
    openFiles(order.filter((path) => marked().includes(path)));
    marked.set([]);
  };

  const current = () => VIEWS.find((each) => each.value === filesView()) ?? VIEWS[0];
  const folderIn = (id: string) => files().find((file) => file.path === id)?.folder ?? "";
  const searching = () => searchQuery() !== null;
  const results = () => searching() && resultsInFiles();

  return (
    <View class="flex-1">
      <PaneHeader title={results() ? "Search" : "Files"}>
        <If when={searching()}>
          <If when={results()}>
            <IconButton icon="maximize-2" label="Open the results in a tab" size="small" onPress={openResultsTab} />
          </If>
          <IconButton
            icon={results() ? "list-tree" : "search"}
            label={results() ? "Show every file" : "Show what the search found"}
            size="small"
            onPress={() => resultsInFiles.set(!resultsInFiles())}
          />
        </If>
        <If when={!results()}>
          <MenuOpener>
            {(open) => (
              <IconButton
                icon={current().icon}
                label={`${current().title}. Change how the files are listed`}
                size="small"
                onPress={() => open(VIEWS.map((each) => ({ label: each.title, checked: each.value === filesView(), onSelect: () => filesView.set(each.value) })))}
              />
            )}
          </MenuOpener>
          <If when={view() !== "list"}>
            <IconButton icon="chevrons-down-up" label="Collapse all folders" size="small" onPress={collapseAll} />
          </If>
        </If>
        <IconButton icon="panel-left-close" label="Hide files" size="small" onPress={props.onHide} />
      </PaneHeader>
      <If when={!results()} else={<SearchResultsList />}>
        <View class="px-3 pb-2 pt-3">
          <SearchField label="Filter files" value={query()} onChange={(next) => query.set(next)} />
        </View>
        <If when={files().length > 0 && items().length === 0}>
          <Text class="px-4 py-2 text-xs text-muted-foreground">No file's path holds “{query().trim()}”.</Text>
        </If>
        <TreeList
          label="Files"
          testID="files"
          items={items()}
          expanded={expanded()}
          selected={cursor()}
          marked={marked()}
          icon={(id, folder) => (folder ? (expanded().includes(id) ? "folder-open" : "folder") : "file-text")}
          detail={(id) => (view() === "list" && !isFolder(id) ? folderIn(id) : "")}
          count={(id) => (isFolder(id) ? 0 : countIn(found(), id))}
          dimmed={(id) => searching() && !isFolder(id) && countIn(found(), id) === 0}
          menu={(id) => (isFolder(id) ? [] : fileMenu(id, marked()))}
          onExpandedChange={setExpanded}
          onSelect={(id) => {
            cursor.set(id);
            if (!isFolder(id)) {
              anchor = id;
              marked.set([]);
              openFile(id);
            }
          }}
          onAction={(id) => {
            if (!isFolder(id)) openFile(id, "keep");
          }}
          onPress={press}
        />
        <If when={marked().length > 1}>
          <SelectionBar onClear={() => marked.set([])}>
            <Button label={`Open ${marked().length} in tabs`} icon="file-text" kind="primary" onPress={openPicked} />
          </SelectionBar>
        </If>
      </If>
    </View>
  );
}
