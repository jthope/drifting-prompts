// The Appearance settings every product's Settings shows, as its Appearance
// section (`appearanceSection` in `settings.tsx`): the mode (light, dark, or
// as the system is), the texture and the elevation over every theme, and a dark
// theme and a light one, each theme a tile drawn in its own colors, four to a
// row in the Settings dialog. The app saves the values with its other preferences
// (`SavedAppearance`): `showAppearance` shows them before the first frame, and
// `keepAppearance` keeps them as the user picks, which these settings draw and
// report. And the title bar's quicker way to another theme (`ThemeMenu`): the
// same mode, then the themes of the appearance it shows, each beside its
// window in miniature.
import { colorScheme, effect, Scroll, setColorScheme, signal, Text, View, windowSize } from "nsh";
import { applyThemeColors, defaultTheme, highContrast, look, picked, type Scheme, swatch, THEME_SECTIONS, THEMES, type ThemeInfo, type ThemeName, theme, themeIn, themeScheme, withAlpha } from "./brand";
import { Appear } from "./appear";
import { IconTrigger } from "./button";
import { ELEVATIONS, type ElevationName, elevation, elevationNamed } from "./elevation";
import { Icon, type IconName } from "./icon";
import { MenuLine, MenuRow, PopupMenu, SectionLabel } from "./menu";
import { Press } from "./press";
import { Setting, SettingsGroup, SettingsPanel } from "./rows";
import { Segmented } from "./segmented";
import { TEXTURES, type TextureName, texture, textureNamed } from "./texture";
import { notify } from "./toast";

/** Light, dark, or as the system is. */
export type Mode = "light" | "dark" | "system";

/**
 * How the window looks, as an app saves it with its preferences
 * (`dp_ui::SavedAppearance`): the mode, as whether it is dark and whether it
 * follows the system, the theme picked for each appearance, and the texture and
 * the elevation, each by name.
 */
export interface SavedAppearance {
  dark: boolean;
  followSystem: boolean;
  darkTheme: string;
  lightTheme: string;
  texture: string;
  elevation: string;
}

/** The theme whose colors show, so a change of appearance shows the other pick's only when it differs. */
let colorsShown: ThemeName | undefined;

/** Shows `name`'s colors. A failure leaves the colors the app had, and says why. */
function showColors(name: ThemeName) {
  if (name === colorsShown) return;
  colorsShown = name;
  applyThemeColors(name).catch((error) => notify(`The theme's colors could not be shown: ${String(error)}`, "warn"));
}

/**
 * Shows the appearance `saved` keeps. An app calls it before its first frame,
 * so a light window never flashes dark or the other way round, and the theme,
 * texture and elevation never flash others. A name this version doesn't know,
 * as a hand-edited `settings.json` can have, is Default.
 */
export function showAppearance(saved: SavedAppearance): void {
  picked.dark.set(themeIn(saved.darkTheme, "dark"));
  picked.light.set(themeIn(saved.lightTheme, "light"));
  setColorScheme(saved.followSystem ? "system" : saved.dark ? "dark" : "light");
  showColors(theme());
  texture.set(textureNamed(saved.texture));
  elevation.set(elevationNamed(saved.elevation));
}

/** The appearance as an app keeps it: the mode, what Appearance and the theme menu report, and what to save. */
export interface KeptAppearance {
  /** The mode: light, dark, or as the system is. */
  mode: () => Mode;
  setMode: (mode: Mode) => void;
  /** Picks `name` for its own appearance, which shows it at once while that appearance shows. */
  pickTheme: (name: ThemeName) => void;
  /** Picks Default for the appearance Light or Dark fixes, or for both with System: Settings shows one Default, and the mode decides which appearance it is in. */
  pickDefault: () => void;
  setTexture: (name: TextureName) => void;
  setElevation: (name: ElevationName) => void;
  /** The appearance as it is now, for the app to save with its other preferences. */
  saved: () => SavedAppearance;
}

/**
 * Keeps the appearance an app shows, from what `saved` keeps (shown already by
 * `showAppearance`): while the mode is System it follows the system, remembering
 * the appearance the system showed last as dark or not, and the colors follow the
 * theme picked for the appearance shown, from one pick to the other. Each pick
 * shows at once and calls `save`, the app's, which saves `saved()` with its
 * other preferences. An app calls it once, in its root component.
 */
export function keepAppearance(saved: SavedAppearance, save: () => void): KeptAppearance {
  const followSystem = signal(saved.followSystem);
  const darkMode = signal(saved.dark);
  const mode = (): Mode => (followSystem() ? "system" : darkMode() ? "dark" : "light");
  effect(() => {
    const shown = colorScheme();
    if (followSystem()) darkMode.set(shown === "dark");
  });
  effect(() => showColors(theme()));
  return {
    mode,
    setMode: (next) => {
      followSystem.set(next === "system");
      if (next !== "system") darkMode.set(next === "dark");
      setColorScheme(next);
      save();
    },
    pickTheme: (name) => {
      picked[themeScheme(name)].set(name);
      save();
    },
    pickDefault: () => {
      for (const scheme of ["dark", "light"] as const) {
        if (mode() === "system" || mode() === scheme) picked[scheme].set(defaultTheme(scheme));
      }
      save();
    },
    setTexture: (name) => {
      texture.set(name);
      save();
    },
    setElevation: (name) => {
      elevation.set(name);
      save();
    },
    saved: () => ({
      dark: darkMode(),
      followSystem: followSystem(),
      darkTheme: picked.dark(),
      lightTheme: picked.light(),
      texture: texture(),
      elevation: elevation(),
    }),
  };
}

const MODES: { value: Mode; title: string; icon: IconName }[] = [
  { value: "light", title: "Light", icon: "sun" },
  { value: "dark", title: "Dark", icon: "moon" },
  { value: "system", title: "System", icon: "monitor" },
];

const MODE_DETAILS: Record<Mode, string> = {
  light: "Always light, in the light theme picked below.",
  dark: "Always dark, in the dark theme picked below.",
  system: "Light or dark as the system is, following it when it changes, each in the theme picked below.",
};

/** A short bar standing for a line of text in a theme's small window. */
function Line(props: { color: string; width: number | `${number}%`; height?: number }) {
  return <View style={{ width: props.width, height: props.height ?? 3, borderRadius: 2, backgroundColor: props.color }} />;
}

/**
 * A theme's small window, in that theme's colors whatever the app shows now:
 * its title bar, its sidebar with a row selected, a page with a card and a
 * primary button on it, and its status bar.
 */
function ThemePreview(props: { value: ThemeName }) {
  const shown = () => swatch(props.value);
  return (
    <View class="overflow-hidden" style={{ height: 88, borderRadius: 8, padding: 1, backgroundImage: shown().edge }}>
      <View class="flex-1 overflow-hidden" style={{ borderRadius: 7 }}>
        <View class="flex-row items-center px-1.5" style={{ height: 11, backgroundImage: shown().title }}>
          <Line color={withAlpha(shown().sideText, 0.6)} width={18} height={2} />
        </View>
        <View class="flex-1 flex-row">
          <View class="gap-1.5 px-1 pt-2" style={{ width: 40, backgroundImage: shown().side }}>
            <View class="px-1">
              <Line color={withAlpha(shown().sideText, 0.55)} width={22} />
            </View>
            <View class="px-1 py-0.5" style={{ borderRadius: 2, backgroundImage: shown().sideRow }}>
              <Line color={shown().sideText} width={24} />
            </View>
            <View class="px-1">
              <Line color={withAlpha(shown().sideText, 0.55)} width={18} />
            </View>
          </View>
          <View class="flex-1 p-1.5" style={{ backgroundImage: shown().page }}>
            <View style={{ borderRadius: 4, padding: 1, backgroundImage: shown().cardEdge }}>
              <View class="gap-1 p-1.5" style={{ borderRadius: 3, backgroundImage: shown().card }}>
                <View class="flex-row items-center gap-1">
                  <Line color={shown().text} width="55%" height={4} />
                  <Each of={[0, 1, 2]} key={(index) => String(index)}>
                    {(index) => <View style={{ width: 4, height: 4, borderRadius: 2, backgroundColor: shown().accents[index()] }} />}
                  </Each>
                </View>
                <Line color={shown().muted} width="70%" />
                <View class="self-start px-1.5 py-1" style={{ borderRadius: 2, backgroundImage: shown().button }}>
                  <Line color={shown().buttonText} width={16} height={2} />
                </View>
              </View>
            </View>
          </View>
        </View>
        <View class="flex-row items-center px-1.5" style={{ height: 9, backgroundImage: shown().status }}>
          <Line color={withAlpha(shown().statusText, 0.8)} width={26} height={2} />
        </View>
      </View>
    </View>
  );
}

/**
 * A theme's window in miniature, small enough for a menu's row: its title bar,
 * its sidebar with a row selected, a card on its page with two lines of text
 * and a primary button, and its status bar, in its own colors.
 */
function ThemeThumb(props: { value: ThemeName }) {
  const shown = () => swatch(props.value);
  return (
    <View style={{ width: 52, height: 34, borderRadius: 5, padding: 1, backgroundImage: shown().edge }}>
      <View class="flex-1 overflow-hidden" style={{ borderRadius: 4 }}>
        <View style={{ height: 5, backgroundImage: shown().title }} />
        <View class="flex-1 flex-row">
          <View class="gap-1 px-0.5 pt-1" style={{ width: 14, backgroundImage: shown().side }}>
            <View style={{ height: 3, borderRadius: 1, backgroundImage: shown().sideRow }} />
            <Line color={withAlpha(shown().sideText, 0.55)} width={8} height={2} />
          </View>
          <View class="flex-1 p-1" style={{ backgroundImage: shown().page }}>
            <View class="flex-1 gap-0.5 p-1" style={{ borderRadius: 2, backgroundImage: shown().card }}>
              <Line color={shown().text} width="80%" height={2} />
              <Line color={shown().muted} width="55%" height={2} />
              <View style={{ width: 9, height: 3, borderRadius: 1, backgroundImage: shown().button }} />
            </View>
          </View>
        </View>
        <View style={{ height: 3, backgroundImage: shown().status }} />
      </View>
    </View>
  );
}

/**
 * Room the theme menu leaves between its list and the window's top and bottom,
 * in points: the title bar's, the mode's above the list (its heading, its
 * switch and the line under it, about 80), the list's heading, and a margin.
 */
const MENU_ROOM = 200;

/** The heading over the theme menu's list: the appearance its themes are in, and with System, that the system decides it. */
function themesHeading(mode: Mode, scheme: Scheme): string {
  const named = scheme === "dark" ? "Dark themes" : "Light themes";
  return mode === "system" ? `${named}, while the system is ${scheme}` : named;
}

/**
 * The title bar's quick way to another theme, beside Settings' tiles: an icon
 * whose menu starts with the mode, Light, Dark or System, the same switch as
 * Settings' (`mode`, and `onMode`, the app's, which saves it), then lists the
 * themes of the appearance that mode shows (Default, then the theme files,
 * high contrast last), each beside its window in miniature, the one shown
 * checked. A mode picked there leaves the menu open, its list now the new
 * appearance's. A theme picked there is picked for its appearance alone, as
 * Settings picks it (`onPick`, the app's, which saves it), and closes the
 * menu; with the mode System, the other appearance keeps its own. The list
 * scrolls in a window too short for it.
 */
export function ThemeMenu(props: { mode: Mode; onMode: (mode: Mode) => void; onPick: (name: ThemeName) => void; class?: string }) {
  // The appearance shown, which the mode sets at once: its own with Light or Dark, the system's with System.
  const scheme = (): Scheme => colorScheme();
  const themes = () => THEMES.filter((each) => each.scheme === scheme());
  return (
    <PopupMenu trigger={<IconTrigger icon="palette" label="Theme" class={props.class} />}>
      <SectionLabel text="Mode" />
      {/* A row, so the switch keeps its own width rather than stretching its track across the menu. */}
      <View class="flex-row px-2 pb-1">
        <Segmented items={MODES} value={props.mode} onChange={props.onMode} />
      </View>
      <MenuLine />
      <SectionLabel text={themesHeading(props.mode, scheme())} />
      <Scroll style={{ maxHeight: Math.max(160, windowSize().height - MENU_ROOM) }}>
        <Each of={themes()} key={(each) => each.value}>
          {(each) => (
            <MenuRow
              tall
              label={each().title}
              leading={<ThemeThumb value={each().value} />}
              checked={theme() === each().value}
              onSelect={() => props.onPick(each().value)}
            />
          )}
        </Each>
      </Scroll>
    </PopupMenu>
  );
}

/**
 * A theme to pick, showing its own small window and its name. The one picked
 * for its appearance is ringed, with a check on its window's corner, in the
 * colors of the theme the app shows, as its primary button has them.
 */
function ThemeTile(props: { value: ThemeName; title: string; selected: boolean; onSelect: () => void }) {
  const primary = () => look().primary;
  return (
    <Press
      label={`${props.title} theme`}
      selected={props.selected}
      scale={0.98}
      class={`w-40 gap-1.5 rounded-xl p-1.5 hover:bg-accent ${props.selected ? look().rings.shown : look().rings.visible}`}
      onPress={props.onSelect}
    >
      <View>
        <ThemePreview value={props.value} />
        <If when={props.selected}>
          <View class="absolute bottom-1 right-1 size-5 items-center justify-center rounded-full bg-primary" style={primary().style}>
            <Icon name="check" size={12} tint="primary-foreground" />
          </View>
        </If>
      </View>
      <Text class="px-1 text-sm font-medium text-card-foreground" style={{ numberOfLines: 1 }}>
        {props.title}
      </Text>
    </Press>
  );
}

/** What the Appearance settings show, and what they report. */
export type AppearanceProps = {
  mode: Mode;
  onMode: (mode: Mode) => void;
  /** Picks `name` for its own appearance. */
  onTheme: (name: ThemeName) => void;
  /** Picks Default for the appearance the mode fixes, or for both with System, where the mode then decides which shows. */
  onDefault: () => void;
  onTexture: (name: TextureName) => void;
  onElevation: (name: ElevationName) => void;
};

/** A section of the Theme setting: its themes, under `title` (none for Default's), and whether its appearance is the one shown (`inUse`). */
type ThemeSection = { key: string; title?: string; inUse: boolean; themes: ThemeInfo[] };

/**
 * The sections of themes `mode` offers, with `shown` the appearance the app
 * shows now: Default first, alone and with no heading, then a section of theme
 * files for each appearance the mode shows, Dark then Light. Default is one
 * tile, the one the mode shows: its own appearance's, or with System the one
 * for the appearance shown. With System, `inUse` marks the appearance shown.
 */
function themeSections(mode: Mode, shown: Scheme): ThemeSection[] {
  const shows = (scheme: Scheme) => mode === "system" || mode === scheme;
  const own = defaultTheme(mode === "system" ? shown : mode);
  const defaults: ThemeSection = { key: "default", inUse: false, themes: THEMES.filter((each) => each.value === own) };
  const files = THEME_SECTIONS.filter((each) => shows(each.value)).map((each) => ({
    key: each.value,
    title: each.title,
    inUse: mode === "system" && shown === each.value,
    themes: THEMES.filter((theme) => theme.file && theme.scheme === each.value),
  }));
  return [defaults, ...files];
}

/**
 * A section of themes, a tile for each, under its title when it has one.
 * `inUse` marks the appearance shown. A theme file's tile picks it (`onPick`);
 * Default's picks Default for whatever the mode shows (`onDefault`).
 */
function ThemeGroup(props: { section: ThemeSection; onPick: (name: ThemeName) => void; onDefault: () => void }) {
  return (
    <View class="gap-2">
      <If when={props.section.title}>
        <View class="flex-row items-center gap-2">
          <Text role="header" class="text-sm font-medium text-card-foreground">
            {props.section.title}
          </Text>
          <If when={props.section.inUse}>
            <Text class="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">In use</Text>
          </If>
        </View>
      </If>
      <View class="flex-row flex-wrap gap-2">
        <Each of={props.section.themes} key={(each) => each.value}>
          {(each) => (
            <ThemeTile
              value={each().value}
              title={each().title}
              selected={picked[each().scheme]() === each().value}
              onSelect={() => (each().file ? props.onPick(each().value) : props.onDefault())}
            />
          )}
        </Each>
      </View>
    </View>
  );
}

/** The Appearance settings: the mode, the texture, the elevation and the themes, in one panel. */
export function AppearanceSettings(props: AppearanceProps) {
  const textured = () => TEXTURES.find((each) => each.value === texture())?.description ?? "";
  const contrast = () => highContrast(theme());
  /** What the elevation does, and that a high contrast theme keeps flat whatever it is. */
  const elevated = () => {
    const said = ELEVATIONS.find((each) => each.value === elevation())?.description ?? "";
    return contrast() ? `${said} High contrast themes stay flat: their outlines mark every state.` : said;
  };
  /** The name of the theme picked for `scheme`. */
  const pickedTitle = (scheme: Scheme) => THEMES.find((each) => each.value === picked[scheme]())?.title ?? "";
  /** The theme the mode shows, by name: one, or both with System, said once when both picks are Default. */
  const themed = () => {
    if (props.mode !== "system") return `${pickedTitle(props.mode)}, the ${props.mode} theme the app shows.`;
    const dark = pickedTitle("dark");
    const light = pickedTitle("light");
    return dark === light ? `${dark}, dark or light as the system is.` : `${dark} while the system is dark, ${light} while it is light.`;
  };
  return (
    <SettingsGroup title="Look" description="How the whole app looks. Changes show at once.">
      <SettingsPanel>
        <Appear index={0}>
          <Setting icon="sun" label="Mode" detail={MODE_DETAILS[props.mode]} control={<Segmented items={MODES} value={props.mode} onChange={props.onMode} />} />
        </Appear>
        <Appear index={1}>
          <Setting icon="grip" label="Texture" detail={textured()} control={<Segmented items={TEXTURES} value={texture()} onChange={props.onTexture} />} />
        </Appear>
        <Appear index={2}>
          <Setting icon="layers" label="Elevation" detail={elevated()} control={<Segmented items={ELEVATIONS} value={elevation()} onChange={props.onElevation} />} />
        </Appear>
        <Appear index={3}>
          {/* Default, then a section of theme files for each appearance the mode shows, each by name, then high contrast. */}
          <Setting icon="palette" label="Theme" detail={themed()}>
            <View class="gap-4">
              <Each of={themeSections(props.mode, colorScheme())} key={(each) => each.key}>
                {(each) => <ThemeGroup section={each()} onPick={props.onTheme} onDefault={props.onDefault} />}
              </Each>
            </View>
          </Setting>
        </Appear>
      </SettingsPanel>
    </SettingsGroup>
  );
}
