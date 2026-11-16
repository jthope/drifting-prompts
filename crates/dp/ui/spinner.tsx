import { createMotion, Text, View } from "nsh";
import { Icon } from "./icon";

/**
 * Something is under way and its end can't be told (a search of the network):
 * a ring that turns, with what is under way (`label`) beside it, which is also
 * what a screen reader says. The turning is nsh's own `spin`, so it runs
 * natively and holds still when the system asks for less motion: the ring and
 * the words are then what says it.
 */
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
