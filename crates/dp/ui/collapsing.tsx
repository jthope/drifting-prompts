// A page's header that collapses as the page scrolls, after nsh's gallery's
// "Collapsing header": one banner holding a masthead (the theme's badge, a big
// title, a line under it and any `details` under that, and anything `aside`
// at its end) over a bar that stays (`children`: a page's tabs, its filters).
// The badge is in the chart color of what the page holds (`CHART` in
// `views/model.ts`), as its tile on the dashboard.
//
// Open, the banner is lifted off the page as a hero card is: the app's card
// (`card.tsx`), as the dashboard's tiles are, set in from the page's edges,
// with the texture inside it and room under it for its shadow. The header
// around it is the page's own color in every theme, so the banner stands on
// the page.
//
// Make it the first child of the page's `Scroll` and pass that scroll's
// `scrollValue`; a `List` under it takes `viewport="parent"`, and its sticky
// headings `stickyHeaderOffset={HEADER_BAR}`. The header sticks all but its
// last `HEADER_BAR` points above the top of the scroll view. As the page
// scrolls, the masthead fades and slides up under the title bar, the card
// fades away, and the bar settles flush with the header's bottom, flat on the
// page's color with the theme's page line under it (Default draws none), while
// the page scrolls on under it, in
// the elevation's shade (`Look.scrollShade`) where it passes beneath: the
// line and the shade are a `ScrollEdge` (`edge.tsx`), as anything that stays
// over a scroll can have. Everything moves natively: no JS runs as the page
// scrolls.
import { type AnimatedNode, type AnimatedValue, Canvas, type JSX, Text, View } from "nsh";
import { BrandBadge, look } from "./brand";
import { cardClass, SHADOW_ROOM } from "./card";
import { pastTop, ScrollEdge } from "./edge";
import type { IconName } from "./icon";
import { headerTexture } from "./texture";

/** The page showing above the banner, and under it: room for its shadow. */
const ABOVE = 16;
const BELOW = SHADOW_ROOM;
/** The banner's one-point edge. */
const EDGE = 1;
/** The banner's bar: the row of tabs or filters along its bottom, and all that stays once the header collapses. */
export const HEADER_BAR = 48;
/** The masthead's room in the banner, above its bar, unless a page asks for more. */
const MASTHEAD = 104;
/** The header's height at the top of the page: the banner and the page around it. */
const OPEN = ABOVE + EDGE + MASTHEAD + HEADER_BAR + EDGE + BELOW;
/** How far a page scrolls while its header collapses, at its usual height: past it, the page's content starts right under the bar. */
export const HEADER_COLLAPSE = OPEN - HEADER_BAR;
/** Either side of the banner, as a page's content has (`px-6`). */
const GUTTER = 24;

/**
 * 0 while the masthead shows, rising to 1 as its last part leaves: for what
 * the bar shows only once the masthead is gone (a count it said). `open` is
 * the header's, if the page gave it one.
 */
export function collapsedness(scroll: AnimatedValue, open = OPEN): AnimatedNode {
  const range = open - HEADER_BAR;
  return scroll.interpolate({ inputRange: [range * 0.7, range], outputRange: [0, 1], extrapolate: "clamp" });
}

export function CollapsingHeader(props: {
  scroll: AnimatedValue;
  icon: IconName;
  /** The badge's color: one of the theme's first four chart colors, 0 for `chart-1`, the color of what the page holds. */
  chart: number;
  title: string;
  subtitle: string;
  /** Under the subtitle: what is recording, say. */
  details?: JSX.Element;
  /** Beside the title, at the masthead's end: a page's figures, say. */
  aside?: JSX.Element;
  /** The header's height at the top of the page: 182 points unless the masthead needs more. */
  open?: number;
  children?: JSX.Element;
}) {
  const open = () => props.open ?? OPEN;
  /** How far the page scrolls while the header collapses. */
  const range = () => open() - HEADER_BAR;
  /** The banner's height, its edge included, and the masthead's room in it. */
  const banner = () => open() - ABOVE - BELOW;
  const masthead = () => banner() - 2 * EDGE - HEADER_BAR;
  const along = (to: number) => props.scroll.interpolate({ inputRange: [0, range()], outputRange: [0, to], extrapolate: "clamp" });
  /** The card's presence: whole until the masthead has gone, then fading out as the bar reaches the top. */
  const card = () => props.scroll.interpolate({ inputRange: [range() * 0.6, range()], outputRange: [1, 0], extrapolate: "clamp" });
  return (
    // The page's own color in every theme, so the banner stands on the page, and opaque, since the page scrolls on
    // under it. It doesn't clip: the texture clips itself to the banner's corners, and the shade hangs below it.
    <View class="bg-background" style={{ position: "sticky", top: -range(), height: open() }}>
      {/* The banner, sinking by the room under it as the header collapses, so its bar ends flush with the header. */}
      <View style={{ position: "absolute", left: GUTTER, right: GUTTER, top: ABOVE, height: banner(), translate: [0, along(BELOW + EDGE)] }}>
        <View class={cardClass()} style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, ...look().surface, opacity: card() }} />
        {/* The texture's light, and its dots if it has them, inside the card's corners, going with it. */}
        <If when={headerTexture(banner() - 2 * EDGE)}>
          {(finish) => (
            <View
              class="overflow-hidden rounded-xl"
              style={{ position: "absolute", top: EDGE, left: EDGE, right: EDGE, bottom: EDGE, pointerEvents: "none", opacity: card() }}
            >
              <View style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, backgroundImage: finish().light }}>
                <If when={finish().dots}>{(dots) => <Canvas class="flex-1" draw={dots()} />}</If>
              </View>
            </View>
          )}
        </If>
        <View style={{ position: "absolute", top: EDGE, left: EDGE, right: EDGE, bottom: EDGE }}>
          <View
            class="flex-row items-center gap-4 px-5"
            style={{
              height: masthead(),
              // Gone before the bar reaches the top, and drifting down a little as it goes, so it seems to sink under the bar.
              opacity: props.scroll.interpolate({ inputRange: [0, range() * 0.6], outputRange: [1, 0], extrapolate: "clamp" }),
              translate: [0, along(range() / 4)],
            }}
          >
            <BrandBadge icon={props.icon} chart={props.chart} size={52} />
            <View class="min-w-0 flex-1 gap-1">
              <Text role="header" class="text-3xl font-bold text-card-foreground" style={{ letterSpacing: -0.6, numberOfLines: 1 }}>
                {props.title}
              </Text>
              <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1 }}>
                {props.subtitle}
              </Text>
              {props.details}
            </View>
            {props.aside}
          </View>
          <View class="flex-row items-stretch pl-5 pr-3" style={{ height: HEADER_BAR }}>
            {props.children}
          </View>
        </View>
      </View>
      {/* On the header's last point: content passes under it once the header has collapsed, and its line leads, coming in as the card goes. */}
      <ScrollEdge side="top" past={pastTop(props.scroll, range())} inset={open() - 1} lead={range() * 0.3} />
    </View>
  );
}
