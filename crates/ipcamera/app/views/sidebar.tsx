import { type JSX, keymap, signal, Text, View } from "nsh";
import type { CameraState, GroupInfo, ViewInfo } from "ipcamera/native";
import { createReorder, EdgedScroll, IconButton, LiveDot, look, NavFolder, NavRow, NavSection, Reorderable, SidebarFrame, StatusDot } from "../../../dp/ui";
import { cameraMenu, groupMenu, moveCamera, newCamera, newGroup, newView, nudgeCamera, showing, viewMenu } from "./actions";
import { cameraOf, groups, page, recording, show, states, views } from "./model";
import { showSettings } from "./preferences";

/**
 * The narrowest and widest the sidebar is dragged to, in points. Its width is
 * the user's, saved with the preferences (`sidebarWidth`, 232 at first); the
 * sidebar fills the panel `App.tsx` gives it.
 */
export const SIDEBAR_MIN_WIDTH = 180;
export const SIDEBAR_MAX_WIDTH = 400;

/** The red dot at the end of a row while something it shows records. */
const recordingDot = (on: boolean) => (on ? <LiveDot label="Recording" /> : undefined);

/** A camera's row: under All cameras when it has no `group`, where its `grip` moves it among the cameras. */
function CameraRow(props: { camera: CameraState; group?: GroupInfo; grip?: JSX.Element }) {
  return (
    <NavRow
      label={props.camera.name}
      leading={<StatusDot tone={props.camera.tone} class="mx-1" />}
      trailing={recordingDot(props.camera.recording)}
      indent
      grip={props.grip}
      selected={showing("camera", props.camera.id)}
      menu={() => cameraMenu(props.camera, props.group, props.group === undefined)}
      onPress={() => show({ kind: "camera", id: props.camera.id })}
    />
  );
}

function ViewRow(props: { view: ViewInfo }) {
  return (
    <NavRow
      label={props.view.name}
      icon="layout-grid"
      selected={showing("view", props.view.id)}
      menu={() => viewMenu(props.view)}
      onPress={() => show({ kind: "view", id: props.view.id })}
    />
  );
}

/**
 * The app's navigation: the dashboard, the videos, the snapshots and the events, the saved
 * views, then the cameras, all of them and by group. Only the cameras scroll,
 * under their heading, which keeps its buttons in reach (`EdgedScroll`): a line
 * shows under the heading while cameras lie above it, with the list fading into
 * the sidebar under it and the elevation's shade over the fade, and the same,
 * facing up, over the rows pinned at the bottom while more lie below. The Cameras and Groups pages (every camera,
 * and every group, in a table, to change many at once) and the app's settings, which open in a dialog over the page,
 * stay at the bottom, in reach however long the list grows. A camera in several groups is listed under
 * each, and is the same camera everywhere.
 *
 * All cameras is in the cameras' own order, which the user changes there: by
 * dragging a camera by its grip (`Reorderable`), from its menu, or with Alt+Up
 * and Alt+Down while it is open on its own, the row the sidebar shows
 * selected. A group's cameras keep the group's order.
 */
export function Sidebar(props: { onHide: () => void }) {
  /** The folders folded away: `"all"`, or a group's id. */
  const folded = signal<string[]>([]);
  const isOpen = (key: string) => !folded().includes(key);
  const toggle = (key: string) => folded.set(isOpen(key) ? [...folded(), key] : folded().filter((each) => each !== key));

  const membersOf = (group: GroupInfo) => group.cameras.map((id) => cameraOf(id)).filter((camera): camera is CameraState => camera !== undefined);

  const order = createReorder({ count: () => states().length, onMove: (from, to) => moveCamera(states()[from].id, to) });
  /** The camera open on its own: what Alt+Up and Alt+Down move. */
  const nudge = (by: -1 | 1) => {
    const shown = page();
    if (shown.kind === "camera") nudgeCamera(shown.id, by);
  };
  keymap.bind({ "alt+up": () => nudge(-1), "alt+down": () => nudge(1) }, { when: "notEditing" });

  return (
    <SidebarFrame product="ipcamera" name="IP Camera" onHide={props.onHide}>
      {/* Room under the cameras' heading, so the edge the list scrolls under keeps clear of its label. */}
      <View class="gap-1 px-2 pb-1.5 pt-2">
        <NavRow label="Dashboard" icon="layout-dashboard" selected={page().kind === "dashboard"} onPress={() => show({ kind: "dashboard" })} />
        <NavRow label="Videos" icon="film" trailing={recordingDot(recording())} selected={page().kind === "videos"} onPress={() => show({ kind: "videos" })} />
        <NavRow label="Snapshots" icon="images" selected={page().kind === "snapshots"} onPress={() => show({ kind: "snapshots" })} />
        <NavRow label="Events" icon="bell" selected={page().kind === "events"} onPress={() => show({ kind: "events" })} />
        <NavSection title="Views">
          <IconButton icon="plus" label="New view" onPress={newView} />
        </NavSection>
        <Each of={views()} key={(view: ViewInfo) => view.id}>
          {(view) => <ViewRow view={view()} />}
        </Each>
        <NavSection title="Cameras">
          <IconButton icon="folder-plus" label="New group" onPress={() => newGroup()} />
          <IconButton icon="plus" label="Add camera" onPress={() => newCamera()} />
        </NavSection>
      </View>

      {/* The cameras fade into the sidebar at each end while more lie that way, with the elevation's shade over the fade: it lies on the same plain color at both ends, so they read alike. */}
      <EdgedScroll testID="sidebar-cameras" place="sidebar" fade={look().tiers.frame} contentClass="gap-1 px-2 pb-2 pt-1">
        <NavFolder label="All cameras" icon="video" open={isOpen("all")} onToggle={() => toggle("all")} />
        <If when={isOpen("all")}>
          <Each of={states()} key={(camera: CameraState) => camera.id}>
            {(camera, index) => (
              <Reorderable reorder={order} index={index()} label={camera().name} shown={showing("camera", camera().id)}>
                {(grip) => <CameraRow camera={camera()} grip={states().length > 1 ? grip : undefined} />}
              </Reorderable>
            )}
          </Each>
        </If>

        <Each of={groups()} key={(group: GroupInfo) => group.id}>
          {(group) => (
            <>
              <NavFolder
                label={group().name}
                icon="folder"
                open={isOpen(group().id)}
                menu={() => groupMenu(group())}
                onToggle={() => toggle(group().id)}
              />
              <If when={isOpen(group().id)}>
                <Each of={membersOf(group())} key={(camera: CameraState) => camera.id}>
                  {(camera) => <CameraRow camera={camera()} group={group()} />}
                </Each>
                <If when={group().cameras.length === 0}>
                  <Text class="py-1.5 pl-7 text-xs text-muted-foreground">No cameras yet</Text>
                </If>
              </If>
            </>
          )}
        </Each>
      </EdgedScroll>

      {/* No border of its own: the list's bottom edge draws its line there while more cameras lie below, as its top edge does under the heading. */}
      <View class="gap-1 p-2">
        <NavRow label="Cameras" icon="cctv" selected={page().kind === "cameras"} onPress={() => show({ kind: "cameras" })} />
        <NavRow label="Groups" icon="folder" selected={page().kind === "groups"} onPress={() => show({ kind: "groups" })} />
        {/* Over the page, which stays selected behind it. */}
        <NavRow label="Settings" icon="settings" onPress={() => showSettings()} />
      </View>
    </SidebarFrame>
  );
}
