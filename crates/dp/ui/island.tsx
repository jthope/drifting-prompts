import { type JSX, View, type ViewStyle } from "nsh";
import { look, solid } from "./brand";

/** The room between two islands, and around them, in points. */
export const ISLAND_GAP = 6;

/** A resize handle with no line of its own, for the gap between islands: a grip shows on hover. */
export const GRIP_HANDLE = {
  line: "h-full w-px bg-transparent",
  thumb: "h-6 w-1 rounded-full bg-muted-foreground opacity-0 group-hover:opacity-50 group-active:opacity-90 transition",
};

/** A resize handle whose middle is often out of sight, so the theme's line shows along all of it on hover. */
export const LINE_HANDLE = {
  line: "h-full w-px bg-transparent group-hover:bg-border group-active:bg-ring transition",
  thumb: GRIP_HANDLE.thumb,
};

/** Which step of the window's surfaces an island is: a tool pane's or the document's. */
export type IslandKind = "tool" | "document";

/** The window's frame, in its color: the sidebar, and what the islands sit on. */
export function Frame(props: { class?: string; style?: ViewStyle; children?: JSX.Element }) {
  return (
    <View class={props.class ?? ""} style={{ backgroundImage: solid(look().tiers.frame), ...props.style }}>
      {props.children}
    </View>
  );
}

/** Where islands sit, half a gap in from the frame's edges; `flat` leaves the gap out. */
export function Islands(props: { flat?: boolean; children?: JSX.Element }) {
  return (
    <Frame class="min-h-0 flex-1" style={{ padding: props.flat ? 0 : ISLAND_GAP / 2 }}>
      {props.children}
    </Frame>
  );
}

/** One island in `kind`'s color; `flat` fills its place, and `edgeless` drops the outline content would cover. */
export function Island(props: { kind: IslandKind; flat?: boolean; edgeless?: boolean; class?: string; style?: ViewStyle; children?: JSX.Element }) {
  const outline = () => (props.edgeless ? "" : `border ${look().cardEdge}`);
  return (
    <View
      class={`overflow-hidden ${props.flat ? "" : `rounded-lg ${outline()}`} ${props.class ?? ""}`}
      style={{ backgroundImage: solid(look().tiers[props.kind]), margin: props.flat ? 0 : ISLAND_GAP / 2, ...props.style }}
    >
      {props.children}
    </View>
  );
}
