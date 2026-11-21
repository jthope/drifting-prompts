import { createMotion, Text, View } from "nsh";
import { Icon } from "./icon";

/** A ring that turns beside `label`, for work whose end can't be told; still when the system asks for less motion. */
export function Spinner(props: { label: string; size?: number }) {
  const turning = createMotion({ enter: "spin" });
  return (
    <View role="progressbar" accessibilityLabel={props.label} class="flex-row items-center gap-2">
      <View style={turning.style}>
        <Icon name="loader-circle" size={props.size ?? 16} tint="muted-foreground" />
      </View>
      <Text class="text-sm text-muted-foreground">{props.label}</Text>
    </View>
  );
}
