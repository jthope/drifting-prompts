import { signal, Text, View } from "nsh";
import { Cameras } from "ipcamera/native";
import { DirectionPad, IconButton, LevelSlider, PanelHeader, SectionLabel } from "../../../dp/ui";
import { failed } from "./actions";

/**
 * The pan, tilt and zoom controls for the selected camera, beside the video,
 * on the tool pane's island `App.tsx` sets it on. Holding a direction or a
 * zoom keeps moving until it is released. With no camera selected (`camera` is
 * `""`) the buttons do nothing.
 */
export function PtzPanel(props: { camera: string; status: string; onHide: () => void }) {
  const speed = signal(0.5);

  // Not awaited: on the web a move resolves only when it is let go. A move the camera refuses says
  // why in the status below; these reject only for a camera that is gone.
  const move = (pan: number, tilt: number, zoom: number) => {
    if (props.camera) Cameras.ptzMove(props.camera, pan, tilt, zoom, speed()).catch(failed);
  };
  const stop = () => {
    if (props.camera) Cameras.ptzStop(props.camera).catch(failed);
  };

  return (
    <View class="flex-1">
      <PanelHeader title="PTZ" icon="move" island action={<IconButton icon="chevron-right" label="Collapse PTZ panel" onPress={props.onHide} />} />

      <View class="gap-1 px-3 pb-3 pt-3">
        <View class="items-center">
          <DirectionPad onMove={(pan, tilt) => move(pan, tilt, 0)} onStop={stop} />
        </View>

        <SectionLabel text="Zoom" />
        <View class="flex-row justify-center gap-1">
          <IconButton icon="zoom-in" label="Zoom in" kind="secondary" size="large" onPressDown={() => move(0, 0, 1)} onPressUp={stop} />
          <IconButton icon="zoom-out" label="Zoom out" kind="secondary" size="large" onPressDown={() => move(0, 0, -1)} onPressUp={stop} />
        </View>

        <SectionLabel text="Speed" />
        <LevelSlider
          label="PTZ speed"
          value={speed()}
          min={0.1}
          max={1}
          step={0.05}
          format={(value) => value.toFixed(2)}
          class="px-2"
          onChange={(value) => speed.set(value)}
        />

        <Text class="px-2 pt-3 text-xs text-muted-foreground">{props.status}</Text>
      </View>
    </View>
  );
}
