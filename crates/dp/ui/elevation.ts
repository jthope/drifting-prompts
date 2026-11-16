// The app's elevations: how far what stands out lifts off the page (Settings >
// Appearance > Elevation, saved with the other preferences), whatever the
// theme. A theme decides the colors and outlines; the elevation adds depth
// over them, as a texture adds a finish over the page headers. Its shadows are
// black, and its lights white or black, at low alphas, so it sits on any
// theme's colors without bringing a color of its own.
//
// - `none` is flat: colors and lines alone set things apart.
// - `soft` casts a shadow under what is raised: cards, menus, tooltips,
//   buttons, the chosen segment, a selected sidebar row, a slider's thumb, and
//   a page's header, on the page once it scrolls under it.
// - `raised` adds light: the top edge of what is raised catches it, a primary
//   button and a check have a sheen, a badge is beveled like a key, and what
//   takes input (a text field, a track) or is on is pressed in. The theme limits
//   a button or selected segment's sheen where it would obscure its text.
//
// The high contrast themes stay flat at every elevation (`brand.tsx`), as
// their outlines mark every state.
import { signal } from "nsh";

export type ElevationName = "none" | "soft" | "raised";

/** The elevations, in the order Settings offers them. */
export const ELEVATIONS: { value: ElevationName; title: string; label?: string; description: string }[] = [
  { value: "none", title: "None", label: "No elevation", description: "Flat: colors and lines alone set things apart." },
  { value: "soft", title: "Soft", description: "Soft shadows lift cards, menus and buttons off the page." },
  { value: "raised", title: "Raised", description: "Shadows, with light catching the top edges of what is raised, and fields pressed in." },
];

/** The elevation the app shows. The app sets it from the saved preferences before its first frame. */
export const elevation = signal<ElevationName>("none");

/** `name` as an elevation: the default for a name this version doesn't know, as a hand-edited `settings.json` can have. */
export function elevationNamed(name: string): ElevationName {
  return ELEVATIONS.find((each) => each.value === name)?.value ?? "none";
}

/**
 * The places an elevation lifts or presses in: a card, what floats (a menu, a
 * toast, a dialog's panel), a tooltip, a primary button, a check, the chosen
 * segment of a segmented control, a badge, a plain card of settings, a raised
 * control with an outline, what is set in (a text field, a track, a button
 * that is on), a selected sidebar row, and a slider's thumb.
 */
export type Place = "card" | "popup" | "tooltip" | "primary" | "check" | "segment" | "badge" | "surface" | "control" | "well" | "nav" | "thumb";

const PLACES: Place[] = ["card", "popup", "tooltip", "primary", "check", "segment", "badge", "surface", "control", "well", "nav", "thumb"];

/**
 * What an elevation adds to one place: a light over its own color (a paint for
 * `backgroundImage`, drawn over the theme's), the light along a card's
 * one-point edge, and its shadows, inner then outer, as `boxShadow` takes them.
 */
export type Lift = { light?: string; edge?: string; shadows: string[] };

/**
 * What an elevation adds: each place's lift, and the shade along the edges of
 * a scroll view where content passes beyond them (`scrollShade`), as under a
 * page's header.
 */
export type Depth = Record<Place, Lift> & { scrollShade: ScrollShade };

/** The side of a scroll view an edge is on, which its shade falls away from. */
export type EdgeSide = "top" | "bottom";

/**
 * The shade along an edge of a scroll view on `side`: a paint for a strip
 * there, falling away from the edge, or `"none"`. Both sides are made from the
 * same stops, so the two edges of one list look alike but for their direction.
 */
export type ScrollShade = (side: EdgeSide) => string;

const black = (alpha: number) => `rgb(0 0 0 / ${alpha})`;

/** No shade along either edge. */
export const NO_SCROLL_SHADE: ScrollShade = () => "none";

/**
 * The shade along the edge of anything that stays over a scroll, once content
 * passes beyond it, at `level`, on a dark page (`night`) or a light one:
 * darkest along the edge and gone by the end of its strip, more on a dark
 * page, where a shade shows less. Down from a top edge, up from a bottom one.
 * None at `none`: colors and lines alone set things apart there.
 */
function scrollShade(level: ElevationName, night: boolean): ScrollShade {
  if (level === "none") return NO_SCROLL_SHADE;
  const [edge, middle] = night ? [0.4, 0.14] : [0.08, 0.03];
  const stops = `${black(edge)}, ${black(middle)} 40%, ${black(0)}`;
  return (side) => `linear-gradient(${side === "top" ? "180deg" : "0deg"}, ${stops})`;
}
const white = (alpha: number) => `rgb(255 255 255 / ${alpha})`;

/** A light falling from the top of a surface, fading to nothing by `to`. */
const fall = (from: string, to = "100%") => `linear-gradient(180deg, ${from}, ${from.replace(/[\d.]+\)$/, "0)")} ${to})`;

/** A primary button's sheen, fading from `alpha` at its top. */
export const primaryLight = (alpha: number) => fall(white(alpha));

/** A selected segment's sheen, from top to bottom. */
export const segmentLight = (top: number, bottom: number) => `linear-gradient(180deg, ${white(top)}, ${white(bottom)})`;

/** The unadjusted sheens, before a theme limits them to keep their text readable. */
export const SHEEN = { primaryBright: 0.5, primaryDark: 0.14, segmentTop: 0.17, segmentBottom: 0.1 };

/** Each place's shadows under it, on a dark page and on a light one: a light page shows a shadow at a lower alpha. */
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

/**
 * Each place's light and inner shadows, on a light color of its own and on a
 * dark one: a light color needs more white to show a sheen, and on a light
 * card the bottom falls into shade instead of the top catching the light.
 */
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
  // A key: lit from above and edged in the dark, shaded at the bottom in the light.
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

/**
 * What `level` adds to every place. `night` is whether the page is dark, which
 * decides the shadows; `bright` whether each place's own color is light, which
 * decides its light (the page's opposite for a place it leaves out).
 */
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
