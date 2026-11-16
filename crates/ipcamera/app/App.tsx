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

/** The narrowest the PTZ panel gets, in points. */
const PTZ_MIN_WIDTH = 180;
/** The narrowest the video gets, in points. Above this, the PTZ panel gives way first. */
const VIDEO_MIN_WIDTH = 200;
/** Below this width the sidebar closes by itself, leaving the room to the video. */
const SIDEBAR_MIN_WINDOW = 640;

// The shared controls draw the themes through this app's `Themes` module (`dp-ui`'s), before anything shows.
connectThemes(Themes);
// The saved appearance, before the first frame.
showAppearance(Preferences.load().appearance);

// The header and the sidebar's top row are the title bar, so the system draws none (`nsh.json`'s
// `window`). Its window buttons stay, over the header's right end and as tall as it (`h-12`). Full
// screen hides them with the header.

/** While this is mounted, Escape leaves full screen. F11 needs nothing: nsh handles it natively. */
function EscapeLeavesFullScreen() {
  keymap.bind("escape", () => leaveFullScreen());
  return null;
}

/** The dashboard, the cameras, the groups, the videos, the events or the snapshots, as `kind` says. */
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
  // A file that could not be read was moved aside or left alone: say so once.
  for (const problem of Library.problems()) notify(problem, "warn");

  const preferences = Preferences.load();
  const ptzOpen = signal(!preferences.railCollapsed);
  const ptzWidth = signal(preferences.railWidth);
  const sidebarOpen = signal(true);
  const sidebarWidth = signal(preferences.sidebarWidth);
  const fullScreen = isFullScreen;
  // Where the window was left, before it shows.
  keepWindowPlace(AppWindow.place(), (place) => void AppWindow.savePlace(place));
  const win = currentWindow();

  const savePreferences = () =>
    void Preferences.save({
      railCollapsed: !ptzOpen(),
      railWidth: ptzWidth(),
      sidebarWidth: sidebarWidth(),
      appearance: appearance.saved(),
    });
  // The mode, the themes, the texture and the elevation, as Settings and the theme menu change them.
  const appearance = keepAppearance(preferences.appearance, savePreferences);

  const togglePtz = () => {
    ptzOpen.set(!ptzOpen());
    savePreferences();
  };

  const focused = () => cameraOf(focusedId());
  /** Whether the dashboard, the cameras, the groups, the snapshots, the videos or the events have the window, in place of the live video. */
  const library = () => replacesStage(page());

  /** The width the video and the PTZ panel share. */
  const stageWidth = () => windowSize().width - (sidebarOpen() && !fullScreen() ? sidebarWidth() : 0);
  /** Whether both fit. In a window too small for them, the video wins. */
  const roomForPtz = () => stageWidth() >= VIDEO_MIN_WIDTH + PTZ_MIN_WIDTH;

  keymap.bind("mod+b", () => sidebarOpen.set(!sidebarOpen()));

  // The page's name, not the camera's whole state, which changes every second with its stats: the
  // effect then runs only when the title would read differently.
  const title = computed(() => (pageName() ? `IP Camera — ${pageName()}` : "IP Camera"));
  effect(() => win.set({ title: title() }));

  effect(() => {
    if (windowSize().width < SIDEBAR_MIN_WINDOW) sidebarOpen.set(false);
  });

  // A deleted view or camera can't stay on the stage: show the first view, else the first camera.
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
    const count = currentView()?.tiles.length ?? 0;
    if (count > 0 && selectedTile() >= count) selectedTile.set(0);
  });

  // A layout being edited is dropped when its view leaves the stage, as Cancel drops it.
  effect(() => {
    const editing = draft();
    if (editing && currentView()?.id !== editing.view) cancelEditing();
  });

  // A recording that could not write its file stops by itself: say why, once each time.
  const told = new Map<string, string>();
  effect(() => {
    for (const camera of states()) {
      if (camera.recordProblem !== "" && told.get(camera.id) !== camera.recordProblem) notify(`${camera.name}: ${camera.recordProblem}`, "warn");
      told.set(camera.id, camera.recordProblem);
    }
  });

  // Rust connects the streams on screen, the last view's and the cameras opened on their own
  // since, each on the stream its tile picks; decodes only those on screen, disconnects the rest,
  // and lets only the selected tile's be heard. Without a password a camera waits to be asked, so
  // launching never fails a login. While a layout is edited, its frames' pictures are the ones on
  // screen. The key keeps this to real changes, not every library refresh.
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

      {/* A server with a password shows nothing of itself until someone signs in. */}
      <If when={signInNeeded()}>
        <SignInPage />
      </If>

      <ResizablePanelGroup
        axis="horizontal"
        class={signInNeeded() ? "hidden" : "min-h-0 flex-1"}
        // No line between the sidebar and the page, which would run into the title bar but stop at the status bar: a
        // grip in the middle of the edge shows instead while the pointer is over it, fuller while it is dragged.
        handleClass={GRIP_HANDLE.line}
        handleThumbClass={GRIP_HANDLE.thumb}
        handleAccessibilityLabel="Resize the sidebar"
      >
        {/* As wide as it was last dragged to, kept as the window resizes; the page takes the rest. */}
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
            {/* The video and the PTZ panel are islands on the frame; full screen's video fills the window instead. */}
            <Islands flat={fullScreen()}>
              <ResizablePanelGroup
                axis="horizontal"
                class="flex-1"
                // The handle lies in the gap between the video and the PTZ panel, with no line, as the sidebar's.
                handleClass={GRIP_HANDLE.line}
                handleThumbClass={GRIP_HANDLE.thumb}
                handleAccessibilityLabel="Resize the PTZ panel"
              >
                <ResizablePanel minSize={VIDEO_MIN_WIDTH}>
                  {/* The video fills its island to the edge, which has no outline to lie inside, the island
                      rounding its outer corners (`Stage` in `views/stage.tsx`); in full screen both are square and
                      fill the window. */}
                  <Island kind="document" flat={fullScreen()} edgeless class="flex-1">
                    <Stage />
                  </Island>
                </ResizablePanel>
                <ResizablePanel
                  fixed
                  // A `fixed` panel does not shrink by itself, so a narrow window asks for less. The saved
                  // width stays as it is, for when there is room again.
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

      {/* The app's, under every page, not only the video's, and as wide as the window: under the sidebar too. */}
      <If when={!fullScreen() && !signInNeeded()}>
        <StatusBar />
      </If>

      <CameraDialog />
      <BulkDialog />
      <GroupDialog />
      {/* Over whatever page shows, which stays as it was behind it. */}
      <AppSettings appearance={appearance} />
      <NameDialog />
      <Toaster />
      <MenuLayer />
    </View>
  );
}

mount(App);
