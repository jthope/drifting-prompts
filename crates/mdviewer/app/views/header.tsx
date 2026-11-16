import { signal, Text, View } from "nsh";
import type { ViewMode } from "mdviewer/native";
import { IconButton, type IconName, Logo, look, MenuOpener, type Mode, ThemeMenu, type ThemeName, TitleBar, toggleFullScreen } from "../../../dp/ui";
import { mapOpen, mode, openPath, page, project, resultsInFront, resultsTab, scrollTogether, searchQuery } from "./model";
import { SEARCH_MIN_WIDTH, SearchBar } from "./searchbar";

/** The gap between the name, where the file is, the search and the room either side of it, in points: `gap-3`. */
const ROW_GAP = 12;

/** Raw, Split and Rendered, as the header's menu and Settings offer them. */
export const MODES: { value: ViewMode; title: string; icon: IconName }[] = [
  { value: "raw", title: "Raw", icon: "code" },
  { value: "split", title: "Split", icon: "columns-2" },
  { value: "rendered", title: "Rendered", icon: "book-open" },
];

/** What the header names: the open file, else the project, or New. */
function title(): string {
  if (page() === "new") return "New project";
  if (resultsTab() && resultsInFront()) return `Search: ${searchQuery()?.text ?? ""}`;
  const file = openPath();
  return file === "" ? (project()?.name ?? "") : (file.split("/").pop() ?? file);
}

/** What it says beside the name, muted: the project, and the folder the file is in. */
function subtitle(): string {
  if (page() !== "project") return "";
  if (resultsTab() && resultsInFront()) return project()?.name ?? "";
  if (openPath() === "") return "";
  const folder = openPath().split("/").slice(0, -1).join("/");
  const name = project()?.name ?? "";
  return folder === "" ? name : `${name} / ${folder}`;
}

/**
 * The title bar over the files panel, the file and its outline, as IP
 * Camera's over its stage: the file's name and where it is, the search in the
 * middle of the room between them and the buttons, then how its tab shows it
 * (an icon whose menu offers Raw, Split and Rendered: `onMode`), the theme
 * menu, with the appearance's mode, Light, Dark or System (`appearance`,
 * `onAppearance`, as Settings picks it), over the themes of the appearance it
 * shows (`onTheme`, as Settings picks one), and full screen. The files panel
 * and the outline have their buttons in their own headers.
 */
export function Header(props: {
  sidebarOpen: boolean;
  appearance: Mode;
  onShowSidebar: () => void;
  onMode: (mode: ViewMode) => void;
  onAppearance: (mode: Mode) => void;
  onTheme: (name: ThemeName) => void;
}) {
  /** Whether a file is in front, rather than nothing or the search's results, which its buttons would act on. */
  const reading = () => page() === "project" && openPath() !== "" && !(resultsTab() && resultsInFront());
  /** How the file in front shows, as its menu's button says it. */
  const shown = () => MODES.find((each) => each.value === mode()) ?? MODES[0];
  /** The name's width, as laid out: it never narrows, so a resized window leaves it as it is. */
  const nameWidth = signal(0);
  /** The narrowest the name and the search's row gets: the name, and with the search, its narrowest and the row's four gaps. */
  const rowMinWidth = () => nameWidth() + (page() === "project" ? SEARCH_MIN_WIDTH + 4 * ROW_GAP : 0);
  return (
    <TitleBar class="pl-3">
      <If when={!props.sidebarOpen}>
        <View class="flex-row items-center gap-2">
          <Logo product="mdviewer" />
          <IconButton icon="panel-left" label="Show sidebar" onPress={props.onShowSidebar} />
        </View>
      </If>

      {/* The name and the search share the room the buttons leave, in one row the title bar never breaks (it moves
          the buttons under it first): the name as wide as it is, then the search in the middle of what is left, as
          much room each side of it. Short of room, where the file is gives way first, then the search; the name
          keeps its width (up to `max-w-64`), as nsh doesn't round a layout and the least it lost would cut it
          short. The row is never narrower than the name and the narrowest search, so the buttons move under it
          before it would overflow. */}
      <View class="flex-1 flex-row items-center gap-3" style={{ minWidth: rowMinWidth() }}>
        <View class="max-w-64" onLayout={(event) => nameWidth.set(event.bounds.size.width)}>
          <Text class={`text-sm font-semibold ${look().titleText ?? "text-foreground"}`} style={{ numberOfLines: 1 }}>
            {title()}
          </Text>
        </View>
        <If when={subtitle() !== ""}>
          <Text class="min-w-0 shrink-100 text-sm text-muted-foreground" style={{ numberOfLines: 1 }}>
            {subtitle()}
          </Text>
        </If>
        <View class="flex-1" />
        <If when={page() === "project"}>
          <SearchBar />
        </If>
        <View class="flex-1" />
      </View>

      {/* As wide as the buttons, so the search's room ends where they start; on a line of its own, at the end. */}
      <View class="ml-auto flex-row items-center gap-1">
        <If when={reading()}>
          <MenuOpener>
            {(open) => (
              <IconButton
                icon={shown().icon}
                label={`${shown().title}. Change how the file shows`}
                onPress={() => open(MODES.map((each) => ({ label: each.title, checked: each.value === mode(), onSelect: () => props.onMode(each.value) })))}
              />
            )}
          </MenuOpener>
        </If>
        {/* In Split: whether its two sides scroll together. */}
        <If when={reading() && mode() === "split"}>
          <IconButton
            icon="arrow-down-up"
            label={scrollTogether() ? "Scroll the sides on their own" : "Scroll the sides together"}
            on={scrollTogether()}
            class="ml-1"
            onPress={() => scrollTogether.set(!scrollTogether())}
          />
        </If>
        {/* The map of the whole file beside each view, saved with the preferences. */}
        <If when={reading()}>
          <IconButton icon="map" label={mapOpen() ? "Hide the map" : "Show the map"} on={mapOpen()} class="ml-1" onPress={() => mapOpen.set(!mapOpen())} />
        </If>
        {/* What the whole window does, apart from the file's own. */}
        <ThemeMenu mode={props.appearance} onMode={props.onAppearance} onPick={props.onTheme} class="ml-1" />
        <IconButton icon="maximize" label="Full screen" onPress={toggleFullScreen} />
      </View>
    </TitleBar>
  );
}
