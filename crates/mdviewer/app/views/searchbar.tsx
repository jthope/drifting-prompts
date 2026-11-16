// The search in the middle of the title bar, as an editor's command center: a
// box that shows what is searched for and where the reader is among the
// matches, and under it, once pressed (or Ctrl+K, or Ctrl+F), a drop-down:
// the field, where to look (This file, Open tabs, Project, All projects), how
// to read what is typed (case, whole words, a pattern), and the matches, a
// section for each file, each a line's number and its words around the match.
//
// It is nsh's `Command` in a `Popover`: the app answers the query
// (`filterable={false}`) from Rust's search (`searchResults` in `model.ts`),
// and the drop-down lists up to `MOST_LISTED` matches; the files panel lists
// them all (Ctrl+Return, or the button at the foot). Up and Down move through
// them, Return goes to one and closes the drop-down, and Escape closes it,
// keeping the search and its marks. Tab moves the keyboard on to the options
// and where to look, as it does anywhere: the field keeps Tab for that, so it
// can't step through the scopes as an editor's does. A row doesn't
// open its file as the highlight passes over it: nsh 0.5.1's `Command` doesn't
// say whether the keys moved the highlight or new results did, so the reader
// would jump to each first match while the query is typed.
import { type JSX, keymap, Text, View, windowSize } from "nsh";
import { Command, type CommandEntry, type CommandGroup, type CommandRef, Popover } from "nsh/base";
import { Button, CountBadge, Icon, IconButton, type IconName, look, popupSurface, Segmented } from "../../../dp/ui";
import { clearSearch, goToResult, openResultsTab, showResultsInFiles } from "./actions";
import { currentMatch, found, projectPath, projects, searchOpen, searchOptions, searchResults, searchScope, searchText } from "./model";
import { excerptParts, MOST_LISTED, matchOf, ordinal, resultItems, rowCount, rowOf, SCOPES, type SearchScope } from "./search";

/** How the search reads what is typed, as the buttons in its field offer it. */
const OPTIONS: { key: "caseSensitive" | "wholeWord" | "regex"; label: string; icon: IconName }[] = [
  { key: "caseSensitive", label: "Match case", icon: "case-sensitive" },
  { key: "wholeWord", label: "Whole words", icon: "whole-word" },
  { key: "regex", label: "Regular expression", icon: "regex" },
];

/** The drop-down's width, in points, in a window wide enough for it. */
const DROP_DOWN_WIDTH = 600;

/** The narrowest the box in the title bar gets, in points, short of room: the header keeps room for it. */
export const SEARCH_MIN_WIDTH = 160;

const tabular = { fontVariant: ["tabular-nums" as const] };

/** Where the reader is among the matches, as the box says it: `2 / 12`, or how many while none is picked. */
function position(): string {
  const results = found();
  if (searchText() === "" || !results) return "";
  const total = `${results.total.toLocaleString()}${results.truncated ? "+" : ""}`;
  const at = currentMatch();
  const which = at ? ordinal(results, at) : 0;
  return which > 0 ? `${which.toLocaleString()} / ${total}` : total;
}

/**
 * The box in the title bar: what is searched for, or how to start; pressing it
 * opens the drop-down. It shows the I-beam, as a field does, since pressing it
 * puts the keyboard in one.
 */
function SearchBox() {
  const empty = () => searchText() === "";
  return (
    <View
      testID="search"
      accessibilityLabel={empty() ? "Search" : `Search: ${searchText()}`}
      class={`h-8 min-w-0 flex-1 cursor-text flex-row items-center gap-2 rounded-md border pl-2.5 pr-2 ${look().field.border} ${searchOpen() ? "border-ring" : ""}`}
      style={look().well}
    >
      <Icon name="search" size={16} tint="muted-foreground" />
      <Text class={`flex-1 text-sm ${empty() ? "text-muted-foreground" : "text-foreground"}`} style={{ numberOfLines: 1 }}>
        {empty() ? "Search" : searchText()}
      </Text>
      <Text class="text-xs text-muted-foreground" style={tabular}>
        {empty() ? keymap.label("mod+k") : position()}
      </Text>
    </View>
  );
}

/** A file's section in the drop-down: its name, its folder (and its project, from another one), and how many matches. */
function FileHeading(props: { group: CommandGroup }) {
  const row = () => rowOf(props.group.heading);
  const file = () => {
    const shown = row();
    return shown?.kind === "file" ? shown : null;
  };
  const folder = () => file()?.file.split("/").slice(0, -1).join("/") ?? "";
  const elsewhere = () => {
    const shown = file();
    if (!shown || shown.project === projectPath()) return "";
    return projects().projects.find((each) => each.path === shown.project)?.name ?? shown.project;
  };
  return (
    <View class="flex-row items-center gap-2 px-2 pb-0.5 pt-2">
      <Icon name="file-text" size={14} tint="muted-foreground" />
      <Text class="text-xs font-semibold text-foreground" style={{ numberOfLines: 1 }}>
        {file()?.file.split("/").pop() ?? ""}
      </Text>
      <Text class="shrink text-xs text-muted-foreground" style={{ numberOfLines: 1 }}>
        {[elsewhere(), folder()].filter(Boolean).join(" / ")}
      </Text>
      <View class="flex-1" />
      <CountBadge count={rowCount(searchResults(), props.group.heading)} />
    </View>
  );
}

/** A match's row: its line's number, then the words around it, the match marked. */
function MatchLine(props: { entry: CommandEntry }) {
  const found = () => matchOf(searchResults(), props.entry.id)?.match;
  const parts = () => {
    const match = found();
    return match ? excerptParts(match) : ["", props.entry.label, ""];
  };
  return (
    <View class="min-w-0 flex-1 flex-row items-baseline gap-2.5">
      <Text class="w-8 text-right text-xs text-muted-foreground" style={tabular}>
        {found() ? String((found()?.startLine ?? 0) + 1) : ""}
      </Text>
      <Text class={`flex-1 text-sm ${look().menuItem.text}`} style={{ numberOfLines: 1 }}>
        {parts()[0]}
        <Text style={{ backgroundColor: look().found.match }}>{parts()[1]}</Text>
        {parts()[2]}
      </Text>
    </View>
  );
}

/** A key's name, small, as a hint beside what it does. */
function Hint(props: { keys: string; children: JSX.Element }) {
  return (
    <View class="flex-row items-center gap-1">
      <Text class="rounded border border-border px-1 text-xs text-muted-foreground">{props.keys}</Text>
      <Text class="text-xs text-muted-foreground">{props.children}</Text>
    </View>
  );
}

/**
 * While the drop-down shows, Ctrl+Return hands the results to the files panel.
 * Escape is the popover's own, which closes it before the field could take it
 * to empty the search.
 */
function DropDownKeys() {
  keymap.bind("mod+enter", () => showResultsInFiles(), { when: "always" });
  return null;
}

/** What the drop-down says while it lists no match: why, or that it is searching. */
function Empty() {
  const said = () => {
    const results = searchResults();
    if (searchText() === "") return "Type to find it in every file where the search looks.";
    if (!results) return "Searching…";
    if (results.problem !== "") return results.problem;
    return "No matches.";
  };
  return <Text class="px-3 py-6 text-center text-sm text-muted-foreground">{said()}</Text>;
}

/** The drop-down under the box: the field and its options, where to look, the matches, and the keys. `onRef` hands over its list, to focus. */
function DropDown(props: { onRef: (command: CommandRef) => void }) {
  const results = () => searchResults();
  const listed = () => resultItems(results());
  const summary = () => {
    const shown = results();
    if (!shown || searchText() === "" || shown.problem !== "") return "";
    const files = shown.files.length;
    return `${shown.total.toLocaleString()}${shown.truncated ? "+" : ""} ${shown.total === 1 ? "match" : "matches"} in ${files.toLocaleString()} ${files === 1 ? "file" : "files"}`;
  };
  const toggle = (key: (typeof OPTIONS)[number]["key"]) => searchOptions.set({ ...searchOptions(), [key]: !searchOptions()[key] });
  return (
    <View>
      <Command
        ref={props.onRef}
        filterable={false}
        separator={false}
        items={listed()}
        query={searchText()}
        loading={searchText() !== "" && !results()}
        placeholder="Search"
        accessibilityLabel="Matches"
        searchAccessibilityLabel="Search the projects"
        searchBarClass="h-11 flex-row items-center gap-2 border-b border-border pl-3 pr-1.5"
        searchClass="flex-1 text-sm text-foreground"
        listClass="max-h-96 px-1.5 pb-1.5"
        itemClass={`h-7 flex-row items-center rounded-md px-2 ${look().menuItem.row}`}
        groupClass="px-0"
        searchPrefix={<Icon name="search" size={16} tint="muted-foreground" />}
        searchSuffix={
          <View class="flex-row items-center">
            <Each of={OPTIONS} key={(option) => option.key}>
              {(option) => <IconButton icon={option().icon} label={option().label} size="small" on={searchOptions()[option().key]} onPress={() => toggle(option().key)} />}
            </Each>
          </View>
        }
        header={
          <View class="flex-row flex-wrap items-center gap-3 border-b border-border px-3 py-2">
            <Segmented<SearchScope> items={SCOPES} value={searchScope()} onChange={(next) => searchScope.set(next)} />
            <Text class="flex-1 text-right text-xs text-muted-foreground" style={tabular}>
              {summary()}
            </Text>
          </View>
        }
        empty={<Empty />}
        footer={
          <View class="flex-row flex-wrap items-center gap-x-4 gap-y-1 border-t border-border px-3 py-2">
            <Hint keys="↑ ↓">move</Hint>
            <Hint keys="Enter">go there</Hint>
            <Hint keys="Esc">close</Hint>
            <View class="flex-1" />
            <If when={(results()?.total ?? 0) > MOST_LISTED}>
              <Text class="text-xs text-muted-foreground">The first {MOST_LISTED.toLocaleString()} here</Text>
            </If>
            <Button label={`Show in files (${keymap.label("mod+enter")})`} kind="ghost" disabled={(results()?.total ?? 0) === 0} onPress={showResultsInFiles} />
            <Button label="Open in a tab" kind="ghost" disabled={(results()?.total ?? 0) === 0} onPress={openResultsTab} />
          </View>
        }
        renderGroup={(group) => <FileHeading group={group} />}
        renderItem={(entry) => <MatchLine entry={entry} />}
        onQueryChange={(event) => searchText.set(event.query)}
        onSelect={(event) => {
          searchOpen.set(false);
          goToResult(event.id);
        }}
        onCancel={() => searchOpen.set(false)}
      />
    </View>
  );
}

/**
 * The search in the title bar: the box, the drop-down under it, and a button
 * that ends the search. Ctrl+K and Ctrl+F open the drop-down.
 */
export function SearchBar() {
  keymap.bind({
    "mod+k": () => searchOpen.set(true),
    "mod+f": () => searchOpen.set(true),
  });
  const surface = () => popupSurface("overflow-hidden");
  let command: CommandRef | undefined;
  return (
    <View class="w-96 shrink flex-row items-center gap-1" style={{ minWidth: SEARCH_MIN_WIDTH }}>
      <Popover
        open={searchOpen()}
        placement="bottom"
        offset={4}
        margin={12}
        class="min-w-0 flex-1 flex-row"
        popupClass={surface().class}
        popupStyle={{ ...surface().style, width: Math.min(DROP_DOWN_WIDTH, windowSize().width - 24) }}
        trigger={<SearchBox />}
        onOpenChange={(event) => searchOpen.set(event.open)}
        // The keyboard goes to the field as the drop-down shows, however it was opened.
        onOpen={() => command?.focus()}
      >
        <DropDown
          onRef={(ref) => {
            command = ref;
          }}
        />
      </Popover>
      <If when={searchText() !== ""}>
        <IconButton icon="x" label="Clear search" size="small" onPress={clearSearch} />
      </If>
      <If when={searchOpen()}>
        <DropDownKeys />
      </If>
    </View>
  );
}
