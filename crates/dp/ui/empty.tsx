import { type JSX, Text, View } from "nsh";
import { BrandBadge, look } from "./brand";
import type { IconName } from "./icon";

/** Wide enough for the outermost of the theme's rings (`Look.halo`). */
const BADGE = 56;
const HALO = 256;

/** What a page says before it has anything to show: an icon, a title, `text` and an `action`. */
export function EmptyState(props: { icon: IconName; title: string; text: string; action?: JSX.Element }) {
  return (
    <View class="flex-1 items-center justify-center gap-4 p-6">
      <View style={{ width: BADGE, height: BADGE }}>
        <View
          style={{
            position: "absolute",
            left: (BADGE - HALO) / 2,
            top: (BADGE - HALO) / 2,
            width: HALO,
            height: HALO,
            backgroundImage: look().halo,
            pointerEvents: "none",
          }}
        />
        <BrandBadge icon={props.icon} size={BADGE} />
      </View>
      <View class="max-w-96 items-center gap-1">
        <Text role="header" class="text-base font-semibold text-foreground">
          {props.title}
        </Text>
        <Text class="text-center text-sm text-muted-foreground">{props.text}</Text>
      </View>
      {props.action}
    </View>
  );
}
