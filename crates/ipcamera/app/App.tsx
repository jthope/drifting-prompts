import { computed, effect, keymap, mount, signal, untracked, View, windowSize } from "nsh";
import { ResizablePanel, ResizablePanelGroup } from "nsh/base";
import { currentWindow } from "nsh/desktop";
import { AppWindow, Cameras, Library, Preferences, type ShownStream, Themes } from "ipcamera/native";
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
  NameDialog,
  notify,
  showAppearance,
  Toaster,
} from "../../dp/ui";
import { cancelEditing, draft, draftStreams } from "./views/arrange";
import { BulkDialog } from "./views/bulk";
import { CamerasPage } from "./views/cameras";
import { DashboardPage } from "./views/dashboard";
import { followEvents } from "./views/eventlog";
import { EventsPage } from "./views/events";
import { Gallery } from "./views/gallery";
import { GroupsPage } from "./views/groups";
import { Header } from "./views/header";
import { GroupDialog } from "./views/members";
import { cameraOf, currentView, focusedId, focusedStream, keptStreams, listen, type Page, page, pageName, replacesStage, selectedTile, show, signInNeeded, states, streamsOnScreen, views } from "./views/model";
import { AppSettings } from "./views/preferences";
import { PtzPanel } from "./views/ptz";
import { CameraDialog } from "./views/settings";
import { SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH, Sidebar } from "./views/sidebar";
import { SignInPage } from "./views/signin";
import { Stage } from "./views/stage";
import { StatusBar } from "./views/statusbar";
import { VideosPage } from "./views/videos";

const PTZ_MIN_WIDTH = 180;
const VIDEO_MIN_WIDTH = 200;
const SIDEBAR_MIN_WINDOW = 640;

// Before anything shows, so the shared controls draw this app's themes.
connectThemes(Themes);
// The saved appearance, before the first frame.
showAppearance(Preferences.load().appearance);

function EscapeLeavesFullScreen() {
  keymap.bind("escape", () => leaveFullScreen());
  return null;
}

function OwnPage(props: { kind: Page["kind"] }) {
  const media = () => (
    <If when={props.kind === "videos"} else={<If when={props.kind === "events"} else={<Gallery />}><EventsPage /></If>}>
      <VideosPage />
    </If>
  );
  return (
    <If
      when={props.kind === "dashboard"}
      else={
        <If
          when={props.kind === "cameras"}
          else={
            <If when={props.kind === "groups"} else={media()}>
              <GroupsPage />
            </If>
          }
        >
          <CamerasPage />
        </If>
      }
    >
      <DashboardPage />
    </If>
  );
}

function App() {
  listen();
  followEvents();
  for (const problem of Library.problems()) notify(problem, "warn");

  const preferences = Preferences.load();
  const ptzOpen = signal(!preferences.railCollapsed);
  const ptzWidth = signal(preferences.railWidth);
  const sidebarOpen = signal(true);
  const sidebarWidth = signal(preferences.sidebarWidth);
  const fullScreen = isFullScreen;
  // Before the window shows.
  keepWindowPlace(AppWindow.place(), (place) => void AppWindow.savePlace(place));
  const win = currentWindow();

  const savePreferences = () =>
    void Preferences.save({
      railCollapsed: !ptzOpen(),
      railWidth: ptzWidth(),
      sidebarWidth: sidebarWidth(),
      appearance: appearance.saved(),
    });
  const appearance = keepAppearance(preferences.appearance, savePreferences);

  const togglePtz = () => {
    ptzOpen.set(!ptzOpen());
    savePreferences();
  };

  const focused = () => cameraOf(focusedId());
  const library = () => replacesStage(page());

  const stageWidth = () => windowSize().width - (sidebarOpen() && !fullScreen() ? sidebarWidth() : 0);
  const roomForPtz = () => stageWidth() >= VIDEO_MIN_WIDTH + PTZ_MIN_WIDTH;

  keymap.bind("mod+b", () => sidebarOpen.set(!sidebarOpen()));

  // A computed title, so the effect runs only when it reads differently, not with every camera state.
  const title = computed(() => (pageName() ? `IP Camera — ${pageName()}` : "IP Camera"));
  effect(() => win.set({ title: title() }));

  effect(() => {
    if (windowSize().width < SIDEBAR_MIN_WINDOW) sidebarOpen.set(false);
  });

  effect(() => {
    const shown = page();
    const gone =
      (shown.kind === "view" && !views().some((view) => view.id === shown.id)) ||
      (shown.kind === "camera" && !states().some((camera) => camera.id === shown.id)) ||
      (shown.kind === "none" && (views().length > 0 || states().length > 0));
    if (!gone) return;
    const view = untracked(views)[0];
    const camera = untracked(states)[0];
    show(view ? { kind: "view", id: view.id } : camera ? { kind: "camera", id: camera.id } : { kind: "none" });
  });

  // A smaller layout can leave the selected tile outside it.
  effect(() => {
    const tile = selectedTile();
    const count = currentView()?.tiles.length ?? 0;
    if (tile !== null && count > 0 && tile >= count) selectedTile.set(null);
  });

  effect(() => {
    const editing = draft();
    if (editing && currentView()?.id !== editing.view) cancelEditing();
  });

  const told = new Map<string, string>();
  effect(() => {
    for (const camera of states()) {
      if (camera.recordProblem !== "" && told.get(camera.id) !== camera.recordProblem) notify(`${camera.name}: ${camera.recordProblem}`, "warn");
      told.set(camera.id, camera.recordProblem);
    }
  });

  // The JSON key keeps this to real changes, not every library refresh.
  const screen = computed(() => JSON.stringify([draftStreams() ?? streamsOnScreen(), keptStreams(), { camera: focusedId(), stream: focusedStream() }]));
  effect(() => {
    const [shown, keep, selected] = JSON.parse(screen()) as [ShownStream[], ShownStream[], ShownStream];
    Cameras.setOnScreen(shown, keep, selected).catch(() => {});
  });

  return (
    <View class="flex-1 bg-background">
      <If when={fullScreen()}>
        <EscapeLeavesFullScreen />
      </If>

      <If when={signInNeeded()}>
        <SignInPage />
      </If>

      <ResizablePanelGroup
        axis="horizontal"
        class={signInNeeded() ? "hidden" : "min-h-0 flex-1"}
        handleClass={GRIP_HANDLE.line}
        handleThumbClass={GRIP_HANDLE.thumb}
        handleAccessibilityLabel="Resize the sidebar"
      >
        <ResizablePanel
          fixed
          size={sidebarWidth()}
          minSize={SIDEBAR_MIN_WIDTH}
          maxSize={SIDEBAR_MAX_WIDTH}
          visible={sidebarOpen() && !fullScreen() && !signInNeeded()}
          onResize={({ size }) => {
            sidebarWidth.set(size);
            savePreferences();
          }}
        >
          <If when={sidebarOpen() && !fullScreen() && !signInNeeded()}>
            <Sidebar onHide={() => sidebarOpen.set(false)} />
          </If>
        </ResizablePanel>

        <ResizablePanel minSize={VIDEO_MIN_WIDTH}>
          <If when={!fullScreen()}>
            <Header
              sidebarOpen={sidebarOpen()}
              ptzOpen={ptzOpen()}
              appearance={appearance.mode()}
              onShowSidebar={() => sidebarOpen.set(true)}
              onTogglePtz={togglePtz}
              onAppearance={appearance.setMode}
              onTheme={appearance.pickTheme}
            />
          </If>

          <If when={!library()} else={<OwnPage kind={page().kind} />}>
            <Islands flat={fullScreen()}>
              <ResizablePanelGroup
                axis="horizontal"
                class="flex-1"
                handleClass={GRIP_HANDLE.line}
                handleThumbClass={GRIP_HANDLE.thumb}
                handleAccessibilityLabel="Resize the PTZ panel"
              >
                <ResizablePanel minSize={VIDEO_MIN_WIDTH}>
                  <Island kind="document" flat={fullScreen()} edgeless class="flex-1">
                    <Stage />
                  </Island>
                </ResizablePanel>
                <ResizablePanel
                  fixed
                  // A `fixed` panel doesn't shrink by itself, so a narrow window asks for less.
                  size={Math.min(ptzWidth(), stageWidth() - VIDEO_MIN_WIDTH)}
                  minSize={PTZ_MIN_WIDTH}
                  visible={ptzOpen() && !fullScreen() && roomForPtz()}
                  onResize={({ size }) => {
                    ptzWidth.set(size);
                    savePreferences();
                  }}
                >
                  <Island kind="tool" class="flex-1">
                    <PtzPanel camera={focusedId()} status={focused()?.ptzStatus ?? "Select a camera to move it."} onHide={togglePtz} />
                  </Island>
                </ResizablePanel>
              </ResizablePanelGroup>
            </Islands>
          </If>
        </ResizablePanel>
      </ResizablePanelGroup>

      <If when={!fullScreen() && !signInNeeded()}>
        <StatusBar />
      </If>

      <CameraDialog />
      <BulkDialog />
      <GroupDialog />
      <AppSettings appearance={appearance} />
      <NameDialog />
      <Toaster />
      <MenuLayer />
    </View>
  );
}

mount(App);
