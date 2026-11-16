// How many matches a search found in what a row or a tab shows (a file, a
// heading), in a small pill beside its name, on the search's own color
// (`look().found`), so it reads as the matches it counts.
import { Text, View } from "nsh";
import { look } from "./brand";

const tabular = { fontVariant: ["tabular-nums" as const] };

/** A count of 1,000 or more, said shortly: `1k`, `12k`. */
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
