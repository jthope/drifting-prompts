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

const READER_MIN_WIDTH = 320;
const SIDEBAR_MIN_WINDOW = 640;

// Before anything shows.
connectThemes(Themes);
Scrollbar.setTheme(SCROLLBARS);

const saved = Preferences.load();
showAppearance(saved.appearance);
defaultMode.set(saved.mode);
filesView.set(saved.filesView);
tabOverflow.set(saved.tabOverflow);
outlineOpen.set(saved.outlineOpen);
filesOpen.set(saved.filesOpen);
mapOpen.set(saved.mapOpen);

function EscapeLeavesFullScreen() {
  keymap.bind("escape", () => leaveFullScreen());
  return null;
}

function App() {
  listen();
  const problem = Projects.problem();
  if (problem !== "") notify(problem, "warn");
  // Before the window shows.
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

  const appearance = keepAppearance(saved.appearance, savePreferences);

  const mainWidth = () => windowSize().width - (sidebarOpen() && !fullScreen() ? sidebarWidth() : 0);
  const filesSize = () => Math.min(filesWidth(), mainWidth() - READER_MIN_WIDTH);
  const filesShown = () => page() === "project" && filesOpen() && !fullScreen() && mainWidth() >= READER_MIN_WIDTH + FILES_MIN_WIDTH;
  const outlineShown = () => outlineOpen() && mainWidth() - (filesShown() ? filesSize() : 0) >= READER_MIN_WIDTH + OUTLINE_WIDTH;

  keymap.bind("mod+b", () => sidebarOpen.set(!sidebarOpen()));
  keymap.bind("mod+e", () => {
    if (openPath() === "") return;
    const order: ViewMode[] = ["raw", "split", "rendered"];
    setModeInFront(order[(order.indexOf(mode()) + 1) % order.length]);
  });
  keymap.bind({
    "mod+w": () => closeFront(),
    "ctrl+tab": () => stepTab(1),
    "ctrl+shift+tab": () => stepTab(-1),
    "mod+shift+t": () => reopenClosed(),
  });
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
            <Islands>
              <ResizablePanelGroup
                axis="horizontal"
                class="flex-1"
                handleClass={GRIP_HANDLE.line}
                handleThumbClass={GRIP_HANDLE.thumb}
                handleAccessibilityLabel="Resize the files panel"
              >
                <ResizablePanel
                  fixed
                  // A `fixed` panel doesn't shrink by itself, so a narrow window asks for less.
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

      <If when={!fullScreen()}>
        <StatusBar />
      </If>

      <AppSettings appearance={appearance} />
      <Toaster />
      <MenuLayer />
    </View>
  );
}

mount(App);
