import { type JSX, keymap, Text, View, windowSize } from "nsh";
import { Command, type CommandEntry, type CommandGroup, type CommandRef, Popover } from "nsh/base";
import { Button, CountBadge, Icon, IconButton, type IconName, look, popupSurface, Segmented } from "../../../dp/ui";
import { clearSearch, goToResult, openResultsTab, showResultsInFiles } from "./actions";
import { currentMatch, found, projectPath, projects, searchOpen, searchOptions, searchResults, searchScope, searchText } from "./model";
import { excerptParts, MOST_LISTED, matchOf, ordinal, resultItems, rowCount, rowOf, SCOPES, type SearchScope } from "./search";
import { bindShortcuts } from "./shortcuts";

const OPTIONS: { key: "caseSensitive" | "wholeWord" | "regex"; label: string; icon: IconName }[] = [
  { key: "caseSensitive", label: "Match case", icon: "case-sensitive" },
  { key: "wholeWord", label: "Whole words", icon: "whole-word" },
  { key: "regex", label: "Regular expression", icon: "regex" },
];

const DROP_DOWN_WIDTH = 600;

/** The narrowest the search box gets, in points. */
export const SEARCH_MIN_WIDTH = 160;

const tabular = { fontVariant: ["tabular-nums" as const] };

function position(): string {
  const results = found();
  if (searchText() === "" || !results) return "";
  const total = `${results.total.toLocaleString()}${results.truncated ? "+" : ""}`;
  const at = currentMatch();
  const which = at ? ordinal(results, at) : 0;
  return which > 0 ? `${which.toLocaleString()} / ${total}` : total;
}

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

function Hint(props: { keys: string; children: JSX.Element }) {
  return (
    <View class="flex-row items-center gap-1">
      <Text class="rounded border border-border px-1 text-xs text-muted-foreground">{props.keys}</Text>
      <Text class="text-xs text-muted-foreground">{props.children}</Text>
    </View>
  );
}

// Escape stays the popover's, which closes it before the field could empty the search.
function DropDownKeys() {
  keymap.bind("mod+enter", () => showResultsInFiles(), { when: "always" });
  return null;
}

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
        // Only on select: nsh's `Command` doesn't say whether keys or new results moved the highlight.
        onSelect={(event) => {
          searchOpen.set(false);
          goToResult(event.id);
        }}
        onCancel={() => searchOpen.set(false)}
      />
    </View>
  );
}

/** The title bar's search: the box, its drop-down, and a button that ends the search. */
export function SearchBar() {
  bindShortcuts({
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
