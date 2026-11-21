// Edges over scrolling content, driven by animated nodes so no JS runs as it scrolls.
import { type AnimatedNode, createAnimatedValue, type JSX, Scroll, signal, View } from "nsh";
import { EdgeLine, type EdgePlace, fadeFrom, look } from "./brand";
import type { EdgeSide } from "./elevation";

const LINE_IN = 8;
const SHADE = 12;
const SHADE_IN = 24;
const FADE = 24;
/** Small, so a row the edge cuts never shows through at the line. */
const FADE_IN = 4;

/** How far content reaches past a scroll view's top edge; `from` is room above that passes under nothing. */
export const pastTop = (scroll: AnimatedNode, from = 0): AnimatedNode => scroll.sub(from);

/** How far content reaches past a scroll view's bottom edge; `end` is how far it scrolls in all. */
export const pastBottom = (scroll: AnimatedNode, end: number): AnimatedNode => scroll.mul(-1).add(end);

/** One edge of a scroll view on `side`: a line and a shade, and with `fade` the content fading into that color. */
export function ScrollEdge(props: { side: EdgeSide; past: AnimatedNode; inset?: number; lead?: number; place?: EdgePlace; line?: boolean; fade?: string }) {
  const shown = (inputRange: number[]) => props.past.interpolate({ inputRange, outputRange: [0, 1], extrapolate: "clamp" });
  const line = () => props.line ?? true;
  const along = (at: number) => (props.side === "top" ? { top: at } : { bottom: at });
  const strip = (depth: number, at: number) => ({ position: "absolute" as const, left: 0, right: 0, height: depth, ...along(at) });
  return (
    <View
      style={{ position: "absolute", left: 0, right: 0, height: props.fade === undefined ? 1 + SHADE : FADE, pointerEvents: "none", ...along(props.inset ?? 0) }}
    >
      <If when={props.fade}>
        {(surface) => <View style={{ ...strip(FADE, 0), backgroundImage: fadeFrom(surface(), props.side), opacity: shown([0, FADE_IN]) }} />}
      </If>
      <View style={{ ...strip(SHADE, line() ? 1 : 0), backgroundImage: look().scrollShade(props.side), opacity: shown([0, SHADE_IN]) }} />
      <If when={line()}>
        <View style={{ ...strip(1, 0), opacity: shown(props.lead ? [-props.lead, 0] : [0, LINE_IN]) }}>
          <EdgeLine edge={props.side} place={props.place} />
        </View>
      </If>
    </View>
  );
}

/** A scroll view with a `ScrollEdge` at each end, measuring itself for the bottom one. */
export function EdgedScroll(props: { testID?: string; place?: EdgePlace; line?: boolean; fade?: string; contentClass?: string; children: JSX.Element }) {
  const scroll = createAnimatedValue(0);
  const viewport = signal(0);
  const content = signal(0);
  const end = () => Math.max(0, content() - viewport());
  return (
    <View class="flex-1" onLayout={(event) => viewport.set(event.bounds.size.height)}>
      <Scroll testID={props.testID} scrollValue={scroll} style={{ flex: 1 }}>
        <View class={props.contentClass} onLayout={(event) => content.set(event.bounds.size.height)}>
          {props.children}
        </View>
      </Scroll>
      <ScrollEdge side="top" past={pastTop(scroll)} place={props.place} line={props.line} fade={props.fade} />
      <ScrollEdge side="bottom" past={pastBottom(scroll, end())} place={props.place} line={props.line} fade={props.fade} />
    </View>
  );
}
