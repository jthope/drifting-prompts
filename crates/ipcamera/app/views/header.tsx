import { Text, View } from "nsh";
import { Cameras } from "ipcamera/native";
import { IconButton, LevelSlider, Logo, look, type Mode, StatusLine, ThemeMenu, type ThemeName, TitleBar, toggleFullScreen } from "../../../dp/ui";
import { takeSnapshots, toggleMute, toggleRecording } from "./actions";
import { LayoutMenu } from "./layout";
import { cameraOf, currentView, focusedId, onScreen, page, pageName, recording, replacesStage } from "./model";

function SnapshotButton() {
  return <IconButton icon="camera" label="Snapshot" onPress={takeSnapshots} />;
}

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

/** What the stage shows and what can be done with it, then what the whole window does; it wraps rather than hides anything. */
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
        {/* Fragments, so every control wraps on its own. */}
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
