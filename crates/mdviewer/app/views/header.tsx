import { signal, Text, View } from "nsh";
import type { ViewMode } from "mdviewer/native";
import { IconButton, type IconName, Logo, look, MenuOpener, type Mode, ThemeMenu, type ThemeName, TitleBar, toggleFullScreen } from "../../../dp/ui";
import { mapOpen, mode, openPath, page, project, resultsInFront, resultsTab, scrollTogether, searchQuery } from "./model";
import { SEARCH_MIN_WIDTH, SearchBar } from "./searchbar";

/** Must match the row's `gap-3`. */
const ROW_GAP = 12;

/** Raw, Split and Rendered, with their titles and icons. */
export const MODES: { value: ViewMode; title: string; icon: IconName }[] = [
  { value: "raw", title: "Raw", icon: "code" },
  { value: "split", title: "Split", icon: "columns-2" },
  { value: "rendered", title: "Rendered", icon: "book-open" },
];

function title(): string {
  if (page() === "new") return "New project";
  if (resultsTab() && resultsInFront()) return `Search: ${searchQuery()?.text ?? ""}`;
  const file = openPath();
  return file === "" ? (project()?.name ?? "") : (file.split("/").pop() ?? file);
}

function subtitle(): string {
  if (page() !== "project") return "";
  if (resultsTab() && resultsInFront()) return project()?.name ?? "";
  if (openPath() === "") return "";
  const folder = openPath().split("/").slice(0, -1).join("/");
  const name = project()?.name ?? "";
  return folder === "" ? name : `${name} / ${folder}`;
}

/** The title bar: the file's name and place, the search, and the window's buttons. */
export function Header(props: {
  sidebarOpen: boolean;
  appearance: Mode;
  onShowSidebar: () => void;
  onMode: (mode: ViewMode) => void;
  onAppearance: (mode: Mode) => void;
  onTheme: (name: ThemeName) => void;
}) {
  const reading = () => page() === "project" && openPath() !== "" && !(resultsTab() && resultsInFront());
  const shown = () => MODES.find((each) => each.value === mode()) ?? MODES[0];
  const nameWidth = signal(0);
  const rowMinWidth = () => nameWidth() + (page() === "project" ? SEARCH_MIN_WIDTH + 4 * ROW_GAP : 0);
  return (
    <TitleBar class="pl-3">
      <If when={!props.sidebarOpen}>
        <View class="flex-row items-center gap-2">
          <Logo product="mdviewer" />
          <IconButton icon="panel-left" label="Show sidebar" onPress={props.onShowSidebar} />
        </View>
      </If>

      {/* The row is never narrower than the name: nsh doesn't round a layout, so any shrink would cut the name short. */}
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
        <If when={reading() && mode() === "split"}>
          <IconButton
            icon="arrow-down-up"
            label={scrollTogether() ? "Scroll the sides on their own" : "Scroll the sides together"}
            on={scrollTogether()}
            class="ml-1"
            onPress={() => scrollTogether.set(!scrollTogether())}
          />
        </If>
        <If when={reading()}>
          <IconButton icon="map" label={mapOpen() ? "Hide the map" : "Show the map"} on={mapOpen()} class="ml-1" onPress={() => mapOpen.set(!mapOpen())} />
        </If>
        <ThemeMenu mode={props.appearance} onMode={props.onAppearance} onPick={props.onTheme} class="ml-1" />
        <IconButton icon="maximize" label="Full screen" onPress={toggleFullScreen} />
      </View>
    </TitleBar>
  );
}
