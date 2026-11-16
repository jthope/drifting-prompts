import { computed, effect, keymap, mount, signal, View, windowSize } from "nsh";
import { ResizablePanel, ResizablePanelGroup, Scrollbar } from "nsh/base";
import { currentWindow } from "nsh/desktop";
import { AppWindow, Preferences, Projects, Themes, type ViewMode } from "mdviewer/native";
import {
  connectThemes,
  GRIP_HANDLE,
  Island,
  Islands,
  isFullScreen,
  keepAppearance,
  keepWindowPlace,
  leaveFullScreen,
  MenuLayer,
  notify,
  SCROLLBARS,
  showAppearance,
  Toaster,
} from "../../dp/ui";
import { closeFront, failed, openDropped, reopenClosed, setModeInFront, stepMatch, stepTab } from "./views/actions";
import { FILES_MIN_WIDTH, FilesPanel } from "./views/files";
import { Header } from "./views/header";
import { defaultMode, filesOpen, filesView, listen, mapOpen, mode, openPath, outlineOpen, page, project, resultsInFront, resultsTab, tabOverflow } from "./views/model";
import { NewProject } from "./views/newproject";
import { OUTLINE_WIDTH, Outline, Reader } from "./views/reader";
import { ResultsView } from "./views/results";
import { AppSettings } from "./views/settings";
import { SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH, Sidebar } from "./views/sidebar";
import { StatusBar } from "./views/statusbar";
import { TabStrip } from "./views/tabstrip";

/** The narrowest the file gets, in points. Above this, the outline gives way first, then the files panel. */
const READER_MIN_WIDTH = 320;
/** Below this width the sidebar closes by itself, leaving the room to the file. */
const SIDEBAR_MIN_WINDOW = 640;

// The shared controls draw the themes through this app's `Themes` module (`dp-ui`'s), before anything shows.
connectThemes(Themes);
// Every scroll view's bar is the suite's: always there while there is more than fits, and wide enough to take hold of.
Scrollbar.setTheme(SCROLLBARS);

// The saved preferences, before the first frame: the appearance, how files show and list, and how the tabs fit.
const saved = Preferences.load();
showAppearance(saved.appearance);
defaultMode.set(saved.mode);
filesView.set(saved.filesView);
tabOverflow.set(saved.tabOverflow);
outlineOpen.set(saved.outlineOpen);
filesOpen.set(saved.filesOpen);
mapOpen.set(saved.mapOpen);

// The header and the sidebar's top row are the title bar, so the system draws none (`nsh.json`'s
// `window`). Its window buttons stay, over the header's right end and as tall as it (`h-12`). Full
// screen hides them with the header.

/** While this is mounted, Escape leaves full screen. F11 needs nothing: nsh handles it natively. */
function EscapeLeavesFullScreen() {
  keymap.bind("escape", () => leaveFullScreen());
  return null;
}

function App() {
  listen();
  // A settings file that could not be read was moved aside or left alone: say so once.
  const problem = Projects.problem();
  if (problem !== "") notify(problem, "warn");
  // Where the window was left, before it shows.
  keepWindowPlace(AppWindow.place(), (place) => void AppWindow.savePlace(place));
  const win = currentWindow();

  const sidebarOpen = signal(true);
  const sidebarWidth = signal(saved.sidebarWidth);
  const filesWidth = signal(saved.filesWidth);
  const fullScreen = isFullScreen;

  const savePreferences = () =>
    Preferences.save({
      appearance: appearance.saved(),
      sidebarWidth: sidebarWidth(),
      filesWidth: filesWidth(),
      filesOpen: filesOpen(),
      outlineOpen: outlineOpen(),
      mapOpen: mapOpen(),
      mode: defaultMode(),
      filesView: filesView(),
      tabOverflow: tabOverflow(),
    });

  // The panels, the files' view, how new tabs show and how the tabs fit change from several places (a
  // pane's own button, the tab strip, the status bar, Settings): whatever changed them, they are saved.
  let first = true;
  effect(() => {
    outlineOpen();
    filesOpen();
    mapOpen();
    filesView();
    defaultMode();
    tabOverflow();
    if (first) first = false;
    else savePreferences();
  });

  // The mode, the themes, the texture and the elevation, as Settings and the theme menu change them.
  const appearance = keepAppearance(saved.appearance, savePreferences);

  /** The width the files panel, the file and its outline share. */
  const mainWidth = () => windowSize().width - (sidebarOpen() && !fullScreen() ? sidebarWidth() : 0);
  /** The files panel's width: the user's, less in a window too narrow for it beside the file. */
  const filesSize = () => Math.min(filesWidth(), mainWidth() - READER_MIN_WIDTH);
  /** Whether the files panel shows. In a window too small for it and the file, the file wins. */
  const filesShown = () => page() === "project" && filesOpen() && !fullScreen() && mainWidth() >= READER_MIN_WIDTH + FILES_MIN_WIDTH;
  /** Whether the outline shows: it gives way before the files panel does. */
  const outlineShown = () => outlineOpen() && mainWidth() - (filesShown() ? filesSize() : 0) >= READER_MIN_WIDTH + OUTLINE_WIDTH;

  keymap.bind("mod+b", () => sidebarOpen.set(!sidebarOpen()));
  // Raw, Split and Rendered in turn for the file in front, as the header's menu lists them.
  keymap.bind("mod+e", () => {
    if (openPath() === "") return;
    const order: ViewMode[] = ["raw", "split", "rendered"];
    setModeInFront(order[(order.indexOf(mode()) + 1) % order.length]);
  });
  // The tabs, as a browser's and an editor's.
  keymap.bind({
    "mod+w": () => closeFront(),
    "ctrl+tab": () => stepTab(1),
    "ctrl+shift+tab": () => stepTab(-1),
    "mod+shift+t": () => reopenClosed(),
  });
  // The search's matches, across the files, as an editor's find steps through them; Raw's editor would step through
  // its own file's alone.
  keymap.bind({
    f3: () => stepMatch(1),
    "shift+f3": () => stepMatch(-1),
  });

  const title = computed(() => {
    const file = openPath().split("/").pop() ?? "";
    const name = project()?.name;
    return name ? `${file === "" ? name : `${file} — ${name}`} — Markdown Viewer` : "Markdown Viewer";
  });
  effect(() => win.set({ title: title() }));

  effect(() => {
    if (windowSize().width < SIDEBAR_MIN_WINDOW) sidebarOpen.set(false);
  });

  const removeOpen = () => {
    const shown = project();
    if (shown) Projects.remove(shown.path).catch(failed);
  };

  return (
    <View testID="window" class="flex-1 bg-background" onFileDrop={(event) => openDropped(event.uris)}>
      <If when={fullScreen()}>
        <EscapeLeavesFullScreen />
      </If>

      <ResizablePanelGroup
        axis="horizontal"
        class="min-h-0 flex-1"
        // No line between the sidebar and the page, as IP Camera has none: a grip in the middle of the edge shows
        // instead while the pointer is over it, fuller while it is dragged.
        handleClass={GRIP_HANDLE.line}
        handleThumbClass={GRIP_HANDLE.thumb}
        handleAccessibilityLabel="Resize the sidebar"
      >
        <ResizablePanel
          fixed
          size={sidebarWidth()}
          minSize={SIDEBAR_MIN_WIDTH}
          maxSize={SIDEBAR_MAX_WIDTH}
          visible={sidebarOpen() && !fullScreen()}
          onResize={({ size }) => {
            sidebarWidth.set(size);
            savePreferences();
          }}
        >
          <If when={sidebarOpen() && !fullScreen()}>
            <Sidebar onHide={() => sidebarOpen.set(false)} />
          </If>
        </ResizablePanel>

        <ResizablePanel minSize={READER_MIN_WIDTH}>
          <If when={!fullScreen()}>
            <Header
              sidebarOpen={sidebarOpen()}
              appearance={appearance.mode()}
              onShowSidebar={() => sidebarOpen.set(true)}
              onMode={setModeInFront}
              onAppearance={appearance.setMode}
              onTheme={appearance.pickTheme}
            />
          </If>

          <If when={page() === "new"}>
            <NewProject />
          </If>
          <If when={page() === "project"}>
            {/* The panes are islands on the frame, a gap apart and a gap from its edges. */}
            <Islands>
              <ResizablePanelGroup
                axis="horizontal"
                class="flex-1"
                // The handle lies in the gap between the files and the file, with no line: a grip shows there while
                // the pointer is over it, fuller while it is dragged, as the sidebar's does.
                handleClass={GRIP_HANDLE.line}
                handleThumbClass={GRIP_HANDLE.thumb}
                handleAccessibilityLabel="Resize the files panel"
              >
                <ResizablePanel
                  fixed
                  // A `fixed` panel does not shrink by itself, so a narrow window asks for less. The saved
                  // width stays as it is, for when there is room again.
                  size={filesSize()}
                  minSize={FILES_MIN_WIDTH}
                  visible={filesShown()}
                  onResize={({ size }) => {
                    filesWidth.set(size);
                    savePreferences();
                  }}
                >
                  <Island kind="tool" class="flex-1">
                    <FilesPanel onHide={() => filesOpen.set(false)} />
                  </Island>
                </ResizablePanel>
                <ResizablePanel minSize={READER_MIN_WIDTH}>
                  <View class="min-h-0 flex-1 flex-row">
                    <Island kind="document" class="min-w-0 flex-1">
                      {/* Full screen keeps only the file and its outline. */}
                      <If when={!fullScreen()}>
                        <TabStrip filesShown={filesShown()} outlineShown={outlineShown()} />
                      </If>
                      <If when={resultsTab() && resultsInFront()} else={<Reader rail={!outlineShown()} onRemove={removeOpen} />}>
                        <ResultsView />
                      </If>
                    </Island>
                    <If when={outlineShown()}>
                      <Outline onHide={() => outlineOpen.set(false)} />
                    </If>
                  </View>
                </ResizablePanel>
              </ResizablePanelGroup>
            </Islands>
          </If>
        </ResizablePanel>
      </ResizablePanelGroup>

      {/* The app's, under every page, and as wide as the window: under the sidebar too. */}
      <If when={!fullScreen()}>
        <StatusBar />
      </If>

      {/* Over the project or New, which stays as it was behind it. */}
      <AppSettings appearance={appearance} />
      <Toaster />
      <MenuLayer />
    </View>
  );
}

mount(App);
