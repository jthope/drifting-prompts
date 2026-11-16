// The themes' looks: the default themes keep nsh's stock colors everywhere a
// theme reaches. No theme has depth of its own: the elevation adds shadows and
// lights over every theme, black and white at low alphas, never a color of its
// own. The high contrast themes take none.
//
// Colors are read as Rust reads them (`themeColors`), from the app's theme file
// and the files in `themes/`; a test's own engine has nsh's default theme file,
// so `token()` would not have the app's. `nsh check` still fails if
// `theme.json` stops defining a token that a class names. Rust's own tests
// check that every file sets every token.
import { expect, test } from "nsh/test";
import { type CanvasRenderingContext2D, setColorScheme } from "nsh";
import { Themes } from "ipcamera/native";
import { connectThemes, contrast, defaultTheme, fadeFrom, type Look, look, over, picked, THEMES, theme, themeColors, themeIn, themeScheme, withAlpha } from "../../dp/ui/brand";
import { ELEVATIONS, type ElevationName, elevationNamed } from "../../dp/ui/elevation";
import { type Finish, headerTexture, TEXTURES, textureNamed } from "../../dp/ui/texture";

// No app opens here, so the themes' module is handed over as `App.tsx` hands it.
connectThemes(Themes);

/** The tokens `classes` name for a color: `hover:bg-toolbar-hover` names `toolbar-hover`, `selected:bg-accent/25` names `accent`. */
function tokensIn(classes: string): string[] {
  const words = new Set(["sm", "xs", "base", "lg", "xl", "center", "right", "left", "white", "black", "transparent"]);
  return classes
    .split(/\s+/)
    .map((each) => /^(?:bg|text|border|ring)-([a-z][a-z0-9-]*?)(?:\/\d+)?$/.exec(each.slice(each.lastIndexOf(":") + 1))?.[1])
    .filter((name): name is string => name !== undefined && !words.has(name));
}

/** nsh's stock tokens, the only ones the default themes' classes name. */
const STOCK = ["background", "foreground", "card", "card-foreground", "popover", "popover-foreground", "primary", "primary-foreground", "secondary", "secondary-foreground", "muted", "muted-foreground", "accent", "accent-foreground", "destructive", "border", "input", "ring", "sidebar", "sidebar-foreground", "sidebar-accent", "sidebar-accent-foreground", "sidebar-border", "sidebar-ring"];

/** Every color in `paint` is black or white at an alpha: shade and light, never a color of its own. */
function expectBlackOrWhite(paint: string) {
  expect(paint).not.toContain("#");
  for (const color of paint.match(/rgb\([^)]*\)/g) ?? []) {
    expect(/^rgb\((0 0 0|255 255 255) \/ [\d.]+\)$/.test(color)).toBe(true);
  }
}

const LEVELS: ElevationName[] = ["none", "soft", "raised"];

/** The styles the elevation reaches in `shown`. */
const lifted = (shown: Look) => [
  shown.primary.style,
  shown.popup.style,
  shown.tooltip.style,
  shown.check.style,
  shown.segment,
  shown.surface,
  shown.control,
  shown.well,
  shown.onStyle,
  shown.navLift,
  shown.thumb,
];

/** Every paint and shadow the elevation reaches in `shown`, as text. */
const depthOf = (shown: Look) => [shown.glow, shown.cornerLight, shown.edge, shown.badge.fill, shown.badge.shadow, ...lifted(shown).flatMap((style) => Object.values(style).map(String))];

/** The classes of every place a look styles, joined. */
const classesOf = (shown: Look) =>
  [
    shown.header,
    shown.titleBar,
    shown.titleText ?? "",
    shown.primary.frame,
    shown.danger.frame,
    shown.danger.label,
    shown.buttons.secondary.frame,
    shown.buttons.ghost.frame,
    shown.on,
    shown.navOn,
    shown.navText ?? "",
    shown.listRow.hover,
    shown.listRow.selected,
    shown.listRow.selectedText ?? "",
    shown.chipOpen,
    shown.chip.frame,
    shown.chip.text,
    shown.popup.class,
    shown.menuItem.row,
    shown.menuItem.text,
    shown.menuItem.picture,
    shown.menuItem.option,
    shown.menuItem.line,
    shown.tooltip.class,
    shown.tooltip.text,
    shown.field.frame,
    shown.field.border,
    shown.check.box,
    shown.toggle.track,
    shown.toggle.frame,
    shown.toggle.on,
    shown.toggle.off,
    shown.tabs.indicator,
    shown.tabs.on,
    shown.tabs.off,
    shown.statusBar.frame,
    shown.statusBar.text,
    shown.slider?.thumb ?? "",
    ...Object.values(shown.rings),
    ...Object.values(shown.edges),
  ].join(" ");

/** The themes from `themes/`, as Settings offers them. */
const files = () => THEMES.filter((each) => each.file);

for (const scheme of ["dark", "light"] as const) {
  const name = `default-${scheme}`;

  test(`the default theme keeps the stock colors (${scheme})`, () => {
    setColorScheme(scheme);
    // Whatever the elevation, its depth is shade and light over the stock colors.
    for (const level of LEVELS) {
      const raised = look(name, level);
      for (const paint of [...depthOf(raised), raised.halo]) expectBlackOrWhite(paint);
    }
    const stock = look(name, "none");
    const classes = classesOf(stock);
    // Default keeps the stock tokens, but for the tab line and readable field outline it sets itself.
    for (const named of tokensIn(classes)) expect(STOCK.includes(named) || named === "tab-border" || named === "field-border").toBe(true);
    if (scheme === "dark") expect(themeColors(name)["tab-border"]).toMatch(/^#0078d4/);
    expect(stock.halo.startsWith("radial-gradient(circle, ")).toBe(true);
    // The headers share the sidebar's color.
    expect(stock.header).toBe("bg-sidebar");
    const border = themeColors(name).border;
    expect([0, 1, 2].map((index) => stock.accent(index))).toEqual([border, border, border]);
    expect(stock.slider).toBeUndefined();
    expect(stock.rings.visible).toBe("focus-visible:ring-2 focus-visible:ring-ring");
    expect(stock.primary.frame).toBe("bg-primary hover:opacity-80 active:opacity-70");
    // A hover shows on any surface: a tint of the text's color, not `accent`, which is `secondary`'s own in the dark.
    for (const kind of [stock.buttons.secondary, stock.buttons.ghost]) expect(kind.frame).toContain("hover:bg-foreground/10");
    expect(stock.popup.class).toBe("rounded-lg border border-border bg-popover");
  });

  test(`no theme has depth of its own: the elevation adds it, shadows then lights (${scheme})`, () => {
    // At None the default theme is nsh's flat look.
    const flat = look(name, "none");
    for (const none of [flat.glow, flat.cornerLight, flat.edge, flat.badge.fill, flat.badge.shadow]) expect(none).toBe("none");
    for (const style of lifted(flat)) expect(style).toEqual({});
    expect(flat.toggle.frame).not.toContain("shadow");

    // Soft casts shadows under what is raised, with no light and nothing pressed in.
    const soft = look(name, "soft");
    expect(soft.glow).not.toBe("none");
    for (const style of [soft.primary.style, soft.popup.style, soft.tooltip.style, soft.surface, soft.control, soft.navLift, soft.thumb]) {
      expect(style.boxShadow).toBeDefined();
      expect(String(style.boxShadow)).not.toContain("inset");
      expect(style.backgroundImage).toBeUndefined();
    }
    expect(soft.well).toEqual({});
    expect(soft.check.style).toEqual({});
    expect(soft.cornerLight).toBe("none");

    // Raised adds the light: a sheen on the primary button, a lit or shaded edge, and fields pressed in.
    const raised = look(name, "raised");
    expect(raised.primary.style.backgroundImage).toContain("linear-gradient");
    expect(String(raised.primary.style.boxShadow)).toContain("inset 0 1px 0");
    expect(raised.edge).toContain("linear-gradient");
    expect(String(raised.well.boxShadow)).toContain("inset");
    expect(raised.onStyle).toEqual(raised.well);
    expect(raised.badge.fill).toContain("linear-gradient");
  });
}

test("every theme file takes the elevation's depth over its own colors, and the high contrast ones stay flat", () => {
  for (const each of files()) {
    const flat = look(each.value, "none");
    expect(flat.glow).toBe("none");
    expect(flat.surface).toEqual({});
    if (each.contrast) {
      for (const level of LEVELS) expect(look(each.value, level)).toBe(flat);
      continue;
    }
    const soft = look(each.value, "soft");
    expect(soft.glow).not.toBe("none");
    expect(soft.surface.boxShadow).toBeDefined();
    expect(soft.popup.style.boxShadow).toBeDefined();
    const raised = look(each.value, "raised");
    if (raised.primary.style.backgroundImage) expect(raised.primary.style.backgroundImage).toContain("linear-gradient");
    // The light goes over the theme's own color, which still shows under it.
    expect(raised.cornerLight.endsWith(flat.cornerLight)).toBe(true);
    expect(raised.edge.endsWith(flat.edge)).toBe(true);
    // A theme's own outline stays, under the elevation's shadows.
    if (flat.onStyle.boxShadow) expect(String(raised.onStyle.boxShadow).startsWith(String(flat.onStyle.boxShadow))).toBe(true);
    // What the elevation adds is shade and light, never a color of its own.
    const added = (paint: string, own: string) => (paint.endsWith(own) ? paint.slice(0, paint.length - own.length) : paint);
    expectBlackOrWhite(added(raised.cornerLight, flat.cornerLight));
    expectBlackOrWhite(added(raised.glow, flat.glow));
  }
});

test("content fades into its own surface at either edge, the same both ways but for direction, in every theme", () => {
  for (const scheme of ["dark", "light"] as const) {
    setColorScheme(scheme);
    const sidebar = themeColors(theme()).sidebar;
    const top = fadeFrom("sidebar", "top");
    // Solid in the sidebar's color along the edge, so nothing shows through there, and nothing of it at the far end.
    expect(top).toBe(`linear-gradient(180deg, ${sidebar}, ${sidebar} 16%, ${withAlpha(sidebar, 0.7)} 40%, ${withAlpha(sidebar, 0)})`);
    expect(fadeFrom("sidebar", "bottom")).toBe(top.replace("180deg", "0deg"));
  }
});

test("content fades into a color of the window's surfaces as into a token's", () => {
  const frame = look().tiers.frame;
  expect(fadeFrom(frame, "top")).toBe(`linear-gradient(180deg, ${frame}, ${frame} 16%, ${withAlpha(frame, 0.7)} 40%, ${withAlpha(frame, 0)})`);
});

test("the panes' islands stand in three steps in every theme, a tool pane's between the frame's and the document's; the high contrast themes keep their own", () => {
  for (const each of THEMES) {
    const { frame, tool, document } = look(each.value, "none").tiers;
    for (const color of [frame, tool, document]) expect(color).toMatch(/^#[0-9a-f]{6}$/);
    const colors = themeColors(each.value);
    const page = colors.background.slice(0, 7);
    if (each.contrast) {
      expect([frame, tool, document]).toEqual([over(colors.sidebar, page), page, page]);
      continue;
    }
    // A theme file keeps its page for the document; Default keeps its frame, one color with its title bar.
    if (each.file) expect(document).toBe(page);
    else if (each.scheme === "dark") expect(frame).toBe(over(colors.sidebar, page));
    // The frame and the document stand the set contrast apart, each step shows against the next, and the tool
    // pane's lies between the other two.
    expect(contrast(frame, document)).toBeGreaterThan(each.scheme === "dark" ? 1.299 : 1.179);
    expect(contrast(frame, tool)).toBeGreaterThan(1.08);
    expect(contrast(tool, document)).toBeGreaterThan(1.08);
    expect(contrast(frame, document)).toBeGreaterThan(Math.max(contrast(frame, tool), contrast(tool, document)));
  }
});

test("a scroll view's edges cast the elevation's shade, the same both ways but for direction, none at None or in the high contrast themes", () => {
  for (const each of THEMES) {
    for (const side of ["top", "bottom"] as const) expect(look(each.value, "none").scrollShade(side)).toBe("none");
    for (const level of ["soft", "raised"] as const) {
      const shade = look(each.value, level).scrollShade;
      if (each.contrast) {
        expect([shade("top"), shade("bottom")]).toEqual(["none", "none"]);
        continue;
      }
      // Down from a top edge, up from a bottom one, from the same stops.
      expect(shade("top")).toContain("linear-gradient(180deg, ");
      expect(shade("bottom")).toBe(shade("top").replace("180deg", "0deg"));
      expectBlackOrWhite(shade("top"));
      // Darker along a dark page's edge, where a shade shows less.
      expect(shade("top")).toContain(themeScheme(each.value) === "dark" ? "rgb(0 0 0 / 0.4)" : "rgb(0 0 0 / 0.08)");
    }
  }
});

test("a light falls on each place by how light its own color is", () => {
  // The default's primary button is pale in the dark and near black in the light: more white shows a sheen on pale.
  expect(look("default-dark", "raised").primary.style.backgroundImage).toContain("rgb(255 255 255 / 0.5)");
  expect(look("default-light", "raised").primary.style.backgroundImage).toContain("rgb(255 255 255 / 0.14)");
  // Dark+'s blue button has a softer sheen so its white text stays readable.
  expect(look("dark-plus", "raised").primary.style.backgroundImage).toContain("linear-gradient");
  expect(look("dark-plus", "raised").primary.style.backgroundImage).not.toContain("rgb(255 255 255 / 0.14)");
  // On a light theme's white card, the bottom falls into shade.
  expect(look("light-modern", "raised").edge).toContain("rgb(0 0 0 / 0.07)");
  expect(look("dark-modern", "raised").edge).toContain("rgb(255 255 255 / 0.12)");
});

test("each appearance has its own themes, Default first and high contrast last", () => {
  for (const scheme of ["dark", "light"] as const) {
    const own = THEMES.filter((each) => each.scheme === scheme);
    // Saved settings hold Default by these names, so they stay.
    expect(defaultTheme(scheme)).toBe(`default-${scheme}`);
    expect(own[0].value).toBe(defaultTheme(scheme));
    expect(own[0].title).toBe("Default");
    expect(own[0].file).toBe(false);
    expect(own[own.length - 1].contrast).toBe(true);
    for (const each of own) expect(themeScheme(each.value)).toBe(scheme);
  }
  // The dark ones, then the light ones.
  const order = THEMES.map((each) => each.scheme);
  expect(order).toEqual([...order].sort());
});

test("the theme shown is the one picked for the appearance shown", () => {
  const before = { dark: picked.dark(), light: picked.light() };
  picked.dark.set("monokai");
  picked.light.set("solarized-light");
  setColorScheme("dark");
  expect(theme()).toBe("monokai");
  setColorScheme("light");
  expect(theme()).toBe("solarized-light");
  picked.dark.set(before.dark);
  picked.light.set(before.light);
  setColorScheme("dark");
});

test("a name this version doesn't know, or one of the other appearance, picks Default", () => {
  expect(themeIn("monokai", "dark")).toBe("monokai");
  expect(themeIn("monokai", "light")).toBe("default-light");
  expect(themeIn("solarized-light", "light")).toBe("solarized-light");
  expect(themeIn("sunset", "dark")).toBe("default-dark");
  expect(themeIn("", "light")).toBe("default-light");
  expect(themeIn("neon", "light")).toBe("default-light");
});

test("every theme file is offered, each under a name of its own, and Default is one name in both appearances", () => {
  expect(files().map((each) => each.value).sort()).toEqual([...Themes.names()].sort());
  // Default is one theme to the user, dark or light as the mode is, so neither of its titles says which.
  expect(THEMES.filter((each) => !each.file).map((each) => each.title)).toEqual(["Default", "Default"]);
  const titles = files().map((each) => each.title);
  // No two theme files share a name, and none takes Default's or the mode's.
  expect(new Set(titles).size).toBe(titles.length);
  for (const title of titles) expect(["Default", "Light", "Dark", "System"].includes(title)).toBe(false);
  for (const name of ["Dark Modern", "Dark+", "Monokai", "Light Modern", "Solarized Light", "Dark High Contrast"]) {
    expect(titles.includes(name)).toBe(true);
  }
});

test("a theme file's look names only tokens the file sets", () => {
  for (const each of files()) {
    const colors = themeColors(each.value);
    // The page is opaque.
    expect(colors.background).toMatch(/^#[0-9a-f]{6}$/);
    const shown = look(each.value, "none");
    const classes = [classesOf(shown), shown.menuItem.picture, shown.menuItem.line].join(" ");
    for (const named of tokensIn(classes)) expect(colors[named]).toMatch(/^#[0-9a-f]{6}([0-9a-f]{2})?$/);
    expect(colors[shown.buttons.ghost.tint]).toMatch(/^#/);
  }
});

test("the high contrast themes outline what is hovered, and the others don't", () => {
  for (const each of files()) {
    const outline = themeColors(each.value)["state-outline"];
    if (each.contrast) expect(outline).toMatch(/^#[0-9a-f]{6}$/);
    else expect(outline).toMatch(/^#[0-9a-f]{6}00$/);
  }
});

test("a theme file's colors are its own, over the app's", () => {
  const plus = themeColors("dark-plus");
  expect(plus.background).toBe("#1e1e1e");
  expect(plus.sidebar).toBe("#252526");
  expect(plus["title-bar"]).toBe("#3c3c3c");
  expect(plus["status-bar"]).toBe("#007acc");
  expect(plus["primary-hover"]).toBe("#1177bb");
});

test("a color over another shows as browsers blend them", () => {
  expect(over("#ffffff80", "#000000")).toBe("#808080");
  expect(over("#123456", "#ffffff")).toBe("#123456");
  expect(over("#00000000", "#abcdef")).toBe("#abcdef");
});

test("the shared controls and Markdown colors stay readable in every theme", () => {
  const failures: string[] = [];
  for (const each of THEMES) {
    const colors = themeColors(each.value);
    const shown = look(each.value, "none");
    const page = colors.background.slice(0, 7);
    const card = over(colors.card, page);
    const popup = over(colors.popover, page);
    const title = over(colors["title-bar"], page);
    const inputAlpha = colors.input.length === 9 ? parseInt(colors.input.slice(7, 9), 16) / 255 : 1;
    const field = each.file ? over(colors.field, popup) : each.scheme === "dark" ? over(withAlpha(colors.input, 0.3 * inputAlpha), popup) : popup;
    const fieldOnCard = over(colors.field, card);
    const menu = over(colors["menu-selection"], popup);
    const selected = over(colors["nav-selection"], shown.tiers.frame);
    const check = (name: string, foreground: string, background: string, minimum: number) => {
      const ratio = contrast(over(foreground, background), background);
      if (ratio < minimum) failures.push(`${each.value}: ${name} ${foreground} on ${background} ${ratio.toFixed(2)}:1`);
    };
    const liftedSurfaces = (background: string, paint: unknown) =>
      [background, ...(String(paint ?? "").match(/rgb\((?:0 0 0|255 255 255) \/ [\d.]+\)/g) ?? []).map((layer) => {
        const parts = /rgb\((0 0 0|255 255 255) \/ ([\d.]+)\)/.exec(layer);
        const color = parts?.[1] === "0 0 0" ? "#000000" : "#ffffff";
        return over(withAlpha(color, Number(parts?.[2])), background);
      })];

    for (const surface of [page, popup]) check("primary button", colors["primary-foreground"], over(colors.primary, surface), 4.5);
    const raisedLook = look(each.value, "raised");
    const light = String(raisedLook.primary.style.backgroundImage ?? "").match(/rgb\(255 255 255 \/ ([\d.]+)\)/);
    if (light) {
      for (const surface of [page, popup]) {
        for (const fill of each.file ? [colors.primary, colors["primary-hover"]] : [colors.primary]) {
          const raised = over(withAlpha("#ffffff", Number(light[1])), over(fill, surface));
          check("raised primary button", colors["primary-foreground"], raised, 4.5);
        }
      }
    }
    if (!each.file) {
      for (const surface of [page, card, popup]) {
        const fill = over(colors.primary, surface);
        const raised = light ? over(withAlpha("#ffffff", Number(light[1])), fill) : fill;
        for (const opacity of [0.8, 0.7]) {
          for (const background of [fill, raised]) {
            const fadedFill = over(withAlpha(background, opacity), surface);
            const fadedInk = over(withAlpha(colors["primary-foreground"], opacity), surface);
            check("primary button hovered or pressed", fadedInk, fadedFill, 4.5);
          }
        }
      }
    }
    if (each.file) {
      for (const surface of [page, popup]) check("primary button hovered", colors["primary-foreground"], over(colors["primary-hover"], surface), 4.5);
      for (const surface of [page, card, popup]) {
        check("secondary button", colors["secondary-foreground"], over(colors.secondary, surface), 4.5);
        check("secondary button hovered", colors["secondary-foreground"], over(colors["secondary-hover"], surface), 4.5);
        check("menu trigger", colors["dropdown-foreground"], over(colors.dropdown, surface), 4.5);
      }
      for (const surface of [field, fieldOnCard]) {
        check("field value", colors["field-foreground"], surface, 4.5);
        check("field placeholder", colors["field-placeholder"], surface, 4.5);
        const segment = over(colors.option, surface);
        for (const fill of liftedSurfaces(segment, raisedLook.segment.backgroundImage)) check("selected segment", colors["option-foreground"], fill, 4.5);
        check("segment hovered", colors["field-foreground"], over(colors["option-hover"], surface), 4.5);
      }
      check("field outline", colors["field-border"], field, 3);
      check("field outline on dialog", colors["field-border"], popup, 3);
      check("field focus", colors.ring, field, 3);
      check("field focus on dialog", colors.ring, popup, 3);
      check("sidebar focus", colors["sidebar-ring"], shown.tiers.frame, 3);
      check("sidebar focus on selection", colors["sidebar-ring"], selected, 3);
      check("menu selection", colors["menu-selection-foreground"], menu, 4.5);
      check("navigation selection", colors["nav-selection-foreground"], selected, 4.5);
      check("inactive tab", colors["tab-inactive"], shown.tiers.tool, 4.5);
      check("status bar", colors["status-bar-foreground"], over(colors["status-bar"], page), 4.5);
    } else {
      check("field outline", colors["field-border"], field, 3);
      check("field outline on dialog", colors["field-border"], popup, 3);
      check("field focus", colors.ring, field, 3);
      check("field focus on dialog", colors.ring, popup, 3);
      check("sidebar focus", colors["sidebar-ring"], shown.tiers.frame, 3);
      const segment = each.scheme === "dark" ? page : card;
      for (const fill of liftedSurfaces(segment, raisedLook.segment.backgroundImage)) check("selected segment", colors.foreground, fill, 4.5);
    }
    for (const surface of [page, card]) check("focus", colors.ring, surface, 3);
    const tooltip = over(each.file ? colors.tooltip : colors.primary, popup);
    const tooltipInk = each.file ? colors["tooltip-foreground"] : colors["primary-foreground"];
    for (const surface of liftedSurfaces(tooltip, raisedLook.tooltip.style.backgroundImage)) check("tooltip", tooltipInk, surface, 4.5);

    for (const surface of [title, card]) {
      check("live status", colors.success, surface, 4.5);
      check("warning status", colors.warning, surface, 4.5);
    }
    for (const surface of [popup, card]) check("error text", colors.destructive, surface, 4.5);
    for (const surface of [page, card, shown.tiers.frame, over(colors.muted, page)]) check("muted text", colors["muted-foreground"], surface, 4.5);
    for (const surface of liftedSurfaces(card, raisedLook.cornerLight)) {
      check("raised live status", colors.success, surface, 4.5);
      check("raised warning status", colors.warning, surface, 4.5);
      check("raised error text", colors.destructive, surface, 4.5);
      check("raised muted text", colors["muted-foreground"], surface, 4.5);
    }
    for (const surface of liftedSurfaces(popup, raisedLook.popup.style.backgroundImage)) {
      check("raised dialog error", colors.destructive, surface, 4.5);
      const raisedField = each.file ? over(colors.field, surface) : each.scheme === "dark" ? over(withAlpha(colors.input, 0.3 * inputAlpha), surface) : surface;
      check("raised field outline", colors["field-border"], raisedField, 3);
      check("raised field outline on dialog", colors["field-border"], surface, 3);
      check("raised field focus", colors.ring, raisedField, 3);
      check("raised field focus on dialog", colors.ring, surface, 3);
    }

    const dangerInk = shown.danger.label === "text-black" ? "#000000" : "#ffffff";
    for (const surface of [page, card, popup]) check("danger button", dangerInk, over(colors.destructive, surface), 4.5);
    expect(shown.danger.ink).toBe(dangerInk === "#000000" ? "rgb(0 0 0 / 1)" : "rgb(255 255 255 / 1)");
    expect(shown.danger.frame).not.toContain("opacity");
    for (const part of ["heading", "link", "code", "marker", "punctuation", "strong", "emphasis", "quote"]) {
      const ink = colors[`syntax-${part}`];
      const background = part === "code" ? over(withAlpha(ink, 0.1), shown.tiers.document) : shown.tiers.document;
      check(`Markdown ${part}`, ink, background, 4.5);
    }
    const mutedAlpha = colors.muted.length === 9 ? parseInt(colors.muted.slice(7, 9), 16) / 255 : 1;
    const frontMatter = over(withAlpha(colors.muted, 0.3 * mutedAlpha), shown.tiers.document);
    check("Markdown front matter", colors["syntax-punctuation"], frontMatter, 4.5);
  }
  expect(failures).toEqual([]);
});

/** What the dots drew on a canvas: each dot's center, and the colors of the fill they share. */
function drawnDots(finish: Finish, width: number, height: number) {
  const centers: [number, number][] = [];
  const colors: string[] = [];
  let fills = 0;
  const cx = {
    beginPath: () => {},
    moveTo: () => {},
    arc: (x: number, y: number) => centers.push([x, y]),
    createRadialGradient: () => ({ addColorStop: (_at: number, color: string) => colors.push(color) }),
    fill: () => fills++,
    fillStyle: undefined as unknown,
  };
  finish.dots?.(cx as unknown as CanvasRenderingContext2D, { width, height });
  return { centers, colors, fills };
}

for (const scheme of ["dark", "light"] as const) {
  test(`a texture is shade and light over any theme, and only Dots has dots (${scheme})`, () => {
    setColorScheme(scheme);
    expect(headerTexture(168, "none")).toBeUndefined();
    for (const name of ["haze", "dots"] as const) {
      const light = headerTexture(168, name)?.light ?? "";
      // Two pools, each fading to nothing, so the theme shows through everywhere else.
      expect(light.match(/radial-gradient\(circle at /g)?.length).toBe(2);
      expect(light.match(/\/ 0\) [\d.]+px\)/g)?.length).toBe(2);
      expectBlackOrWhite(light);
      // White in the dark, black in the light.
      for (const color of light.match(/rgb\([^)]*\)/g) ?? []) expect(color.startsWith(scheme === "dark" ? "rgb(255 255 255" : "rgb(0 0 0")).toBe(true);
    }
    expect(headerTexture(168, "haze")?.dots).toBeUndefined();

    const dots = drawnDots(headerTexture(168, "dots") as Finish, 1000, 168);
    // One fill for every dot, in the middle of its 14-point cell, and only where the light falls.
    expect(dots.fills).toBe(1);
    expect(dots.centers.length).toBeGreaterThan(20);
    expect(dots.centers.length).toBeLessThan((1000 / 14) * (168 / 14) * 0.5);
    for (const [x, y] of dots.centers) expect([x % 14, y % 14]).toEqual([7, 7]);
    expect(dots.centers.every(([x]) => x > 500)).toBe(true);
    for (const color of dots.colors) expectBlackOrWhite(color);
    expect(dots.colors[dots.colors.length - 1]).toMatch(/\/ 0\)$/);
  });
}

test("the textures go from none to the most, and one this version doesn't know is None", () => {
  expect(TEXTURES.map((each) => each.title)).toEqual(["None", "Haze", "Dots"]);
  expect(textureNamed("dots")).toBe("dots");
  expect(textureNamed("haze")).toBe("haze");
  expect(textureNamed("plaid")).toBe("none");
  expect(textureNamed("")).toBe("none");
});

test("the elevations go from none to the most, and one this version doesn't know is None", () => {
  expect(ELEVATIONS.map((each) => each.title)).toEqual(["None", "Soft", "Raised"]);
  expect(elevationNamed("none")).toBe("none");
  expect(elevationNamed("soft")).toBe("soft");
  expect(elevationNamed("raised")).toBe("raised");
  expect(elevationNamed("towering")).toBe("none");
  expect(elevationNamed("")).toBe("none");
});

test("an alpha replaces the color's own", () => {
  expect(withAlpha("#01ccfbff", 0.5)).toBe("#01ccfb80");
  expect(withAlpha("#01ccfb", 0)).toBe("#01ccfb00");
  expect(withAlpha("#01ccfb", 2)).toBe("#01ccfbff");
});

test("the Default themes chart in five colors, the brand's violet first, each its own in either appearance", () => {
  for (const scheme of ["dark", "light"] as const) {
    const colors = themeColors(defaultTheme(scheme));
    const charts = [1, 2, 3, 4, 5].map((number) => colors[`chart-${number}`].slice(0, 7));
    expect(new Set(charts).size).toBe(5);
    // Not nsh's stock grays: each has a hue, its red, green and blue apart.
    for (const each of charts) {
      const channels = [1, 3, 5].map((at) => parseInt(each.slice(at, at + 2), 16));
      expect(Math.max(...channels) - Math.min(...channels)).toBeGreaterThan(60);
    }
    // Violet leads: more blue than red, and more red than green.
    const [r, g, b] = [1, 3, 5].map((at) => parseInt(charts[0].slice(at, at + 2), 16));
    expect(b > r && r > g).toBe(true);
  }
});

test("every theme's dashboard chips read: one ink for all, standing out from each chart color by at least 3:1", () => {
  // The tiles take the first four chart colors.
  for (const each of THEMES) {
    const colors = themeColors(each.value);
    const ink = look(each.value, "none").chartInk;
    for (const number of [1, 2, 3, 4]) expect(contrast(ink, colors[`chart-${number}`].slice(0, 7))).toBeGreaterThan(3);
  }
});
