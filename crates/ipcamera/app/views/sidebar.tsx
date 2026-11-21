import { type JSX, keymap, signal, Text, View } from "nsh";
import type { CameraState, GroupInfo, ViewInfo } from "ipcamera/native";
import { createReorder, EdgedScroll, IconButton, LiveDot, look, NavFolder, NavRow, NavSection, Reorderable, SidebarFrame, StatusDot } from "../../../dp/ui";
import { cameraMenu, groupMenu, moveCamera, newCamera, newGroup, newView, nudgeCamera, showing, viewMenu } from "./actions";
import { cameraOf, groups, page, recording, show, states, views } from "./model";
import { showSettings } from "./preferences";

/** The narrowest and widest the sidebar is dragged to, in points. */
export const SIDEBAR_MIN_WIDTH = 180;
export const SIDEBAR_MAX_WIDTH = 400;

const recordingDot = (on: boolean) => (on ? <LiveDot label="Recording" /> : undefined);

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

/** The app's navigation: the pages, the saved views, then the cameras, all of them and by group. */
export function Sidebar(props: { onHide: () => void }) {
  const folded = signal<string[]>([]);
  const isOpen = (key: string) => !folded().includes(key);
  const toggle = (key: string) => folded.set(isOpen(key) ? [...folded(), key] : folded().filter((each) => each !== key));

  const membersOf = (group: GroupInfo) => group.cameras.map((id) => cameraOf(id)).filter((camera): camera is CameraState => camera !== undefined);

  const order = createReorder({ count: () => states().length, onMove: (from, to) => moveCamera(states()[from].id, to) });
  const nudge = (by: -1 | 1) => {
    const shown = page();
    if (shown.kind === "camera") nudgeCamera(shown.id, by);
  };
  keymap.bind({ "alt+up": () => nudge(-1), "alt+down": () => nudge(1) }, { when: "notEditing" });

  return (
    <SidebarFrame product="ipcamera" name="IP Camera" onHide={props.onHide}>
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

      <View class="gap-1 p-2">
        <NavRow label="Cameras" icon="cctv" selected={page().kind === "cameras"} onPress={() => show({ kind: "cameras" })} />
        <NavRow label="Groups" icon="folder" selected={page().kind === "groups"} onPress={() => show({ kind: "groups" })} />
        <NavRow label="Settings" icon="settings" onPress={() => showSettings()} />
      </View>
    </SidebarFrame>
  );
}
