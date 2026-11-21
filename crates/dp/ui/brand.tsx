// The themes. A component asks `look()` how to draw and never checks which theme shows.
import { colorScheme, computed, type JSX, type Signal, signal, View, type ViewStyle } from "nsh";
import { type Depth, depth, type EdgeSide, type ElevationName, elevation, type Lift, NO_SCROLL_SHADE, primaryLight, type ScrollShade, segmentLight, SHEEN } from "./elevation";
import { Icon, type IconName, type IconTint } from "./icon";

/** A theme color by its token's name: `#RRGGBB`, or `#RRGGBBAA` when it isn't opaque. */
export type NamedColor = { name: string; value: string };

/** The `Themes` module `dp-ui` gives each product's native module. */
export type ThemesModule = {
  /** Draws the app with theme file `name` over its own theme, or with its own again when it is `null`. */
  apply(name: string | null): Promise<void>;
  /** Every color of theme file `name` over the app's own theme (`null`: the app's own), dark or light. */
  colors(name: string | null, dark: boolean): NamedColor[];
};

let themesModule: ThemesModule | undefined;

/** Hands these controls the app's `Themes` module, since each product's native module has a name of its own. */
export function connectThemes(module: ThemesModule) {
  themesModule = module;
}

function themes(): ThemesModule {
  if (!themesModule) throw new Error("Call connectThemes(Themes) before the app shows a theme.");
  return themesModule;
}

/** `default-dark`, `default-light`, or a theme file's name in `themes/`. */
export type ThemeName = string;

/** The appearance a theme is drawn in. */
export type Scheme = "light" | "dark";

export type ThemeInfo = {
  value: ThemeName;
  title: string;
  scheme: Scheme;
  /** A theme file in `themes/`, not the app's own colors. */
  file: boolean;
  /** High contrast: outlines every state and takes no elevation. */
  contrast: boolean;
};

/** Default's name in `scheme`. */
export const defaultTheme = (scheme: Scheme): ThemeName => `default-${scheme}`;

const own = (scheme: Scheme): ThemeInfo => ({ value: defaultTheme(scheme), title: "Default", scheme, file: false, contrast: false });
const darkFile = (value: string, title: string): ThemeInfo => ({ value, title, scheme: "dark", file: true, contrast: false });
const lightFile = (value: string, title: string): ThemeInfo => ({ value, title, scheme: "light", file: true, contrast: false });

/** The themes: the dark ones, then the light, each with Default first and high contrast last. */
export const THEMES: ThemeInfo[] = [
  own("dark"),
  darkFile("dark-modern", "Dark Modern"),
  darkFile("dark-plus", "Dark+"),
  darkFile("abyss", "Abyss"),
  darkFile("kimbie-dark", "Kimbie Dark"),
  darkFile("monokai", "Monokai"),
  darkFile("monokai-dimmed", "Monokai Dimmed"),
  darkFile("solarized-dark", "Solarized Dark"),
  darkFile("tomorrow-night-blue", "Tomorrow Night Blue"),
  { ...darkFile("dark-high-contrast", "Dark High Contrast"), contrast: true },
  own("light"),
  lightFile("light-modern", "Light Modern"),
  lightFile("light-plus", "Light+"),
  lightFile("powershell-ise", "PowerShell ISE"),
  lightFile("quiet-light", "Quiet Light"),
  lightFile("solarized-light", "Solarized Light"),
  { ...lightFile("light-high-contrast", "Light High Contrast"), contrast: true },
];

/** The sections of Settings' theme files, one for each appearance. */
export const THEME_SECTIONS: { value: Scheme; title: string }[] = [
  { value: "dark", title: "Dark" },
  { value: "light", title: "Light" },
];

/** The theme picked for each appearance. */
export const picked: Record<Scheme, Signal<ThemeName>> = {
  dark: signal<ThemeName>("default-dark"),
  light: signal<ThemeName>("default-light"),
};

/** The theme the app shows: the one picked for the appearance shown. */
export const theme = computed<ThemeName>(() => (colorScheme() === "dark" ? picked.dark() : picked.light()));

function info(name: ThemeName): ThemeInfo | undefined {
  return THEMES.find((each) => each.value === name);
}

/** `name` as `scheme`'s theme: Default's for an unknown name or one of the other appearance. */
export function themeIn(name: string, scheme: Scheme): ThemeName {
  const found = info(name);
  return found?.scheme === scheme ? found.value : defaultTheme(scheme);
}

/** Whether `name` is a high contrast theme. */
export function highContrast(name: ThemeName): boolean {
  return info(name)?.contrast ?? false;
}

/** The appearance `name` is drawn in; an unknown name is dark. */
export function themeScheme(name: ThemeName): Scheme {
  return info(name)?.scheme ?? "dark";
}

const isFile = (name: ThemeName) => info(name)?.file ?? false;

/** Shows `name`'s colors: its theme file's tokens over the app's, or the app's own again. */
export function applyThemeColors(name: ThemeName): Promise<void> {
  return themes().apply(isFile(name) ? name : null);
}

const colorsMade = new Map<string, Record<string, string>>();

/** Every color of `name`, by token, in its own appearance whatever the app shows now. */
export function themeColors(name: ThemeName): Record<string, string> {
  const file = isFile(name) ? name : null;
  const night = themeScheme(name) === "dark";
  const key = `${file ?? "own"}:${night}`;
  let colors = colorsMade.get(key);
  if (!colors) {
    colors = Object.fromEntries(themes().colors(file, night).map((each) => [each.name, each.value.toLowerCase()]));
    colorsMade.set(key, colors);
  }
  return colors;
}

/** `top` over `under`: the opaque color that shows. */
export function over(top: string, under: string): string {
  const rgba = (hex: string) => {
    const digits = hex.slice(1);
    const byte = (at: number) => parseInt(digits.slice(at, at + 2), 16);
    return { r: byte(0), g: byte(2), b: byte(4), a: digits.length === 8 ? byte(6) / 255 : 1 };
  };
  const a = rgba(top);
  const b = rgba(under);
  const mix = (x: number, y: number) =>
    Math.round(x * a.a + y * (1 - a.a))
      .toString(16)
      .padStart(2, "0");
  return `#${mix(a.r, b.r)}${mix(a.g, b.g)}${mix(a.b, b.b)}`;
}

/** A one-color fill for `backgroundImage`, for a color no class names. */
export function solid(color: string): string {
  return `linear-gradient(180deg, ${color}, ${color})`;
}

/** A strip in `token`'s color (or `token` itself, a `#rrggbb`), solid at the `side` edge and fading away from it. */
export function fadeFrom(token: string, side: EdgeSide): string {
  const color = token.startsWith("#") ? token : (themeColors(theme())[token] ?? "#00000000");
  return `linear-gradient(${side === "top" ? "180deg" : "0deg"}, ${color}, ${color} 16%, ${withAlpha(color, 0.7)} 40%, ${withAlpha(color, 0)})`;
}

const shows = (color: string | undefined): color is string => color !== undefined && !/^#[0-9a-f]{6}00$/.test(color);

/** `color` (`#rrggbb` or `#rrggbbaa`) at `alpha`, from 0 to 1. */
export function withAlpha(color: string, alpha: number): string {
  const byte = Math.round(Math.min(1, Math.max(0, alpha)) * 255);
  return `${color.slice(0, 7)}${byte.toString(16).padStart(2, "0")}`;
}

/** The focus rings of one color, each class written out whole. */
export type Rings = { visible: string; field: string; within: string; nav: string; shown: string };

const STOCK_RINGS: Rings = {
  visible: "focus-visible:ring-2 focus-visible:ring-ring",
  field: "focus:ring-2 focus:ring-ring",
  within: "focus-within:ring-2 focus-within:ring-ring",
  nav: "focus-visible:ring-2 focus-visible:ring-sidebar-ring",
  shown: "ring-2 ring-ring",
};

const THIN_RINGS: Rings = {
  visible: "focus-visible:ring-1 focus-visible:ring-ring",
  field: "focus:ring-1 focus:ring-ring",
  within: "focus-within:ring-1 focus-within:ring-ring",
  nav: "focus-visible:ring-1 focus-visible:ring-sidebar-ring",
  shown: "ring-2 ring-ring",
};

/** A kind of button that isn't the primary one: its classes, its label's class, and its icon's tint. */
export type ButtonLook = { frame: string; label: string; tint: IconTint };

/** The places a one-point line frames. */
export type EdgePlace = "sidebar" | "panel" | "page";

/** What a theme decides for every place that follows it, as Tailwind classes written out whole and styles. */
export interface Look {
  glow: string;
  cornerLight: string;
  /** The panels' headers' color class, or `""` for the color of what each tops. */
  header: string;
  /** The title bar's color class, or `""` for the color of what it tops. */
  titleBar: string;
  titleText?: string;
  edges: Record<EdgePlace, string>;
  accent: (index: number) => string;
  /** What reads on `chart-1` to `chart-4`: one ink for all four. */
  chartInk: string;
  /** A heatmap's colors in chart color `chart` (0 for `chart-1`); opaque, since a ramp mixes them. */
  heat: (chart: number) => { empty: string; low: string; high: string };
  edge: string;
  /** A card's border color class; Default's is transparent, so both lay out alike. */
  cardEdge: string;
  badge: { fill: string; shadow: string; icon?: string };
  onInk?: string;
  navInk?: string;
  /** A selected sidebar row's text, as `group-selected:` classes (`navOn` sets `group`). */
  navText?: string;
  rings: Rings;
  primary: { frame: string; style: ViewStyle; label: string };
  danger: { frame: string; label: string; ink: string };
  buttons: { secondary: ButtonLook; ghost: ButtonLook };
  on: string;
  onStyle: ViewStyle;
  navOn: string;
  listRow: { hover: string; selected: string; selectedText?: string };
  /** A search's colors: every match, the one the reader is on, and the mark beside a line or on a scroll bar. */
  found: { match: string; current: string; mark: string };
  chipOpen: string;
  chip: { frame: string; text: string };
  popup: { class: string; style: ViewStyle };
  /** A row being dragged: ringed rather than bordered, so it keeps its size. */
  dragged: { class: string; style: ViewStyle };
  menuItem: { row: string; text: string; picture: string; option: string; line: string; activeInk?: string };
  tooltip: { class: string; style: ViewStyle; text: string };
  field: { frame: string; border: string };
  slider?: { rangeStyle: ViewStyle; thumb: string };
  check: { box: string; style: ViewStyle; icon?: string };
  segment: ViewStyle;
  toggle: { track: string; frame: string; on: string; off: string };
  tabs: { indicator: string; on: string; off: string };
  statusBar: { frame: string; text: string };
  /** The islands' surfaces back to front, as `#rrggbb`: the frame, a tool pane's and the document's. */
  tiers: { frame: string; tool: string; document: string };
  surface: ViewStyle;
  control: ViewStyle;
  well: ViewStyle;
  navLift: ViewStyle;
  thumb: ViewStyle;
  halo: string;
  scrollShade: ScrollShade;
}

const black = (alpha: number) => `rgb(0 0 0 / ${alpha})`;
const white = (alpha: number) => `rgb(255 255 255 / ${alpha})`;

/** What sits over a camera's picture, the same in every theme: its surround is black in both. */
export const OVERLAY = {
  chip: "rounded bg-black/60",
  dim: "bg-black/65",
  notice: { title: "text-white", text: "text-white/80", detail: "text-white/60" },
  check: "border-2 border-white bg-black/30",
  toolbar: "rounded-full bg-black/60",
  button: "hover:bg-white/15 active:bg-white/25 selected:bg-white/20",
  ink: white(0.95),
  segments: {
    track: "rounded-full bg-white/15",
    segment: "rounded-full hover:bg-white/15",
    on: "bg-white/90 hover:bg-white/90",
    onText: "text-black",
    offText: "text-white",
  },
  separator: "bg-white/25",
  arrange: {
    frame: "rounded-lg border border-white/30 bg-white/10",
    lifted: "rounded-lg border-2 border-primary bg-white/15",
    ghost: "rounded-lg border-2 border-primary bg-primary/20",
    grip: {
      corner: "border-white/90 group-hover:border-primary",
      edge: "rounded-full bg-white/90 group-hover:bg-primary",
      shade: `drop-shadow(0 0 1px ${black(0.8)})`,
    },
    hint: "text-white/80",
  },
};

function rings(ink: (alpha: number) => string, first: number, alphas: number[]): string {
  const stops = alphas.flatMap((alpha, index) => {
    const at = first + index * 24;
    return [`${ink(0)} ${at - 1}px`, `${ink(alpha)} ${at - 1}px`, `${ink(alpha)} ${at}px`, `${ink(0)} ${at}px`];
  });
  return `radial-gradient(circle, ${stops.join(", ")})`;
}

// Hovers tint with the text's color: nsh's stock `accent` is `secondary`'s own color in the dark.
const STOCK_PLACES = {
  titleText: undefined,
  // The page draws no lines; the panel's stays, since it is what resizes the PTZ panel.
  edges: { sidebar: "bg-sidebar-border", panel: "bg-border", page: "bg-transparent" },
  buttons: {
    secondary: {
      frame: "border border-border bg-secondary hover:border-foreground/20 hover:bg-foreground/10 active:bg-foreground/20",
      label: "text-secondary-foreground",
      tint: "secondary-foreground",
    },
    ghost: { frame: "hover:bg-foreground/10 active:bg-foreground/20", label: "text-foreground", tint: "foreground" },
  },
  chip: {
    frame: "border-input bg-transparent dark:bg-input/30 hover:bg-foreground/10 dark:hover:bg-foreground/10 active:bg-foreground/20 dark:active:bg-foreground/20",
    text: "text-foreground",
  },
  menuItem: {
    row: "hover:bg-accent focus:bg-accent",
    text: "text-popover-foreground",
    picture: "bg-muted-foreground",
    option: "text-popover-foreground focus:bg-accent focus:text-accent-foreground",
    line: "bg-border",
  },
  field: { frame: "bg-transparent text-foreground placeholder:text-muted-foreground dark:bg-input/30", border: "border-field-border" },
  toggle: {
    track: "bg-muted",
    frame: "hover:bg-card/60 aria-pressed:bg-card dark:hover:bg-background/60 dark:aria-pressed:bg-background",
    on: "font-medium text-foreground",
    off: "text-muted-foreground",
  },
  tabs: { indicator: "bg-tab-border", on: "text-foreground", off: "text-muted-foreground hover:text-foreground" },
  statusBar: { frame: "bg-sidebar", text: "text-muted-foreground" },
} satisfies Partial<Look>;

const FLAT = { surface: {}, control: {}, well: {}, onStyle: {}, navLift: {}, thumb: {}, scrollShade: NO_SCROLL_SHADE } satisfies Partial<Look>;

function foundOf(colors: Record<string, string>): Look["found"] {
  return { match: withAlpha(colors.warning, 0.3), current: withAlpha(colors.warning, 0.7), mark: colors.warning };
}

function dangerLook(colors: Record<string, string>): Look["danger"] {
  const fill = over(colors.destructive, colors.background);
  const darkInk = contrast("#000000", fill) >= contrast("#ffffff", fill);
  return {
    frame: "bg-destructive hover:ring-2 hover:ring-ring active:ring-2 active:ring-ring",
    label: darkInk ? "text-black" : "text-white",
    ink: darkInk ? black(1) : white(1),
  };
}

function stockLook(name: ThemeName): Look {
  const night = themeScheme(name) === "dark";
  const colors = themeColors(name);
  const border = colors.border;
  return {
    ...STOCK_PLACES,
    ...FLAT,
    glow: "none",
    cornerLight: "none",
    header: "bg-sidebar",
    titleBar: "bg-sidebar",
    accent: () => border,
    chartInk: chartInk(colors),
    heat: heatOf(colors),
    edge: "none",
    cardEdge: "border-card-border",
    badge: { fill: "none", shadow: "none" },
    rings: STOCK_RINGS,
    primary: { frame: "bg-primary hover:opacity-80 active:opacity-70", style: {}, label: "text-primary-foreground" },
    danger: dangerLook(colors),
    on: "selected:bg-foreground/15",
    navOn: "selected:bg-sidebar-accent",
    listRow: { hover: "hover:bg-muted/50", selected: "bg-muted" },
    found: foundOf(colors),
    chipOpen: "aria-expanded:bg-accent",
    popup: { class: "rounded-lg border border-border bg-popover", style: {} },
    dragged: { class: "rounded-md bg-popover ring-1 ring-border", style: {} },
    tooltip: { class: "rounded-md bg-primary px-2 py-1", style: {}, text: "text-primary-foreground" },
    check: { box: "border-input checked:border-primary checked:bg-primary indeterminate:border-primary indeterminate:bg-primary", style: {} },
    segment: {},
    halo: rings(night ? white : black, 52, [0.07, 0.05, 0.035, 0.02]),
    tiers: tiersOf(name),
  };
}

// The high contrast themes outline states in `state-outline`, which is transparent in the others.
function flatLook(name: ThemeName): Look {
  const colors = themeColors(name);
  const outlined = (color: string | undefined): ViewStyle => (shows(color) ? { boxShadow: `inset 0 0 0 1px ${color}` } : {});
  const hoverOutline = "hover:ring-1 hover:ring-state-outline";
  const accents = [colors["chart-1"], colors["chart-2"], colors["chart-3"]];
  return {
    ...FLAT,
    glow: "none",
    cornerLight: solid(colors.card),
    edge: solid(colors["card-border"]),
    cardEdge: "border-border",
    header: "",
    titleBar: "bg-title-bar",
    titleText: "text-title-bar-foreground",
    edges: { sidebar: "bg-sidebar-border", panel: "bg-border", page: "bg-border" },
    accent: (index) => accents[index % accents.length],
    chartInk: chartInk(colors),
    heat: heatOf(colors),
    badge: { fill: solid(colors.badge), shadow: shows(colors["badge-border"]) ? `inset 0 0 0 1px ${colors["badge-border"]}` : "none", icon: colors["badge-foreground"] },
    onInk: colors["option-foreground"],
    navInk: colors["nav-selection-foreground"],
    rings: THIN_RINGS,
    primary: {
      frame: `border border-button-border bg-primary hover:bg-primary-hover active:bg-primary-hover ${hoverOutline}`,
      style: {},
      label: "text-primary-foreground",
    },
    danger: dangerLook(colors),
    buttons: {
      secondary: {
        frame: `border border-button-border bg-secondary hover:bg-secondary-hover active:bg-secondary-hover ${hoverOutline}`,
        label: "text-secondary-foreground",
        tint: "secondary-foreground",
      },
      ghost: { frame: `hover:bg-toolbar-hover active:bg-toolbar-active ${hoverOutline}`, label: "text-foreground", tint: "icon" },
    },
    on: "selected:bg-option",
    onStyle: outlined(colors["option-border"]),
    navOn: `group selected:bg-nav-selection ${hoverOutline}`,
    navText: "group-selected:text-nav-selection-foreground",
    navLift: outlined(colors["state-outline"]),
    listRow: {
      hover: `hover:bg-toolbar-hover ${hoverOutline}`,
      selected: "bg-nav-selection ring-1 ring-state-outline",
      selectedText: "text-nav-selection-foreground",
    },
    found: foundOf(colors),
    chipOpen: "aria-expanded:border-ring",
    chip: { frame: "border-dropdown-border bg-dropdown", text: "text-dropdown-foreground" },
    popup: { class: "rounded-md border border-menu-border bg-popover", style: {} },
    dragged: { class: "rounded-md bg-popover ring-1 ring-menu-border", style: {} },
    menuItem: {
      row: "group hover:bg-menu-selection focus:bg-menu-selection hover:ring-1 hover:ring-state-outline focus:ring-1 focus:ring-state-outline",
      text: "text-popover-foreground group-hover:text-menu-selection-foreground group-focus:text-menu-selection-foreground",
      picture: "bg-muted-foreground group-hover:bg-menu-selection-foreground group-focus:bg-menu-selection-foreground",
      option:
        "text-popover-foreground hover:bg-menu-selection hover:text-menu-selection-foreground focus:bg-menu-selection focus:text-menu-selection-foreground hover:ring-1 hover:ring-state-outline focus:ring-1 focus:ring-state-outline",
      line: "bg-menu-separator",
      activeInk: colors["menu-selection-foreground"],
    },
    tooltip: { class: "rounded-sm border border-tooltip-border bg-tooltip px-2 py-1", style: {}, text: "text-tooltip-foreground" },
    field: { frame: "bg-field text-field-foreground placeholder:text-field-placeholder", border: "border-field-border" },
    slider: { rangeStyle: { backgroundImage: solid(colors.progress) }, thumb: "border-progress" },
    check: { box: "border-checkbox-border bg-checkbox", style: {}, icon: colors["checkbox-foreground"] },
    segment: { backgroundImage: solid(colors.option), ...outlined(colors["option-border"]) },
    toggle: { track: "border border-field-border bg-field", frame: "hover:bg-option-hover", on: "text-option-foreground", off: "text-field-foreground" },
    tabs: { indicator: "bg-tab-border", on: "text-tab-active", off: "text-tab-inactive hover:text-tab-active" },
    statusBar: { frame: "bg-status-bar", text: "text-status-bar-foreground" },
    halo: "none",
    tiers: tiersOf(name),
  };
}

/** WCAG contrast between the frame and the document, so a tool island halfway stands apart from both. */
const TIERS_APART = { dark: 1.3, light: 1.18 };

/** In hundredths. */
const MOST_MOVED = 60;

function movedApart(color: string, from: string, toward: string, apart: number): string | undefined {
  for (let step = 0; step <= MOST_MOVED; step++) {
    const moved = over(faded(toward, step / 100), color);
    if (contrast(moved, from) >= apart) return moved;
  }
  return undefined;
}

const awayFrom = (color: string, from: string) => (luminance(color) >= luminance(from) ? "#ffffff" : "#000000");

function tiersOf(name: ThemeName): Look["tiers"] {
  const colors = themeColors(name);
  const page = colors.background.slice(0, 7);
  const sidebar = over(colors.sidebar, page);
  if (highContrast(name)) return { frame: sidebar, tool: page, document: page };
  const apart = TIERS_APART[themeScheme(name)];
  let frame = sidebar;
  let document = page;
  if (isFile(name)) {
    const away = awayFrom(sidebar, page);
    frame = movedApart(sidebar, page, away, apart) ?? movedApart(sidebar, page, away === "#ffffff" ? "#000000" : "#ffffff", apart) ?? sidebar;
  } else {
    const card = over(colors.card, page);
    const moved = movedApart(card, sidebar, awayFrom(card, sidebar), apart);
    document = moved ?? card;
    if (!moved) frame = movedApart(sidebar, card, awayFrom(sidebar, card), apart) ?? sidebar;
  }
  return { frame, tool: over(faded(document, 0.5), frame), document };
}

function luminance(color: string): number {
  const channel = (at: number) => {
    const value = parseInt(color.slice(at, at + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

/** Two `#rrggbb` colors' WCAG contrast ratio: 1 for the same, 21 for black on white. */
export function contrast(one: string, other: string): number {
  const [light, dark] = [luminance(one), luminance(other)].sort((a, b) => b - a);
  return (light + 0.05) / (dark + 0.05);
}

function chartInk(colors: Record<string, string>): string {
  const card = colors.card.slice(0, 7);
  const text = colors.foreground.slice(0, 7);
  const darkest = luminance(card) < luminance(text) ? card : text;
  const fills = [1, 2, 3, 4].map((number) => colors[`chart-${number}`].slice(0, 7));
  const least = (ink: string) => Math.min(...fills.map((fill) => contrast(ink, fill)));
  return least("#ffffff") >= least(darkest) ? "#ffffff" : darkest;
}

function faded(color: string, alpha: number): string {
  return `${color.slice(0, 7)}${Math.round(alpha * 255)
    .toString(16)
    .padStart(2, "0")}`;
}

// The empty cell's alpha is `REST`'s in `chart.tsx`.
function heatOf(colors: Record<string, string>): Look["heat"] {
  const card = over(colors.card, colors.background);
  const empty = over(faded(colors["muted-foreground"], 0.15), card);
  return (chart) => {
    const high = colors[`chart-${(chart % 5) + 1}`].slice(0, 7);
    return { empty, low: over(faded(high, 0.25), card), high };
  };
}

function isLight(color: string): boolean {
  const channel = (at: number) => parseInt(color.slice(at, at + 2), 16) / 255;
  return 0.299 * channel(1) + 0.587 * channel(3) + 0.114 * channel(5) > 0.5;
}

function readableSheen(ink: string, fills: string[], maximum: number): number {
  let alpha = Math.round(maximum * 100);
  while (
    alpha > 0 &&
    fills.some((fill) => {
      const lit = over(withAlpha("#ffffff", alpha / 100), fill);
      return contrast(over(ink, lit), lit) < 4.5;
    })
  ) {
    alpha--;
  }
  return alpha / 100;
}

function depthOf(name: ThemeName, level: ElevationName): Depth {
  const night = themeScheme(name) === "dark";
  if (level === "none") return depth(level, night);
  const colors = themeColors(name);
  const light = (color: string) => isLight(over(color, colors.background));
  const field = over(colors.field, colors.background);
  const fills = isFile(name)
    ? {
        tooltip: light(colors.tooltip),
        primary: light(colors.primary),
        check: light(colors.checkbox),
        segment: isLight(over(colors.option, field)),
        badge: light(colors.badge),
        well: isLight(field),
      }
    : { tooltip: light(colors.primary), primary: light(colors.primary), check: light(colors.primary), badge: light(colors.muted) };
  const surfaces = { card: light(colors.card), popup: light(colors.popover) };
  const result = depth(level, night, { ...surfaces, ...fills });
  const page = colors.background.slice(0, 7);
  const backgrounds = [page, over(colors.card, page), over(colors.popover, page)];
  if (result.primary.light) {
    const buttons = isFile(name) ? [colors.primary, colors["primary-hover"]] : [colors.primary];
    const buttonFills = backgrounds.flatMap((background) => buttons.map((button) => over(button, background)));
    const alpha = readableSheen(colors["primary-foreground"], buttonFills, fills.primary ? SHEEN.primaryBright : SHEEN.primaryDark);
    result.primary.light = alpha > 0 ? primaryLight(alpha) : undefined;
  }
  if (result.segment.light) {
    const segments = isFile(name)
      ? backgrounds.map((background) => over(colors.option, over(colors.field, background)))
      : [night ? page : over(colors.card, page)];
    const ink = isFile(name) ? colors["option-foreground"] : colors.foreground;
    const alpha = readableSheen(ink, segments, SHEEN.segmentTop);
    result.segment.light = alpha > 0 ? segmentLight(alpha, Math.min(alpha, SHEEN.segmentBottom)) : undefined;
  }
  return result;
}

const drawn = (each: string | undefined): each is string => each !== undefined && each !== "" && each !== "none";

function layered(...paints: (string | undefined)[]): string {
  const shown = paints.filter(drawn);
  return shown.length > 0 ? shown.join(", ") : "none";
}

function lifted(style: ViewStyle, lift: Lift): ViewStyle {
  const image = layered(lift.light, style.backgroundImage);
  const shadow = layered(typeof style.boxShadow === "string" ? style.boxShadow : undefined, ...lift.shadows);
  const next: ViewStyle = { ...style };
  if (image !== "none") next.backgroundImage = image;
  if (shadow !== "none") next.boxShadow = shadow;
  return next;
}

function raise(base: Look, by: Depth): Look {
  return {
    ...base,
    glow: layered(base.glow, ...by.card.shadows),
    cornerLight: layered(by.card.light, base.cornerLight),
    edge: layered(by.card.edge, base.edge),
    badge: { ...base.badge, fill: layered(by.badge.light, base.badge.fill), shadow: layered(base.badge.shadow, ...by.badge.shadows) },
    primary: { ...base.primary, style: lifted(base.primary.style, by.primary) },
    popup: { ...base.popup, style: lifted(base.popup.style, by.popup) },
    dragged: { ...base.dragged, style: lifted(base.dragged.style, by.popup) },
    tooltip: { ...base.tooltip, style: lifted(base.tooltip.style, by.tooltip) },
    check: { ...base.check, style: lifted(base.check.style, by.check) },
    segment: lifted(base.segment, by.segment),
    surface: lifted(base.surface, by.surface),
    control: lifted(base.control, by.control),
    well: lifted(base.well, by.well),
    onStyle: lifted(base.onStyle, by.well),
    navLift: lifted(base.navLift, by.nav),
    thumb: lifted(base.thumb, by.thumb),
    scrollShade: by.scrollShade,
  };
}

const made = new Map<string, Look>();

/** How `name` draws at elevation `level`, the app's own unless given; high contrast takes none. */
export function look(name: ThemeName = theme(), level: ElevationName = elevation()): Look {
  const lift = highContrast(name) ? "none" : level;
  const key = `${name}:${lift}`;
  let found = made.get(key);
  if (!found) {
    const own = isFile(name) ? flatLook(name) : stockLook(name);
    found = raise(own, depthOf(name, lift));
    made.set(key, found);
  }
  return found;
}

/** The colors a theme picker draws a theme in, as styles: a class would take the shown theme's. */
export interface Swatch {
  title: string;
  side: string;
  sideRow: string;
  page: string;
  card: string;
  edge: string;
  cardEdge: string;
  button: string;
  status: string;
  text: string;
  muted: string;
  sideText: string;
  buttonText: string;
  statusText: string;
  accents: string[];
}

/** The colors a picker draws `name` in, in its own appearance. */
export function swatch(name: ThemeName): Swatch {
  const colors = themeColors(name);
  return isFile(name)
    ? {
        title: solid(colors["title-bar"]),
        side: solid(colors.sidebar),
        sideRow: solid(over(colors["nav-selection"], colors.sidebar)),
        page: solid(colors.background),
        card: solid(colors.card),
        edge: solid(colors["card-border"]),
        cardEdge: solid(colors["card-border"]),
        button: solid(colors.primary),
        status: solid(colors["status-bar"]),
        text: colors.foreground,
        muted: colors["muted-foreground"],
        sideText: colors["sidebar-foreground"],
        buttonText: colors["primary-foreground"],
        statusText: colors["status-bar-foreground"],
        accents: [colors["chart-1"], colors["chart-2"], colors["chart-3"]],
      }
    : {
        title: solid(colors.sidebar),
        side: solid(colors.sidebar),
        sideRow: solid(colors["sidebar-accent"]),
        page: solid(colors.background),
        card: solid(colors.card),
        edge: solid(colors.border),
        cardEdge: solid(colors["card-border"]),
        button: solid(colors.primary),
        status: solid(colors.sidebar),
        text: colors.foreground,
        muted: colors["muted-foreground"],
        sideText: colors["sidebar-foreground"],
        buttonText: colors["primary-foreground"],
        statusText: colors["muted-foreground"],
        accents: [colors.border, colors.border, colors.border],
      };
}

/** A card in the theme's look; `look` is the theme it shows, the app's unless given. */
export function BrandCard(props: { class?: string; style?: ViewStyle; radius?: number; look?: ThemeName; children?: JSX.Element }) {
  const radius = () => props.radius ?? 14;
  const shown = () => look(props.look);
  return (
    <View class="bg-border" style={{ ...props.style, borderRadius: radius(), padding: 1, backgroundImage: shown().edge, boxShadow: shown().glow }}>
      <View class={`bg-card ${props.class ?? ""}`} style={{ borderRadius: radius() - 1, backgroundImage: shown().cornerLight }}>
        {props.children}
      </View>
    </View>
  );
}

const CHART_FILLS = ["bg-chart-1", "bg-chart-2", "bg-chart-3", "bg-chart-4"];

/** An icon on a rounded square; with `chart`, on that chart color in `chartInk`. */
export function BrandBadge(props: { icon: IconName; size?: number; chart?: number; look?: ThemeName }) {
  const size = () => props.size ?? 48;
  const shown = () => look(props.look);
  const fill = () => (props.chart === undefined ? undefined : CHART_FILLS[props.chart % CHART_FILLS.length]);
  return (
    <View
      class={`items-center justify-center ${fill() ?? "bg-muted"}`}
      style={{
        width: size(),
        height: size(),
        borderRadius: size() / 4,
        backgroundImage: fill() ? "none" : shown().badge.fill,
        boxShadow: shown().badge.shadow,
      }}
    >
      <Icon name={props.icon} size={Math.round(size() * 0.46)} tint="foreground" color={fill() ? shown().chartInk : shown().badge.icon} />
    </View>
  );
}

type Edge = "top" | "bottom" | "left" | "right";

const EDGES: Record<Edge, ViewStyle> = {
  top: { top: 0, left: 0, right: 0, height: 1 },
  bottom: { bottom: 0, left: 0, right: 0, height: 1 },
  left: { left: 0, top: 0, bottom: 0, width: 1 },
  right: { right: 0, top: 0, bottom: 0, width: 1 },
};

/** A one-point line along an edge of its parent, in the color the theme gives `place`. */
export function EdgeLine(props: { edge: Edge; place?: EdgePlace }) {
  return <View class={look().edges[props.place ?? "page"]} style={{ position: "absolute", pointerEvents: "none", ...EDGES[props.edge] }} />;
}
