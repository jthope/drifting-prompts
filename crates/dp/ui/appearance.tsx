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

/** How the window looks, as an app saves it (`dp_ui::SavedAppearance`). */
export interface SavedAppearance {
  dark: boolean;
  followSystem: boolean;
  darkTheme: string;
  lightTheme: string;
  texture: string;
  elevation: string;
}

let colorsShown: ThemeName | undefined;

function showColors(name: ThemeName) {
  if (name === colorsShown) return;
  colorsShown = name;
  applyThemeColors(name).catch((error) => notify(`The theme's colors could not be shown: ${String(error)}`, "warn"));
}

/** Shows the appearance `saved` keeps; before the first frame, so nothing flashes. */
export function showAppearance(saved: SavedAppearance): void {
  picked.dark.set(themeIn(saved.darkTheme, "dark"));
  picked.light.set(themeIn(saved.lightTheme, "light"));
  setColorScheme(saved.followSystem ? "system" : saved.dark ? "dark" : "light");
  showColors(theme());
  texture.set(textureNamed(saved.texture));
  elevation.set(elevationNamed(saved.elevation));
}

/** The appearance as an app keeps it: the mode, the picks, and what to save. */
export interface KeptAppearance {
  mode: () => Mode;
  setMode: (mode: Mode) => void;
  /** Picks `name` for its own appearance. */
  pickTheme: (name: ThemeName) => void;
  /** Picks Default for the appearance the mode fixes, or for both with System. */
  pickDefault: () => void;
  setTexture: (name: TextureName) => void;
  setElevation: (name: ElevationName) => void;
  /** The appearance as it is now, to save. */
  saved: () => SavedAppearance;
}

/** Keeps the appearance an app shows, calling `save` after each pick. */
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

function Line(props: { color: string; width: number | `${number}%`; height?: number }) {
  return <View style={{ width: props.width, height: props.height ?? 3, borderRadius: 2, backgroundColor: props.color }} />;
}

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

/** Room left above and below the theme menu's list, in points. */
const MENU_ROOM = 200;

function themesHeading(mode: Mode, scheme: Scheme): string {
  const named = scheme === "dark" ? "Dark themes" : "Light themes";
  return mode === "system" ? `${named}, while the system is ${scheme}` : named;
}

/** The title bar's theme menu: the mode, then the themes of the appearance it shows. */
export function ThemeMenu(props: { mode: Mode; onMode: (mode: Mode) => void; onPick: (name: ThemeName) => void; class?: string }) {
  const scheme = (): Scheme => colorScheme();
  const themes = () => THEMES.filter((each) => each.scheme === scheme());
  return (
    <PopupMenu trigger={<IconTrigger icon="palette" label="Theme" class={props.class} />}>
      <SectionLabel text="Mode" />
      {/* A row, so the switch keeps its own width. */}
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
  /** Picks Default for the appearance the mode fixes, or for both with System. */
  onDefault: () => void;
  onTexture: (name: TextureName) => void;
  onElevation: (name: ElevationName) => void;
};

type ThemeSection = { key: string; title?: string; inUse: boolean; themes: ThemeInfo[] };

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
  const elevated = () => {
    const said = ELEVATIONS.find((each) => each.value === elevation())?.description ?? "";
    return contrast() ? `${said} High contrast themes stay flat: their outlines mark every state.` : said;
  };
  const pickedTitle = (scheme: Scheme) => THEMES.find((each) => each.value === picked[scheme]())?.title ?? "";
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
