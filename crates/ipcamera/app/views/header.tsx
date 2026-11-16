import { Text, View } from "nsh";
import { Cameras } from "ipcamera/native";
import { IconButton, LevelSlider, Logo, look, type Mode, StatusLine, ThemeMenu, type ThemeName, TitleBar, toggleFullScreen } from "../../../dp/ui";
import { takeSnapshots, toggleMute, toggleRecording } from "./actions";
import { LayoutMenu } from "./layout";
import { cameraOf, currentView, focusedId, onScreen, page, pageName, recording, replacesStage } from "./model";

/** Saves the picture of every connected camera on screen, as Record records them. */
function SnapshotButton() {
  return <IconButton icon="camera" label="Snapshot" onPress={takeSnapshots} />;
}

/**
 * Records every connected camera on screen, and stops every recording. While
 * anything records, wherever the user has gone since, it turns red and shows
 * the stop square instead of the record circle.
 */
function RecordButton() {
  return (
    <IconButton
      icon={recording() ? "square" : "circle"}
      label={recording() ? "Stop recording" : "Record"}
      on={recording()}
      tint={recording() ? "destructive" : undefined}
      onPress={toggleRecording}
    />
  );
}

/**
 * What the stage shows, and what can be done with it: a view's layout (each
 * tile's fit is on its own toolbar), then the selected camera's status and
 * actions, whose Connect and Disconnect are on its picture's toolbar
 * instead. Snapshot and Record take
 * every camera on screen, so they stay while an empty tile is selected. The
 * snapshots and the videos keep their count and filters in their own header.
 * Last, what the whole window does: the theme menu, with the mode
 * (`appearance`, `onAppearance`, as Settings picks it) over the themes of the
 * appearance it shows (`onTheme`, as Settings picks one), and full screen. It
 * wraps rather than hiding anything, so every action is still there in a
 * narrow window.
 */
export function Header(props: {
  sidebarOpen: boolean;
  ptzOpen: boolean;
  appearance: Mode;
  onShowSidebar: () => void;
  onTogglePtz: () => void;
  onAppearance: (mode: Mode) => void;
  onTheme: (name: ThemeName) => void;
}) {
  const camera = () => cameraOf(focusedId());
  /** One of the app's own pages (the dashboard, the tables, the galleries, the events), in place of the live video. */
  const library = () => replacesStage(page());
  return (
    <TitleBar class="pl-3">
      <If when={!props.sidebarOpen}>
        <View class="flex-row items-center gap-2">
          <Logo product="ipcamera" />
          <IconButton icon="panel-left" label="Show sidebar" onPress={props.onShowSidebar} />
        </View>
      </If>

      <View class="min-w-40 flex-1 flex-row items-center gap-3">
        <Text class={`text-sm font-semibold ${look().titleText ?? "text-foreground"}`} style={{ numberOfLines: 1 }}>
          {pageName()}
        </Text>
        <If when={currentView() && camera()}>
          <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1 }}>
            {camera()?.name}
          </Text>
        </If>
        <If when={camera()}>{(shown) => <StatusLine tone={shown().tone} text={shown().status} class="flex-1" />}</If>
      </View>

      <View class="max-w-full flex-row flex-wrap items-center gap-1">
        {/* Fragments, so every control wraps on its own in a narrow window. */}
        <If when={currentView()}>{(view) => <LayoutMenu view={view()} class="mr-2" />}</If>
        <If when={camera()}>
          {(shown) => (
            <>
              <SnapshotButton />
              <RecordButton />
              <IconButton
                icon={shown().muted ? "volume-x" : "volume-2"}
                label={shown().muted ? "Unmute" : "Mute"}
                tint={shown().muted ? "destructive" : undefined}
                onPress={() => toggleMute(shown())}
              />
              <LevelSlider
                label="Volume"
                value={shown().volume}
                tone={shown().volume > 0 ? "info" : "primary"}
                class="w-24"
                onChange={(volume) => Cameras.setAudio(shown().id, shown().muted, volume)}
              />
            </>
          )}
        </If>
        {/* Without a selected camera, snapshots and recording are still in reach: to take or
            start them for the rest of the screen, or to stop recording from the videos. */}
        <If when={!camera() && !library() && onScreen().length > 0}>
          <SnapshotButton />
        </If>
        <If when={!camera() && (recording() || (!library() && onScreen().length > 0))}>
          <RecordButton />
        </If>
        <If when={!library()}>
          <IconButton icon="move" label={props.ptzOpen ? "Hide PTZ" : "Show PTZ"} on={props.ptzOpen} class="ml-1" onPress={props.onTogglePtz} />
        </If>
        <ThemeMenu mode={props.appearance} onMode={props.onAppearance} onPick={props.onTheme} class="ml-1" />
        <IconButton icon="maximize" label="Fullscreen" onPress={toggleFullScreen} />
      </View>
    </TitleBar>
  );
}
