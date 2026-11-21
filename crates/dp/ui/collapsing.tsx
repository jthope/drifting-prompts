// A page's header that collapses as the page scrolls, all of it native: no JS runs meanwhile.
import { type AnimatedNode, type AnimatedValue, Canvas, type JSX, Text, View } from "nsh";
import { BrandBadge, look } from "./brand";
import { cardClass, SHADOW_ROOM } from "./card";
import { pastTop, ScrollEdge } from "./edge";
import type { IconName } from "./icon";
import { headerTexture } from "./texture";

const ABOVE = 16;
const BELOW = SHADOW_ROOM;
const EDGE = 1;
/** The banner's bar: all that stays once the header collapses. */
export const HEADER_BAR = 48;
const MASTHEAD = 104;
const OPEN = ABOVE + EDGE + MASTHEAD + HEADER_BAR + EDGE + BELOW;
/** How far a page scrolls while its header collapses, at its usual height. */
export const HEADER_COLLAPSE = OPEN - HEADER_BAR;
/** As a page's content has (`px-6`). */
const GUTTER = 24;

/** 0 while the masthead shows, rising to 1 as its last part leaves. */
export function collapsedness(scroll: AnimatedValue, open = OPEN): AnimatedNode {
  const range = open - HEADER_BAR;
  return scroll.interpolate({ inputRange: [range * 0.7, range], outputRange: [0, 1], extrapolate: "clamp" });
}

export function CollapsingHeader(props: {
  scroll: AnimatedValue;
  icon: IconName;
  /** The badge's color: one of the first four chart colors, 0 for `chart-1`. */
  chart: number;
  title: string;
  subtitle: string;
  details?: JSX.Element;
  aside?: JSX.Element;
  /** The header's height at the top of the page, for a masthead that needs more. */
  open?: number;
  children?: JSX.Element;
}) {
  const open = () => props.open ?? OPEN;
  const range = () => open() - HEADER_BAR;
  const banner = () => open() - ABOVE - BELOW;
  const masthead = () => banner() - 2 * EDGE - HEADER_BAR;
  const along = (to: number) => props.scroll.interpolate({ inputRange: [0, range()], outputRange: [0, to], extrapolate: "clamp" });
  const card = () => props.scroll.interpolate({ inputRange: [range() * 0.6, range()], outputRange: [1, 0], extrapolate: "clamp" });
  return (
    // Opaque, as the page scrolls on under it; unclipped, as the shade hangs below it.
    <View class="bg-background" style={{ position: "sticky", top: -range(), height: open() }}>
      <View style={{ position: "absolute", left: GUTTER, right: GUTTER, top: ABOVE, height: banner(), translate: [0, along(BELOW + EDGE)] }}>
        <View class={cardClass()} style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, ...look().surface, opacity: card() }} />
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
      <ScrollEdge side="top" past={pastTop(props.scroll, range())} inset={open() - 1} lead={range() * 0.3} />
    </View>
  );
}
