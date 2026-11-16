// The panes of a window as islands: rounded blocks of one color, set on the
// window's frame with a gap all round, as the panes of a code editor's newer
// layouts are. Their colors are three steps (`Look.tiers`): the frame behind
// them, which the sidebar shares (`Frame`), a tool pane's (a reader's files,
// its outline, the PTZ panel) and the document's (a reader's file, the video),
// so the panes stand apart in every theme by their color, their gap and their
// corners together, where a line alone barely parted them. An island's outline
// is a card's (`Look.cardEdge`): none in Default, whose islands stand off the
// frame by their color as its cards do, and the theme's border in a theme file.
//
// `Islands` is where they sit: the frame, half a gap in from its edges, so
// islands half a gap in each stand a gap apart and a gap from the frame's
// edges, and a resize handle between two lies in the middle of their gap
// (`GRIP_HANDLE`, which the sidebar's edge takes too).
import { type JSX, View, type ViewStyle } from "nsh";
import { look, solid } from "./brand";

/** The room between two islands, and between an island and the window's frame around it, in points. */
export const ISLAND_GAP = 6;

/**
 * A resize handle with no line of its own (`handleClass` and
 * `handleThumbClass` of a `ResizablePanelGroup`), in the gap between two
 * islands or along the sidebar's edge: a grip shows in its middle while the
 * pointer is over it, fuller while it is dragged.
 */
export const GRIP_HANDLE = {
  line: "h-full w-px bg-transparent",
  thumb: "h-6 w-1 rounded-full bg-muted-foreground opacity-0 group-hover:opacity-50 group-active:opacity-90 transition",
};

/**
 * A resize handle between two parts of a page that scroll together (the events'
 * table and its preview), whose band is as tall as what scrolls and its middle
 * often out of sight: the grip's, with the theme's line along the whole of it
 * while the pointer is over it, so it shows wherever the pointer meets it.
 */
export const LINE_HANDLE = {
  line: "h-full w-px bg-transparent group-hover:bg-border group-active:bg-ring transition",
  thumb: GRIP_HANDLE.thumb,
};

/** Which step of the window's surfaces an island is: a tool pane's or the document's. */
export type IslandKind = "tool" | "document";

/** The window's frame, in its color (`Look.tiers.frame`): the sidebar, and what the islands sit on. `class` and `style` place it. */
export function Frame(props: { class?: string; style?: ViewStyle; children?: JSX.Element }) {
  return (
    <View class={props.class ?? ""} style={{ backgroundImage: solid(look().tiers.frame), ...props.style }}>
      {props.children}
    </View>
  );
}

/**
 * Where islands sit: the frame, filling what it is given, half a gap in from
 * its edges. `flat` leaves the gap out, for an island that fills the window
 * (full screen's video).
 */
export function Islands(props: { flat?: boolean; children?: JSX.Element }) {
  return (
    <Frame class="min-h-0 flex-1" style={{ padding: props.flat ? 0 : ISLAND_GAP / 2 }}>
      {props.children}
    </Frame>
  );
}

/**
 * One island, in `kind`'s color, half a gap in from where it is placed, its
 * corners rounded and its content clipped to them. `class` and `style` size it.
 * `flat` draws it as the whole of its place, with no gap, corners or outline,
 * as `Islands` does with its own `flat`. `edgeless` leaves out the outline
 * alone, for content that fills the island to its rounded edge (IP Camera's
 * video), which would cover it.
 */
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
