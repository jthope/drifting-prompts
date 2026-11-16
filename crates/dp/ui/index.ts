// The styled controls every product of the suite draws with. `nsh/base` brings
// the behavior, focus and accessibility; these bring the look, from the theme
// the app shows (`brand.tsx`), the elevation's depth over it (`elevation.ts`)
// and the texture over the page headers (`texture.ts`). Pages compose these
// rather than styling base components themselves, so a theme reaches every page
// of every product through them. An app imports them by path (`../../dp/ui`),
// and hands them its `Themes` module first (`connectThemes`).
export {
  applyThemeColors,
  BrandBadge,
  BrandCard,
  connectThemes,
  defaultTheme,
  EdgeLine,
  fadeFrom,
  type EdgePlace,
  highContrast,
  type Look,
  look,
  type NamedColor,
  OVERLAY,
  picked,
  type Rings,
  type Scheme,
  solid,
  type Swatch,
  swatch,
  THEME_SECTIONS,
  THEMES,
  type ThemeInfo,
  type ThemeName,
  theme,
  themeIn,
  themeScheme,
  type ThemesModule,
  withAlpha,
} from "./brand";
export { ELEVATIONS, type ElevationName, elevation, elevationNamed } from "./elevation";
export { EdgedScroll, pastBottom, pastTop, ScrollEdge } from "./edge";
export { Appear } from "./appear";
export { type AppearanceProps, AppearanceSettings, type KeptAppearance, keepAppearance, type Mode, type SavedAppearance, showAppearance, ThemeMenu } from "./appearance";
export { Breadcrumbs, type Crumb } from "./breadcrumbs";
export { Button, type ButtonKind, IconButton, IconTrigger } from "./button";
export { Card, cardClass, SHADOW_ROOM } from "./card";
export { CountBadge } from "./count";
export { type BarSeries, ChartCard, Donut, Heatmap, LegendItem, type Paint, REST, SERIES, type Slice, Sparkline, SplitBar, StackedBars, StatTile, WASHES } from "./chart";
export { CollapsingHeader, collapsedness, HEADER_BAR, HEADER_COLLAPSE } from "./collapsing";
export { askName, confirmDelete, NameDialog } from "./dialog";
export { EditorDialog, type EditorSection, type EditorSize } from "./editor";
export { EmptyState } from "./empty";
export { Frame, GRIP_HANDLE, ISLAND_GAP, Island, type IslandKind, Islands, LINE_HANDLE } from "./island";
export { CheckBox, CheckField, FieldRow, FormSection, InlineSelect, LevelSlider, SelectField, TextField } from "./field";
export { Icon, type IconName, type IconTint } from "./icon";
export { Logo } from "./logo";
export type { Product } from "./logos";
export { LiveDot, MediaTile, OverlayBar, OverlayChip, OverlaySegments, OverlaySeparator, OverlayToolbar, PictureNotice, RecMark, TILE_PAD, type TileSize } from "./media";
export { ChipTrigger, MenuChip, type MenuEntries, MenuLayer, MenuLine, MenuOpener, MenuRow, PopupMenu, popupMenu, popupSurface, SectionLabel } from "./menu";
export { NavFolder, NavRow, NavSection, type NavTab, NavTabs, SidebarFrame } from "./nav";
export { DirectionPad } from "./pad";
export { PaneHeader, PanelHeader } from "./panel";
export { Press } from "./press";
export { createReorder, type Reorder, type ReorderAxis, Reorderable } from "./reorder";
export { Setting, SettingRow, SettingsGroup, SettingsPanel } from "./rows";
export { SCROLLBARS } from "./scrollbar";
export { SearchField } from "./search";
export { Segmented } from "./segmented";
export { appearanceSection, SettingsDialog, type SettingsSection } from "./settings";
export { Spinner } from "./spinner";
export { Meter, StatusDot, StatusItem, StatusLine, StatusText, type StatusTone } from "./status";
export { ColumnTitle, GroupTitle, Pager, rowText, SelectionBar, type SortDirection, TABLE_ROW, TableFooter, TableRow, TitleRow } from "./table";
export { type FileTab, FileTabs, PageTabs } from "./tabs";
export { type Finish, headerTexture, TEXTURES, type TextureName, texture, textureNamed } from "./texture";
export { TitleBar } from "./titlebar";
export { type OutlineLine, OutlineList, TreeList, type TreePress } from "./tree";
export { isFullScreen, keepWindowPlace, leaveFullScreen, toggleFullScreen, type WindowPlace } from "./window";
export { CopyableNote } from "./note";
export { notify, Toaster } from "./toast";
