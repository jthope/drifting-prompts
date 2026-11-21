import { Text, View } from "nsh";
import { look } from "./brand";

const tabular = { fontVariant: ["tabular-nums" as const] };

function short(count: number): string {
  return count < 1000 ? String(count) : `${Math.floor(count / 1000)}k`;
}

/** `count` matches, in a pill on the search's color; nothing while there are none. */
export function CountBadge(props: { count: number; class?: string }) {
  return (
    <If when={props.count > 0}>
      <View class={`h-4 min-w-4 items-center justify-center rounded-full px-1.5 ${props.class ?? ""}`} style={{ backgroundColor: look().found.match }}>
        <Text class="text-xs font-semibold text-foreground" style={{ ...tabular, fontSize: 10, lineHeight: 14 }}>
          {short(props.count)}
        </Text>
      </View>
    </If>
  );
}
