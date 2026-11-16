// The app's themes, and what each decides for the places that follow it. A
// component asks `look()` how to draw itself and never checks the theme, so
// changing it changes every themed place at once.
//
// Every theme is light or dark. The app keeps one of each (`picked`, Settings
// > Appearance > Theme, saved with the other preferences) and shows the one
// for the appearance it shows (`theme`): the Light and Dark modes fix it, and
// System goes from one to the other as the system's mode does.
//
// Default starts with nsh's stock colors and flat look, with the app's own
// colors and clearer text, field outlines and focus rings over them. The user
// sees one Default, dark or light as the mode is, but
// it keeps a name in each appearance (`default-dark`, `default-light`), since
// each appearance has a pick of its own and saved settings already hold those
// names. Every other theme is a theme file in `themes/`, as `theme.json` is,
// with a value for every token; Rust lays it over the app's own theme file
// while it shows (`Themes.apply`), so every class naming a token takes its
// colors. They share one look (`flatLook`): flat, with each state in a token of
// its own, such as `toolbar-hover`, `option` (a button that is on),
// `nav-selection` and `menu-selection`.
//
// Over a theme's own look goes the elevation (`elevation.ts`, `raise`), whose
// shadows and lights are the same over every theme, each place's light chosen
// by how light its color is. The high contrast themes take none: their outlines
// mark every state, and a shadow would blur them. Hovering stays the theme's,
// and red still means recording.
//
// Each look takes the theme to draw, the app's by default, so a theme picker
// can show every theme's look side by side; `swatch()` gives the colors a
// picker draws a theme's small window in, which no class does.
//
// The theme files and the Rust that lays them over the app's own are
// `dp-ui`'s (`src/lib.rs`), which every product registers in its own native
// module. These files can't import that module by name, since each product's
// has its own (`ipcamera/native`), so each app hands it over before it shows
// anything (`connectThemes`).
import { colorScheme, computed, type JSX, type Signal, signal, View, type ViewStyle } from "nsh";
import { type Depth, depth, type EdgeSide, type ElevationName, elevation, type Lift, NO_SCROLL_SHADE, primaryLight, type ScrollShade, segmentLight, SHEEN } from "./elevation";
import { Icon, type IconName, type IconTint } from "./icon";

/** A color of a theme, by its token's name (`background`, `ring`): `#RRGGBB`, or `#RRGGBBAA` when it isn't opaque. */
export type NamedColor = { name: string; value: string };

/** What draws the themes' colors: the `Themes` module `dp-ui` gives each product's native module. */
export type ThemesModule = {
  /** Draws the app with theme file `name` over its own theme, or with its own again when it is `null`. */
  apply(name: string | null): Promise<void>;
  /** Every color of theme file `name` over the app's own theme (`null`: the app's own), dark or light. */
  colors(name: string | null, dark: boolean): NamedColor[];
};

let themesModule: ThemesModule | undefined;

/**
 * Hands these controls the app's `Themes` module (`import { Themes } from
 * "<app>/native"`). The app calls it first, before it shows a theme.
 */
export function connectThemes(module: ThemesModule) {
  themesModule = module;
}

function themes(): ThemesModule {
  if (!themesModule) throw new Error("Call connectThemes(Themes) before the app shows a theme.");
  return themesModule;
}

/** A theme's name: `default-dark` or `default-light` (Default in each appearance), or a theme file's in `themes/` (`dark-modern`). */
export type ThemeName = string;

/** The appearance a theme is drawn in. */
export type Scheme = "light" | "dark";

export type ThemeInfo = {
  value: ThemeName;
  title: string;
  scheme: Scheme;
  /** Whether it is a theme file in `themes/`, rather than the app's own colors. */
  file: boolean;
  /** Whether it is a high contrast theme, which outlines every state and takes no elevation. */
  contrast: boolean;
};

/** Default's name in `scheme`. Its title is "Default" in both, since the mode, not the title, says which appearance it is in. */
export const defaultTheme = (scheme: Scheme): ThemeName => `default-${scheme}`;

const own = (scheme: Scheme): ThemeInfo => ({ value: defaultTheme(scheme), title: "Default", scheme, file: false, contrast: false });
const darkFile = (value: string, title: string): ThemeInfo => ({ value, title, scheme: "dark", file: true, contrast: false });
const lightFile = (value: string, title: string): ThemeInfo => ({ value, title, scheme: "light", file: true, contrast: false });

/**
 * The themes: the dark ones, then the light ones, each with Default first,
 * then the theme files by name, then high contrast. Settings offers
 * one Default, the one for the appearance the mode shows, in a section of its
 * own, ahead of the theme files.
 */
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

/** The sections of Settings' theme files, one for each appearance, after Default's own. */
export const THEME_SECTIONS: { value: Scheme; title: string }[] = [
  { value: "dark", title: "Dark" },
  { value: "light", title: "Light" },
];

/** The theme picked for each appearance. The app sets them from the saved preferences before its first frame. */
export const picked: Record<Scheme, Signal<ThemeName>> = {
  dark: signal<ThemeName>("default-dark"),
  light: signal<ThemeName>("default-light"),
};

/** The theme the app shows: the one picked for the appearance shown. */
export const theme = computed<ThemeName>(() => (colorScheme() === "dark" ? picked.dark() : picked.light()));

function info(name: ThemeName): ThemeInfo | undefined {
  return THEMES.find((each) => each.value === name);
}

/**
 * `name` as the theme for `scheme`: Default's for a name this version doesn't
 * know or a theme of the other appearance, as a hand-edited `settings.json` can have.
 */
export function themeIn(name: string, scheme: Scheme): ThemeName {
  const found = info(name);
  return found?.scheme === scheme ? found.value : defaultTheme(scheme);
}

/** Whether `name` is a high contrast theme, which keeps its own look whatever the elevation. */
export function highContrast(name: ThemeName): boolean {
  return info(name)?.contrast ?? false;
}

/** Light or dark: the appearance `name` is drawn in. A name this version doesn't know is dark. */
export function themeScheme(name: ThemeName): Scheme {
  return info(name)?.scheme ?? "dark";
}

const isFile = (name: ThemeName) => info(name)?.file ?? false;

/** Shows `name`'s colors: its theme file's tokens over the app's, or the app's own again. */
export function applyThemeColors(name: ThemeName): Promise<void> {
  return themes().apply(isFile(name) ? name : null);
}

/** Every color of `name`, by token: its theme file over the app's own, in its own appearance, whatever the app shows now. */
const colorsMade = new Map<string, Record<string, string>>();

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

/** `top` over `under`: the opaque color that shows, as a browser blends them. */
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

/** A fill of one color, as `backgroundImage` takes it, so a style can carry a color that no class names. */
export function solid(color: string): string {
  return `linear-gradient(180deg, ${color}, ${color})`;
}

/**
 * A strip in the shown theme's `token` color (the surface content scrolls on,
 * `sidebar`), or in `token` itself when it is a color (`#rrggbb`, one of
 * `Look.tiers`), solid along an edge on `side` for its first sixth, so nothing
 * shows through right at the edge, then fading to nothing away from it: what
 * content dissolves into at the edge of a scroll while more of it lies that
 * way. Both sides are made from the same stops, so the two ends of one list
 * look alike but for their direction.
 */
export function fadeFrom(token: string, side: EdgeSide): string {
  const color = token.startsWith("#") ? token : (themeColors(theme())[token] ?? "#00000000");
  return `linear-gradient(${side === "top" ? "180deg" : "0deg"}, ${color}, ${color} 16%, ${withAlpha(color, 0.7)} 40%, ${withAlpha(color, 0)})`;
}

/** Whether `color` (`#rrggbb` or `#rrggbbaa`) shows at all. */
const shows = (color: string | undefined): color is string => color !== undefined && !/^#[0-9a-f]{6}00$/.test(color);

/** `color` (`#rrggbb` or `#rrggbbaa`, as `token()` gives it) at `alpha`, from 0 to 1. */
export function withAlpha(color: string, alpha: number): string {
  const byte = Math.round(Math.min(1, Math.max(0, alpha)) * 255);
  return `${color.slice(0, 7)}${byte.toString(16).padStart(2, "0")}`;
}

/**
 * The focus rings of one color: for the keyboard's focus, a text field's, one
 * inside a slider's track, a sidebar row's, and one always shown (a chosen tile).
 * Each is written out whole, as every class here is.
 */
export type Rings = { visible: string; field: string; within: string; nav: string; shown: string };

const STOCK_RINGS: Rings = {
  visible: "focus-visible:ring-2 focus-visible:ring-ring",
  field: "focus:ring-2 focus:ring-ring",
  within: "focus-within:ring-2 focus-within:ring-ring",
  nav: "focus-visible:ring-2 focus-visible:ring-sidebar-ring",
  shown: "ring-2 ring-ring",
};

// The theme files' focus: a one-point outline in the ring's color.
const THIN_RINGS: Rings = {
  visible: "focus-visible:ring-1 focus-visible:ring-ring",
  field: "focus:ring-1 focus:ring-ring",
  within: "focus-within:ring-1 focus-within:ring-ring",
  nav: "focus-visible:ring-1 focus-visible:ring-sidebar-ring",
  shown: "ring-2 ring-ring",
};

/** A kind of button that isn't the primary one: its classes, its label's class, and its icon's tint. */
export type ButtonLook = { frame: string; label: string; tint: IconTint };

/** The places a one-point line frames: in the sidebar, beside a panel, and under a page's header. */
export type EdgePlace = "sidebar" | "panel" | "page";

/**
 * What a theme decides, for every place that follows it. `…class` and the
 * plain strings of classes are Tailwind classes, written out whole; styles
 * hold what a class can't say, gradients and colored shadows. A color left
 * undefined keeps the control's own. The styles and paints of the places the
 * elevation reaches (a card's, a badge's, `primary`, `popup`, `tooltip`,
 * `check`, `segment`, `surface`, `control`, `well`, `onStyle`, `navLift`,
 * `thumb`) hold the elevation's lights and shadows over the theme's own.
 */
export interface Look {
  /** The shadow under a card, or `"none"`. */
  glow: string;
  /** A faint light in two corners of a card, over its own color, or `"none"`. */
  cornerLight: string;
  /** The color of the panels' headers, as a class, or `""` for the color of what each tops. A page's header takes the page's color in every theme. */
  header: string;
  /** The color of the title bar (the header over the page, the sidebar's top row) as a class, or `""` for the color of what it tops. */
  titleBar: string;
  /** The class of the title bar's own text (the page's name, the app's), or undefined for the text's own. */
  titleText?: string;
  /** The color of the line along each edge it frames, as a class: `bg-transparent` where the theme draws none (Default's page). */
  edges: Record<EdgePlace, string>;
  /** The mark beside the `index`th of a row of items. */
  accent: (index: number) => string;
  /**
   * What reads on the first four chart colors, `chart-1` to `chart-4`, for an
   * icon on a chip of one: white, or the theme's darkest of its card and its
   * text, whichever stands out more on the one it reads least on. One for all
   * four, so a row of chips has one ink.
   */
  chartInk: string;
  /**
   * A heatmap's colors in one of the chart colors (`chart`, 0 for `chart-1`), as
   * they show on a card: a cell with nothing in it (`empty`, the faint wash a
   * bar's empty track has), the least there is (`low`, a wash of the color) and
   * the most (`high`, the color itself). Opaque, since a plot's ramp mixes
   * between them.
   */
  heat: (chart: number) => { empty: string; low: string; high: string };
  /** A card's one-point edge, over its border color, or `"none"`. */
  edge: string;
  /**
   * A card's outline, as a class: the color of its one-point border. Default
   * draws none (`card-border` is transparent there), so its cards stand off the
   * page by their color alone; a theme file outlines its cards in its `border`.
   * The border takes its point either way, so both lay out alike.
   */
  cardEdge: string;
  /** A badge's square, over `bg-muted`: its fill, its shadow, and its icon's color. */
  badge: { fill: string; shadow: string; icon?: string };
  /** The icon of a button that is on, or a segment that is chosen, when it changes. */
  onInk?: string;
  /** The icon of the selected sidebar row, when it changes. */
  navInk?: string;
  /** The text of a sidebar row while it is selected, as `group-selected:` classes (the row has `group` in `navOn`), when it changes. */
  navText?: string;
  rings: Rings;
  /** A primary button: its classes, its style, and its label's class. */
  primary: { frame: string; style: ViewStyle; label: string };
  /** A button that deletes: the theme's `destructive` with readable text and icon ink. */
  danger: { frame: string; label: string; ink: string };
  /** An ordinary button and a quiet one (a toolbar's). */
  buttons: { secondary: ButtonLook; ghost: ButtonLook };
  /** A button that is on (a panel open, a mode running), and its style while on: its outline, and pressed in where the elevation presses in. */
  on: string;
  onStyle: ViewStyle;
  /** A sidebar row that is selected. */
  navOn: string;
  /**
   * A row of a list on a page (the Cameras page's table): its fill while
   * hovered, as `hover:` classes, its fill while selected, and its text while
   * selected, when that changes.
   */
  listRow: { hover: string; selected: string; selectedText?: string };
  /**
   * What a search found, as colors: every match's fill (`match`), the one the
   * reader is on (`current`), and the mark beside a line or along a scroll bar
   * that holds one (`mark`). The theme's `warning`, as an editor's find is
   * amber; a count of matches is on `match`.
   */
  found: { match: string; current: string; mark: string };
  /** A menu's button while its menu is open. */
  chipOpen: string;
  /** A menu's button or a list's (`ChipTrigger`): its outline and fill, with their hover, and its text. */
  chip: { frame: string; text: string };
  /** What floats over the window: menus, toasts, a dialog's panel. */
  popup: { class: string; style: ViewStyle };
  /**
   * A row lifted off its list while it is dragged to a new place (the
   * sidebar's cameras): floating as a popup does, outlined with a ring rather
   * than a border so it keeps its size.
   */
  dragged: { class: string; style: ViewStyle };
  /**
   * A menu's item (`row`, with its hover and keyboard focus; `text`, which
   * follows them), a small picture in one (a layout's, `picture`, which follows
   * them too), a list's choice (`option`), the line between groups, and the
   * icon's color while the item is hovered or focused, when it changes.
   */
  menuItem: { row: string; text: string; picture: string; option: string; line: string; activeInk?: string };
  /** A button's tooltip, and its text. */
  tooltip: { class: string; style: ViewStyle; text: string };
  /** A text field: its fill and text, and its outline when it holds nothing wrong. */
  field: { frame: string; border: string };
  /** A slider's filled part and its thumb, in place of the slider's tone. */
  slider?: { rangeStyle: ViewStyle; thumb: string };
  /** A checkbox: its box's classes (`checked:` and `indeterminate:` ones for its states), its style while checked or mixed, and its mark's color. */
  check: { box: string; style: ViewStyle; icon?: string };
  /** The chosen segment of a segmented control, over its raised look. */
  segment: ViewStyle;
  /** A segmented control: its track, each segment's hover and chosen fill, and the text of the chosen one and the others. */
  toggle: { track: string; frame: string; on: string; off: string };
  /** The tabs in a page's header: the line under the chosen one, its text, and the others'. */
  tabs: { indicator: string; on: string; off: string };
  /** The status bar along the window's bottom: its color and its text. */
  statusBar: { frame: string; text: string };
  /**
   * The window's surfaces where its panes are islands (`Island`), back to
   * front, as colors (`#rrggbb`): the frame they sit on, which the sidebar
   * shares (the theme's sidebar); a tool pane's island (a reader's files, its
   * outline, the PTZ panel), halfway between; and the document's island (a
   * reader's file, the video: the theme's page, or Default's card, which
   * stands off its page as its cards do). The frame and
   * the document stand a set contrast apart (`TIERS_APART`), one of them moved
   * where the theme has them closer. The high contrast themes keep their own
   * colors: the sidebar's frame, and the page for both islands.
   */
  tiers: { frame: string; tool: string; document: string };
  /** A plain card (a group of settings), over its border and color: its shadow, and a light over its color. */
  surface: ViewStyle;
  /** A raised control with an outline (a secondary button, a menu's button): its shadow. */
  control: ViewStyle;
  /** What is set into its surface: a text field, the track of a segmented control or a slider. */
  well: ViewStyle;
  /** A selected sidebar row, over `navOn`. */
  navLift: ViewStyle;
  /** A slider's thumb, over its classes. */
  thumb: ViewStyle;
  /** Faint rings around the badge of a page with nothing to show, or `"none"`. */
  halo: string;
  /** The shade along an edge of a scroll view on either side where content passes beyond it (`ScrollEdge`), the same both ways but for its direction. The elevation's alone. */
  scrollShade: ScrollShade;
}

const black = (alpha: number) => `rgb(0 0 0 / ${alpha})`;
const white = (alpha: number) => `rgb(255 255 255 / ${alpha})`;

/**
 * What sits over a camera's picture, the same in every theme, since a picture's
 * surround is black in both appearances: a dark chip behind it (a tile's name,
 * its buttons), the same dark as a pill behind its toolbar, a faint white under
 * a button there while it is pointed at, pressed or on, and white ink. A
 * switch on the toolbar is a faint white track, its chosen segment solid white
 * with dark ink; a line parts the toolbar's groups. A gallery's picture has a
 * white ring in its corner to select it, over a faint dark fill so it shows on
 * a white picture too; once selected it takes the theme's `primary`. A picture
 * that isn't live is dimmed by a darker layer over all of it, so its last
 * picture can't pass for live; the notice on it is white, its second line a
 * softer white.
 */
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
  /**
   * A view's layout as it is edited, over the stage's black: the grid's faint
   * lines, a tile's frame, the frame being moved or resized and the outline it
   * will take, in the theme's `primary`, and the marks on a frame's corners and
   * edges that resize it: white, with a dark edge (`shade`, a filter) so they
   * show on a bright picture too, and in `primary` while the pointer is on
   * their handle, which is a `group`.
   */
  arrange: {
    line: "bg-white/10",
    frame: "rounded-md border border-white/30 bg-white/10",
    lifted: "rounded-md border-2 border-primary bg-white/15",
    ghost: "rounded-md border-2 border-primary bg-primary/20",
    grip: {
      corner: "border-white/90 group-hover:border-primary",
      edge: "rounded-full bg-white/90 group-hover:bg-primary",
      shade: `drop-shadow(0 0 1px ${black(0.8)})`,
    },
    hint: "text-white/80",
  },
};

/** Rings of `ink` around a point, 24 points apart from `first`, fainter each time out. */
function rings(ink: (alpha: number) => string, first: number, alphas: number[]): string {
  const stops = alphas.flatMap((alpha, index) => {
    const at = first + index * 24;
    return [`${ink(0)} ${at - 1}px`, `${ink(alpha)} ${at - 1}px`, `${ink(alpha)} ${at}px`, `${ink(0)} ${at}px`];
  });
  return `radial-gradient(circle, ${stops.join(", ")})`;
}

/**
 * The default theme's places that need nothing of their own: nsh's stock
 * classes, where hovering stays neutral. The theme files give each its own tokens.
 *
 * A button, or a chip, under the pointer takes a tint of the text's color, so
 * it shows on any surface, light or dark: nsh's stock `accent`, which shadcn's
 * hovers use, is `secondary`'s own color in the dark (a secondary button didn't
 * change at all) and a shade off the light page's (a ghost button hardly did).
 * A press tints it more, and a button that is on (`on`) more than a hover.
 */
const STOCK_PLACES = {
  titleText: undefined,
  // The page draws no lines: its header's bar, a table's pager bar and the PTZ panel's title are the page's own color,
  // as the cards are told from it by their color alone. The panel's line stays: it is what resizes the PTZ panel.
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
  // The chosen segment stands out of its track as the card it sits on: in the light, the page is
  // a gray close to the track's, so the segment takes the card's white; in the dark, the page's
  // near-black reads better on the track than the card does.
  toggle: {
    track: "bg-muted",
    frame: "hover:bg-card/60 aria-pressed:bg-card dark:hover:bg-background/60 dark:aria-pressed:bg-background",
    on: "font-medium text-foreground",
    off: "text-muted-foreground",
  },
  // The chosen tab's line is `tab-border`: the text's color in the light, a blue in the dark, where white glared.
  tabs: { indicator: "bg-tab-border", on: "text-foreground", off: "text-muted-foreground hover:text-foreground" },
  // The sidebar's color, as the title bar's, so the window's frame is one color around what it shows.
  statusBar: { frame: "bg-sidebar", text: "text-muted-foreground" },
} satisfies Partial<Look>;

/** The places that only the elevation gives depth to, flat in a theme's own look. */
const FLAT = { surface: {}, control: {}, well: {}, onStyle: {}, navLift: {}, thumb: {}, scrollShade: NO_SCROLL_SHADE } satisfies Partial<Look>;

/** A search's colors (`Look.found`), in the theme's `warning`. */
function foundOf(colors: Record<string, string>): Look["found"] {
  return { match: withAlpha(colors.warning, 0.3), current: withAlpha(colors.warning, 0.7), mark: colors.warning };
}

/** A destructive action keeps its red fill and chooses the ink that reads on it. */
function dangerLook(colors: Record<string, string>): Look["danger"] {
  const fill = over(colors.destructive, colors.background);
  const darkInk = contrast("#000000", fill) >= contrast("#ffffff", fill);
  return {
    frame: "bg-destructive hover:ring-2 hover:ring-ring active:ring-2 active:ring-ring",
    label: darkInk ? "text-black" : "text-white",
    ink: darkInk ? black(1) : white(1),
  };
}

// The default look starts with nsh's flat, neutral colors: the title bar and
// the panels' headers share the sidebar's color, and empty pages have faint
// rings, black or white at low alphas.
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
    // As shadcn's data table: a faint muted fill under the pointer, the whole of it when selected.
    listRow: { hover: "hover:bg-muted/50", selected: "bg-muted" },
    found: foundOf(colors),
    chipOpen: "aria-expanded:bg-accent",
    popup: { class: "rounded-lg border border-border bg-popover", style: {} },
    dragged: { class: "rounded-md bg-popover ring-1 ring-border", style: {} },
    tooltip: { class: "rounded-md bg-primary px-2 py-1", style: {}, text: "text-primary-foreground" },
    // Mixed (a dash) is filled as checked is.
    check: { box: "border-input checked:border-primary checked:bg-primary indeterminate:border-primary indeterminate:bg-primary", style: {} },
    segment: {},
    halo: rings(night ? white : black, 52, [0.07, 0.05, 0.035, 0.02]),
    tiers: tiersOf(name),
  };
}

// The theme files' look: flat, with each place and each state in a token of its
// own, which classes name (`hover:bg-toolbar-hover`, `selected:bg-option`), so
// it follows whichever theme file shows. The colors a style carries (a badge's,
// an outline's, an icon's) are the theme file's own, read from Rust. The high
// contrast themes outline what is hovered or selected in `state-outline`, which
// is transparent in the others.
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
      // A toolbar's button: its icon in `icon`, on `toolbar-hover` and `toolbar-active` while hovered and pressed.
      ghost: { frame: `hover:bg-toolbar-hover active:bg-toolbar-active ${hoverOutline}`, label: "text-foreground", tint: "icon" },
    },
    // A button that is on is a toggled option: its fill, its outline and its icon's color.
    on: "selected:bg-option",
    onStyle: outlined(colors["option-border"]),
    navOn: `group selected:bg-nav-selection ${hoverOutline}`,
    navText: "group-selected:text-nav-selection-foreground",
    navLift: outlined(colors["state-outline"]),
    // A list's rows as the sidebar's: its selection's fill and text, outlined in the high contrast themes.
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
      // High contrast themes leave `menu-selection` clear and outline the item in `state-outline` instead.
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
    // A segmented control is a field holding a row of options, each a toggle.
    toggle: { track: "border border-field-border bg-field", frame: "hover:bg-option-hover", on: "text-option-foreground", off: "text-field-foreground" },
    tabs: { indicator: "bg-tab-border", on: "text-tab-active", off: "text-tab-inactive hover:text-tab-active" },
    statusBar: { frame: "bg-status-bar", text: "text-status-bar-foreground" },
    halo: "none",
    tiers: tiersOf(name),
  };
}

/**
 * How far apart the frame and the document's island stand, as WCAG's contrast
 * ratio, so a tool pane's island halfway between stands apart from both. Most
 * themes set their sidebar barely off their page (1.05 to 1.12); a light window
 * shows a step with less.
 */
const TIERS_APART = { dark: 1.3, light: 1.18 };

/** The farthest a surface moves toward white or black to stand `TIERS_APART`, in hundredths. */
const MOST_MOVED = 60;

/** `color` moved toward `toward` (white or black) by the least that stands it `apart` from `from`, or undefined when no move up to `MOST_MOVED` does. */
function movedApart(color: string, from: string, toward: string, apart: number): string | undefined {
  for (let step = 0; step <= MOST_MOVED; step++) {
    const moved = over(faded(toward, step / 100), color);
    if (contrast(moved, from) >= apart) return moved;
  }
  return undefined;
}

/** White if `color` is lighter than `from`, else black: the way that takes it further off. */
const awayFrom = (color: string, from: string) => (luminance(color) >= luminance(from) ? "#ffffff" : "#000000");

/**
 * `name`'s surfaces where its panes are islands: the frame, a tool pane's
 * island and the document's (`Look.tiers`). A theme file keeps its page, whose
 * color is the theme's own as much as anything, and moves its sidebar's color
 * off it: away from the page, or past it where there is no room that way (a
 * sidebar darker than a near-black page). Default keeps its frame, which is
 * one color with its title bar and status bar, and moves the card off it, or
 * the frame where the card has no room (white, in the light).
 */
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

/** How bright `color` (`#rrggbb`) is to the eye, from 0 for black to 1 for white, as WCAG weighs it. */
function luminance(color: string): number {
  const channel = (at: number) => {
    const value = parseInt(color.slice(at, at + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

/** How far apart two colors (`#rrggbb`) stand, as WCAG's contrast ratio: 1 for the same, 21 for black on white. */
export function contrast(one: string, other: string): number {
  const [light, dark] = [luminance(one), luminance(other)].sort((a, b) => b - a);
  return (light + 0.05) / (dark + 0.05);
}

/** What reads on `colors`' first four chart colors: white, or its darkest of its card and its text, whichever stands out more where it stands out least. */
function chartInk(colors: Record<string, string>): string {
  const card = colors.card.slice(0, 7);
  const text = colors.foreground.slice(0, 7);
  const darkest = luminance(card) < luminance(text) ? card : text;
  const fills = [1, 2, 3, 4].map((number) => colors[`chart-${number}`].slice(0, 7));
  const least = (ink: string) => Math.min(...fills.map((fill) => contrast(ink, fill)));
  return least("#ffffff") >= least(darkest) ? "#ffffff" : darkest;
}

/** `color` (`#rrggbb`, or `#rrggbbaa`, whose own alpha it replaces) at `alpha`, 0 to 1, as `#rrggbbaa`. */
function faded(color: string, alpha: number): string {
  return `${color.slice(0, 7)}${Math.round(alpha * 255)
    .toString(16)
    .padStart(2, "0")}`;
}

/**
 * A heatmap's colors in each chart color of `colors`, over its card: the
 * empty cell the muted text at the alpha of `REST` (`ui/chart.tsx`), the
 * least a wash of the chart color, and the most the chart color itself.
 */
function heatOf(colors: Record<string, string>): Look["heat"] {
  const card = over(colors.card, colors.background);
  const empty = over(faded(colors["muted-foreground"], 0.15), card);
  return (chart) => {
    const high = colors[`chart-${(chart % 5) + 1}`].slice(0, 7);
    return { empty, low: over(faded(high, 0.25), card), high };
  };
}

/** Whether `color` (`#rrggbb`) is light: over half of white, as the eye weighs red, green and blue. */
function isLight(color: string): boolean {
  const channel = (at: number) => parseInt(color.slice(at, at + 2), 16) / 255;
  return 0.299 * channel(1) + 0.587 * channel(3) + 0.114 * channel(5) > 0.5;
}

/** The strongest white sheen that leaves `ink` readable on every fill. */
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

/**
 * What `level` adds to `name`'s places: its shadows on the theme's page, and
 * each place's light by how light the place's color is in that theme. A place
 * left out takes the page's opposite: dark on a light page, light on a dark one.
 */
function depthOf(name: ThemeName, level: ElevationName): Depth {
  const night = themeScheme(name) === "dark";
  if (level === "none") return depth(level, night);
  const colors = themeColors(name);
  const light = (color: string) => isLight(over(color, colors.background));
  const field = over(colors.field, colors.background);
  // The default's tooltips and checks are in its primary color, and its badges on `muted`.
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

/** A paint or a shadow that shows: not missing, empty or `"none"`. */
const drawn = (each: string | undefined): each is string => each !== undefined && each !== "" && each !== "none";

/** `paints` stacked, the first on top, or `"none"` when none shows. */
function layered(...paints: (string | undefined)[]): string {
  const shown = paints.filter(drawn);
  return shown.length > 0 ? shown.join(", ") : "none";
}

/** `style` with `lift` added: its light over the style's own paint, and its shadows after the style's own. */
function lifted(style: ViewStyle, lift: Lift): ViewStyle {
  const image = layered(lift.light, style.backgroundImage);
  const shadow = layered(typeof style.boxShadow === "string" ? style.boxShadow : undefined, ...lift.shadows);
  const next: ViewStyle = { ...style };
  if (image !== "none") next.backgroundImage = image;
  if (shadow !== "none") next.boxShadow = shadow;
  return next;
}

/** `base`, a theme's own look, with `by`, the elevation's depth, over every place it reaches. */
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
    // A button that is on is pressed in, as a field is set in.
    well: lifted(base.well, by.well),
    onStyle: lifted(base.onStyle, by.well),
    navLift: lifted(base.navLift, by.nav),
    thumb: lifted(base.thumb, by.thumb),
    scrollShade: by.scrollShade,
  };
}

/**
 * Each theme's look at each elevation, made the first time it is asked for
 * rather than each time a control asks. A theme is drawn in its own
 * appearance, so the one shown doesn't change it.
 */
const made = new Map<string, Look>();

/**
 * How `name` draws, the app's theme unless given, at the elevation (`level`)
 * given, the app's unless given. A high contrast theme takes none.
 */
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

/**
 * The colors a theme picker draws a theme's small window in, as styles take
 * them: every fill a paint for `backgroundImage`, every text a color. A class
 * would take the colors the app shows now, so none is used.
 */
export interface Swatch {
  /** The title bar, the sidebar, its selected row and the page. */
  title: string;
  side: string;
  sideRow: string;
  page: string;
  /** A card on the page, the edge around the whole window, the card's own outline, and a primary button on the card. */
  card: string;
  edge: string;
  cardEdge: string;
  button: string;
  /** The status bar. */
  status: string;
  /** Text: on the page, quieter, on the sidebar, on the button and on the status bar. */
  text: string;
  muted: string;
  sideText: string;
  buttonText: string;
  statusText: string;
  /** The marks beside a row of items. */
  accents: string[];
}

/**
 * The colors a picker draws `name` in, in its own appearance whatever the app
 * shows now, as `look()` would.
 */
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

/**
 * A card with a one-point edge in the theme's border color, with a faint
 * light in two corners. Where the elevation lifts it, a
 * shadow falls under it and its edge is lit at the top in the dark and shaded
 * at the bottom in the light. `class` styles its surface (padding, gaps,
 * direction), `style` the card as a whole (a motion's, say). `look` is the
 * theme it shows, the app's unless given.
 */
export function BrandCard(props: { class?: string; style?: ViewStyle; radius?: number; look?: ThemeName; children?: JSX.Element }) {
  const radius = () => props.radius ?? 14;
  const shown = () => look(props.look);
  return (
    // The edge is this view's color showing through the one point around the surface.
    <View class="bg-border" style={{ ...props.style, borderRadius: radius(), padding: 1, backgroundImage: shown().edge, boxShadow: shown().glow }}>
      <View class={`bg-card ${props.class ?? ""}`} style={{ borderRadius: radius() - 1, backgroundImage: shown().cornerLight }}>
        {props.children}
      </View>
    </View>
  );
}

/** The theme's first four chart colors as a badge's square, in order, for `BrandBadge`'s `chart`. */
const CHART_FILLS = ["bg-chart-1", "bg-chart-2", "bg-chart-3", "bg-chart-4"];

/**
 * An icon on a rounded square: on the muted square, or on a theme file's
 * `badge`, beveled like a key where the elevation raises it. With `chart` (0
 * for `chart-1`), on one of the theme's first four chart colors instead, its
 * icon in the theme's `chartInk`, as a dashboard tile's chip; the bevel's edges
 * and shadow stay, its sheen goes, which would wash the color out on a light
 * page. `size` is the square's, 48 points by default; `look` the theme it
 * shows, the app's unless given.
 */
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

/**
 * A one-point line along an edge of its parent, over what is there, in the
 * color the theme gives the place it frames (`place`): in the sidebar, beside
 * a panel, or under a page's header, which is the default. The title bar and
 * the status bar have none: their color parts them from what they border.
 */
export function EdgeLine(props: { edge: Edge; place?: EdgePlace }) {
  return <View class={look().edges[props.place ?? "page"]} style={{ position: "absolute", pointerEvents: "none", ...EDGES[props.edge] }} />;
}
