// The elevations: black and white at low alphas, so they bring no color of their own.
import { signal } from "nsh";

export type ElevationName = "none" | "soft" | "raised";

/** The elevations, in the order Settings offers them. */
export const ELEVATIONS: { value: ElevationName; title: string; label?: string; description: string }[] = [
  { value: "none", title: "None", label: "No elevation", description: "Flat: colors and lines alone set things apart." },
  { value: "soft", title: "Soft", description: "Soft shadows lift cards, menus and buttons off the page." },
  { value: "raised", title: "Raised", description: "Shadows, with light catching the top edges of what is raised, and fields pressed in." },
];

/** The elevation the app shows. */
export const elevation = signal<ElevationName>("none");

/** `name` as an elevation: the default for an unknown name. */
export function elevationNamed(name: string): ElevationName {
  return ELEVATIONS.find((each) => each.value === name)?.value ?? "none";
}

/** The places an elevation lifts or presses in. */
export type Place = "card" | "popup" | "tooltip" | "primary" | "check" | "segment" | "badge" | "surface" | "control" | "well" | "nav" | "thumb";

const PLACES: Place[] = ["card", "popup", "tooltip", "primary", "check", "segment", "badge", "surface", "control", "well", "nav", "thumb"];

/** What an elevation adds to one place: a light paint, a card edge's light, and shadows inner then outer. */
export type Lift = { light?: string; edge?: string; shadows: string[] };

/** What an elevation adds: each place's lift, and the shade along a scroll view's edges. */
export type Depth = Record<Place, Lift> & { scrollShade: ScrollShade };

/** The side of a scroll view an edge is on. */
export type EdgeSide = "top" | "bottom";

/** The shade along a scroll view's edge on `side`, or `"none"`. */
export type ScrollShade = (side: EdgeSide) => string;

const black = (alpha: number) => `rgb(0 0 0 / ${alpha})`;

/** No shade along either edge. */
export const NO_SCROLL_SHADE: ScrollShade = () => "none";

function scrollShade(level: ElevationName, night: boolean): ScrollShade {
  if (level === "none") return NO_SCROLL_SHADE;
  const [edge, middle] = night ? [0.4, 0.14] : [0.08, 0.03];
  const stops = `${black(edge)}, ${black(middle)} 40%, ${black(0)}`;
  return (side) => `linear-gradient(${side === "top" ? "180deg" : "0deg"}, ${stops})`;
}
const white = (alpha: number) => `rgb(255 255 255 / ${alpha})`;

const fall = (from: string, to = "100%") => `linear-gradient(180deg, ${from}, ${from.replace(/[\d.]+\)$/, "0)")} ${to})`;

/** A primary button's sheen, fading from `alpha` at its top. */
export const primaryLight = (alpha: number) => fall(white(alpha));

/** A selected segment's sheen, from top to bottom. */
export const segmentLight = (top: number, bottom: number) => `linear-gradient(180deg, ${white(top)}, ${white(bottom)})`;

/** The unadjusted sheens, before a theme limits them to keep their text readable. */
export const SHEEN = { primaryBright: 0.5, primaryDark: 0.14, segmentTop: 0.17, segmentBottom: 0.1 };

const DROPS: Record<Place, (night: boolean) => string[]> = {
  card: (night) => (night ? [`0 1px 2px ${black(0.5)}`, `0 10px 28px -14px ${black(0.8)}`] : [`0 1px 2px ${black(0.05)}`, `0 2px 6px -1px ${black(0.05)}`, `0 12px 28px -14px ${black(0.16)}`]),
  popup: (night) => (night ? [`0 2px 4px -1px ${black(0.5)}`, `0 18px 40px -10px ${black(0.75)}`] : [`0 2px 4px -1px ${black(0.06)}`, `0 14px 34px -10px ${black(0.2)}`]),
  tooltip: (night) => [`0 4px 12px -4px ${black(night ? 0.6 : 0.35)}`],
  primary: (night) => (night ? [`0 1px 2px ${black(0.5)}`] : [`0 1px 2px ${black(0.2)}`, `0 3px 8px -3px ${black(0.3)}`]),
  check: () => [],
  segment: (night) => (night ? [`0 1px 2px ${black(0.6)}`] : [`0 1px 2px ${black(0.08)}`, `0 0 0 1px ${black(0.05)}`]),
  badge: (night) => [`0 1px 2px ${black(night ? 0.5 : 0.06)}`],
  surface: (night) => (night ? [`0 1px 2px ${black(0.4)}`] : [`0 1px 2px ${black(0.04)}`, `0 4px 14px -8px ${black(0.1)}`]),
  control: (night) => [`0 1px 2px ${black(night ? 0.4 : 0.05)}`],
  well: () => [],
  nav: (night) => (night ? [`0 1px 2px ${black(0.4)}`] : [`0 1px 2px ${black(0.07)}`, `0 0 0 1px ${black(0.05)}`]),
  thumb: (night) => (night ? [`0 1px 2px ${black(0.5)}`] : [`0 1px 3px ${black(0.1)}`, `0 1px 2px -1px ${black(0.1)}`]),
};

type Lit = { light?: string; edge?: string; inset?: string[] };

const LIGHTS: Record<Place, (bright: boolean) => Lit> = {
  card: (bright) =>
    bright
      ? { light: `linear-gradient(180deg, ${black(0)}, ${black(0.02)})`, edge: `linear-gradient(180deg, ${black(0)} 40%, ${black(0.07)})` }
      : { light: fall(white(0.04), "70%"), edge: fall(white(0.12), "45%") },
  popup: (bright) => (bright ? {} : { light: fall(white(0.035), "40%"), inset: [`inset 0 1px 0 ${white(0.06)}`] }),
  tooltip: (bright) => (bright ? { light: fall(white(0.1)), inset: [`inset 0 1px 0 ${white(0.5)}`] } : { light: fall(white(0.12)), inset: [`inset 0 1px 0 ${white(0.14)}`] }),
  primary: (bright) =>
    bright
      ? { light: primaryLight(SHEEN.primaryBright), inset: [`inset 0 1px 0 ${white(0.7)}`, `inset 0 -1px 0 ${black(0.12)}`] }
      : { light: primaryLight(SHEEN.primaryDark), inset: [`inset 0 1px 0 ${white(0.16)}`] },
  check: (bright) => (bright ? { light: fall(white(0.5)), inset: [`inset 0 1px 0 ${white(0.6)}`] } : { light: fall(white(0.16)), inset: [`inset 0 1px 0 ${white(0.18)}`] }),
  segment: (bright) => (bright ? {} : { light: segmentLight(SHEEN.segmentTop, SHEEN.segmentBottom), inset: [`inset 0 1px 0 ${white(0.1)}`] }),
  badge: (bright) =>
    bright
      ? { light: `linear-gradient(180deg, ${white(0.95)}, ${white(0)})`, inset: [`inset 0 0 0 1px ${black(0.07)}`, `inset 0 -1px 0 ${black(0.05)}`] }
      : { light: `linear-gradient(180deg, ${white(0.1)}, ${white(0.02)})`, inset: [`inset 0 1px 0 ${white(0.1)}`, `inset 0 0 0 1px ${white(0.06)}`] },
  surface: (bright) => (bright ? { light: `linear-gradient(180deg, ${black(0)} 50%, ${black(0.015)})` } : { light: fall(white(0.03), "50%"), inset: [`inset 0 1px 0 ${white(0.04)}`] }),
  control: (bright) => (bright ? {} : { inset: [`inset 0 1px 0 ${white(0.05)}`] }),
  well: (bright) => (bright ? { inset: [`inset 0 1px 2px ${black(0.07)}`] } : { inset: [`inset 0 1px 3px ${black(0.5)}`, `inset 0 -1px 0 ${white(0.04)}`] }),
  nav: (bright) => (bright ? {} : { inset: [`inset 0 1px 0 ${white(0.06)}`] }),
  thumb: (bright) => (bright ? {} : { inset: [`inset 0 1px 0 ${white(0.08)}`] }),
};

/** What `level` adds to every place: `night` decides the shadows, `bright` each place's light. */
export function depth(level: ElevationName, night: boolean, bright: Partial<Record<Place, boolean>> = {}): Depth {
  const lift = (place: Place): Lift => {
    if (level === "none") return { shadows: [] };
    const drops = DROPS[place](night);
    if (level === "soft") return { shadows: drops };
    const lit = LIGHTS[place](bright[place] ?? !night);
    return { light: lit.light, edge: lit.edge, shadows: [...(lit.inset ?? []), ...drops] };
  };
  return { ...Object.fromEntries(PLACES.map((place) => [place, lift(place)])), scrollShade: scrollShade(level, night) } as Depth;
}
