import { View } from "nsh";
import { BrandCard } from "./brand";
import { IconButton } from "./button";
import type { IconName } from "./icon";

/** A button of the pad: its icon, what it does, and which way it moves, `1` right or up, `-1` left or down. */
type PadButton = { icon: IconName; label: string; pan: number; tilt: number };

const ROWS: (PadButton | "stop")[][] = [
  [
    { icon: "arrow-up-left", label: "Up and left", pan: -1, tilt: 1 },
    { icon: "arrow-up", label: "Up", pan: 0, tilt: 1 },
    { icon: "arrow-up-right", label: "Up and right", pan: 1, tilt: 1 },
  ],
  [{ icon: "arrow-left", label: "Left", pan: -1, tilt: 0 }, "stop", { icon: "arrow-right", label: "Right", pan: 1, tilt: 0 }],
  [
    { icon: "arrow-down-left", label: "Down and left", pan: -1, tilt: -1 },
    { icon: "arrow-down", label: "Down", pan: 0, tilt: -1 },
    { icon: "arrow-down-right", label: "Down and right", pan: 1, tilt: -1 },
  ],
];

/** Eight directions around Stop: holding one calls `onMove` with its pan and tilt, letting go `onStop`. */
export function DirectionPad(props: { onMove: (pan: number, tilt: number) => void; onStop: () => void }) {
  return (
    <BrandCard radius={14} class="items-center gap-1 p-1.5">
      <Each of={ROWS}>
        {(row) => (
          <View class="flex-row gap-1">
            <Each of={row()}>
              {(button) => {
                const shown = button();
                return shown === "stop" ? (
                  <IconButton icon="square" label="Stop" kind="primary" size="large" onPress={props.onStop} />
                ) : (
                  <IconButton
                    icon={shown.icon}
                    label={shown.label}
                    size="large"
                    onPressDown={() => props.onMove(shown.pan, shown.tilt)}
                    onPressUp={props.onStop}
                  />
                );
              }}
            </Each>
          </View>
        )}
      </Each>
    </BrandCard>
  );
}
