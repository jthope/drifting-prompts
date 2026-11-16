// Where something that stays put meets content scrolling past it: a page's
// header over its page, the sidebar's cameras heading over the cameras and its
// pinned rows under them. An edge shows only while content lies beyond it: a
// one-point line along it, in the theme's color for the place it frames, and
// the elevation's shade (`Look.scrollShade`) falling from it, so what stays
// reads as above what passes. Over a page, the shade falls on the content
// itself (a page's header). Where both ends of a list must read alike, the
// content first fades into the surface it scrolls on (`fade`, `fadeFrom`) and
// the shade falls over the fade: it then lies on the same plain color at
// either end, not on whatever row happens to be there. They come in as content
// passes the edge and go as the last of it comes back.
//
// The top and the bottom edge are one thing: the same line, the same shade and
// fade from the same stops, coming in over the same distances, set up with the
// same props. The only difference is which way they face. Each follows how far the
// content reaches past it (`pastTop`, `pastBottom`), an animated node on the UI
// thread, so no JS runs as it scrolls.
//
// `ScrollEdge` is one edge, for a layout of its own, as the collapsing header
// has. `EdgedScroll` is a scroll view with one at each end, which measures
// itself for the bottom one: put it where a plain `Scroll` would go.
import { type AnimatedNode, createAnimatedValue, type JSX, Scroll, signal, View } from "nsh";
import { EdgeLine, type EdgePlace, fadeFrom, look } from "./brand";
import type { EdgeSide } from "./elevation";

/** How far content reaches past an edge before its line is whole, unless the line leads (`lead`). */
const LINE_IN = 8;
/** How deep the shade falls from the edge, in points. */
const SHADE = 12;
/** How far content reaches past an edge before its shade is whole. */
const SHADE_IN = 24;
/** How deep content fades into its surface from the edge, in points, its line included. */
const FADE = 24;
/**
 * How far content reaches past an edge before its fade is whole: a few points,
 * so the edge is solid as soon as anything lies beyond it, and a row the edge
 * cuts never shows through at the line.
 */
const FADE_IN = 4;

/**
 * How far a scroll view's content reaches past its top edge: how far it has
 * scrolled, less `from`, the room above the content that passes under nothing
 * (a page's header while it collapses).
 */
export const pastTop = (scroll: AnimatedNode, from = 0): AnimatedNode => scroll.sub(from);

/** How far a scroll view's content reaches past its bottom edge: how much more it can scroll, `end` being how far it scrolls in all. */
export const pastBottom = (scroll: AnimatedNode, end: number): AnimatedNode => scroll.mul(-1).add(end);

/**
 * One edge of a scroll view, on `side`, laid over the view that holds it,
 * `inset` points in from that side (0 by default). `past` is how far the
 * content reaches past it (`pastTop` or `pastBottom`): at 0 or less the edge
 * is clear; its line is whole once content reaches `LINE_IN` points past, and
 * its shade at `SHADE_IN`. `lead` brings the line in sooner, over that many
 * points before content reaches the edge, as a page's header does while its
 * banner goes. `place` gives the line its color, as `EdgeLine`'s does: a
 * page's unless it says; `line={false}` leaves it out. `fade` is the theme
 * color the content scrolls on (`"sidebar"`, or a color of `Look.tiers`): given, the content fades into it
 * from the edge itself (`fadeFrom`), solid along the edge so nothing shows
 * through the line, and whole once content reaches `FADE_IN` points past; the
 * shade falls over the fade, onto that plain color rather than onto whatever
 * row happens to be there, so it reads the same at either end.
 */
export function ScrollEdge(props: { side: EdgeSide; past: AnimatedNode; inset?: number; lead?: number; place?: EdgePlace; line?: boolean; fade?: string }) {
  const shown = (inputRange: number[]) => props.past.interpolate({ inputRange, outputRange: [0, 1], extrapolate: "clamp" });
  const line = () => props.line ?? true;
  /** Placed along the edge: its top for a top edge, its bottom for a bottom one. */
  const along = (at: number) => (props.side === "top" ? { top: at } : { bottom: at });
  /** A strip `depth` deep running from `at` points off the edge. */
  const strip = (depth: number, at: number) => ({ position: "absolute" as const, left: 0, right: 0, height: depth, ...along(at) });
  return (
    <View
      style={{ position: "absolute", left: 0, right: 0, height: props.fade === undefined ? 1 + SHADE : FADE, pointerEvents: "none", ...along(props.inset ?? 0) }}
    >
      {/* Under the line, from the edge itself: the line then lies on the surface's own color, never on a row. */}
      <If when={props.fade}>
        {(surface) => <View style={{ ...strip(FADE, 0), backgroundImage: fadeFrom(surface(), props.side), opacity: shown([0, FADE_IN]) }} />}
      </If>
      {/* From the line, or from the edge itself when there is none. */}
      <View style={{ ...strip(SHADE, line() ? 1 : 0), backgroundImage: look().scrollShade(props.side), opacity: shown([0, SHADE_IN]) }} />
      <If when={line()}>
        <View style={{ ...strip(1, 0), opacity: shown(props.lead ? [-props.lead, 0] : [0, LINE_IN]) }}>
          <EdgeLine edge={props.side} place={props.place} />
        </View>
      </If>
    </View>
  );
}

/**
 * A scroll view with a `ScrollEdge` at each end, alike but for their
 * direction: at its top while content lies above, at its bottom while more
 * lies below. It measures itself and its content for the bottom one, so it
 * goes where a plain `Scroll` would, and takes the room its parent gives it.
 * `contentClass` is the content's (its padding and gaps); `place`, `line` and
 * `fade` are both edges', as `ScrollEdge` takes them.
 */
export function EdgedScroll(props: { testID?: string; place?: EdgePlace; line?: boolean; fade?: string; contentClass?: string; children: JSX.Element }) {
  const scroll = createAnimatedValue(0);
  const viewport = signal(0);
  const content = signal(0);
  /** How far it scrolls in all; 0 while the content fits, which leaves the bottom edge clear. */
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
