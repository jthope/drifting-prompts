// The rows of the app's sidebar: pages and things to show, the headings over
// them, and the folders that fold them away. A selected row takes the theme's
// fill, and its icon the theme's ink. A row's `…` sits inside it at its right
// end, as shadcn's sidebar puts a menu item's action: over the row's button,
// which leaves it the room, rather than beside it. A dialog's sections down its
// side (`NavTabs`) are rows of the same look that are tabs.
import { type JSX, Text, View } from "nsh";
import { Button, Tab, Tabs } from "nsh/base";
import { look } from "./brand";
import { IconButton } from "./button";
import { Icon, type IconName } from "./icon";
import { Frame } from "./island";
import { Logo } from "./logo";
import type { Product } from "./logos";
import { type MenuEntries, MenuOpener, popupMenu } from "./menu";

/** A row's look, wherever it stands. */
const ROW_LOOK = "h-8 flex-row items-center gap-2 rounded-md hover:bg-sidebar-accent active:bg-sidebar-accent";
/** A row beside its `…`, taking the width it leaves. */
const ROW = `min-w-0 flex-1 ${ROW_LOOK}`;

/**
 * A row's `…`, inside its right end: 24 points in a 32-point row, 4 from its
 * edges. It lies over the row's button rather than in it, so pressing it opens
 * the menu and leaves the row as it is. It shows while the pointer is over the
 * row (`group/navrow`) or it has the keyboard focus, and always while `shown`.
 * The fade is on the button, not on `MenuOpener`, which holds a drawn menu on
 * the web: the menu must not fade with it as the pointer moves onto it.
 */
function RowMenu(props: { label: string; shown?: boolean; menu: () => MenuEntries }) {
  return (
    <MenuOpener class="absolute right-1 top-1">
      {(open) => (
        <View class={props.shown ? "" : "opacity-0 focus-within:opacity-100 group-hover/navrow:opacity-100"}>
          <IconButton icon="ellipsis" size="small" label={`${props.label} options`} onPress={() => open(props.menu())} />
        </View>
      )}
    </MenuOpener>
  );
}

/**
 * A row of the sidebar. `leading` takes the icon's place (a camera's status
 * dot) and `trailing` goes at its end (a recording's dot). `menu` is its
 * menu's items, made as it opens: at the pointer for the right button, and
 * from a `…` inside the row, which shows while the pointer is over the row and
 * always while it is selected, as a touch screen has no hover. The row keeps
 * the `…`'s room either way, so its label never runs under it. `grip` (a
 * `Reorderable`'s) sits in an indented row's indent, before its `leading`.
 */
export function NavRow(props: {
  label: string;
  icon?: IconName;
  leading?: JSX.Element;
  trailing?: JSX.Element;
  selected?: boolean;
  indent?: boolean;
  grip?: JSX.Element;
  menu?: () => MenuEntries;
  onPress?: () => void;
}) {
  return (
    <View class="group/navrow flex-row items-center" onContextMenu={(event) => props.menu && popupMenu(props.menu(), event.pagePosition)}>
      <Button
        selected={props.selected}
        accessibilityLabel={props.label}
        class={`${ROW} ${look().navOn} ${look().rings.nav} ${props.indent ? "pl-7" : "pl-2"} ${props.menu ? "pr-8" : "pr-2"}`}
        style={props.selected ? look().navLift : {}}
        onPress={props.onPress}
      >
        {/* Inside the button, so a press on the grip opens the row and only a drag moves it. */}
        <If when={props.grip}>{(grip) => <View class="absolute bottom-0 left-1 top-0 justify-center">{grip()}</View>}</If>
        <If when={props.icon}>{(icon) => <Icon name={icon()} tint="sidebar-foreground" color={props.selected ? look().navInk : undefined} />}</If>
        {props.leading}
        <Text class={`flex-1 text-sm text-sidebar-foreground ${look().navText ?? ""}`} style={{ numberOfLines: 1 }}>
          {props.label}
        </Text>
        {props.trailing}
      </Button>
      <If when={props.menu}>{(items) => <RowMenu label={props.label} shown={props.selected} menu={items()} />}</If>
    </View>
  );
}

/** One of `NavTabs`' rows: its value, its name and icon, whether to mark it (a section with a field to fix), and whether it can't be opened yet. */
export type NavTab<T extends string> = { value: T; title: string; icon: IconName; flagged?: boolean; disabled?: boolean };

/**
 * Rows in the sidebar's look that are tabs, one of them selected: a dialog's
 * sections down its side. Up and Down move between them, as a tab list's
 * arrows do. A flagged row ends in a dot in the theme's destructive color,
 * and a screen reader hears that it has a problem. A disabled row is dimmed,
 * takes no press, and the arrows pass it.
 */
export function NavTabs<T extends string>(props: { items: readonly NavTab<T>[]; value: T; onChange: (value: T) => void }) {
  return (
    <Tabs class="gap-0.5">
      <Each of={[...props.items]} key={(item) => item.value}>
        {(item) => {
          const on = () => props.value === item().value;
          return (
            <Tab
              selected={on()}
              disabled={item().disabled}
              testID={`section:${item().value}`}
              accessibilityLabel={item().flagged ? `${item().title}, has a problem` : item().title}
              class={`${ROW_LOOK} px-2 disabled:opacity-50 ${look().navOn} ${look().rings.nav}`}
              style={on() ? look().navLift : {}}
              onPress={() => props.onChange(item().value)}
            >
              <Icon name={item().icon} tint="sidebar-foreground" color={on() ? look().navInk : undefined} />
              <Text class={`flex-1 text-sm text-sidebar-foreground ${look().navText ?? ""}`} style={{ numberOfLines: 1 }}>
                {item().title}
              </Text>
              <If when={item().flagged}>
                <View class="size-1.5 rounded-full bg-destructive" />
              </If>
            </Tab>
          );
        }}
      </Each>
    </Tabs>
  );
}

/**
 * The sidebar: a column on the window's frame (`Frame`), the frame its panes'
 * islands sit on, topped by the left end of the title bar, in its color: the
 * product's logo and name (`product`, `name`), and the button that hides the
 * sidebar (`onHide`). The row drags the window, and its button stays
 * clickable. Its rows (`children`) go under it. A list in it that scrolls fades
 * into the frame's color (`EdgedScroll`'s `fade={look().tiers.frame}`).
 */
export function SidebarFrame(props: { product: Product; name: string; onHide: () => void; children?: JSX.Element }) {
  return (
    <Frame class="flex-1">
      <View class={`h-12 flex-row items-center gap-2 pl-2 pr-2 ${look().titleBar}`} desktop={{ windowDrag: true }}>
        <Logo product={props.product} />
        <Text class={`flex-1 text-sm font-semibold ${look().titleText ?? "text-sidebar-foreground"}`} style={{ numberOfLines: 1 }}>
          {props.name}
        </Text>
        <IconButton icon="panel-left" label="Hide sidebar" onPress={props.onHide} />
      </View>
      {props.children}
    </Frame>
  );
}

/** A heading over a part of the sidebar, with the buttons that add to it. */
export function NavSection(props: { title: string; children?: JSX.Element }) {
  return (
    <View class="h-8 flex-row items-center pl-2 pt-2">
      <Text class="flex-1 text-xs font-medium text-muted-foreground">{props.title}</Text>
      {props.children}
    </View>
  );
}

/** A heading that folds the rows under it away and back. `menu` is as a `NavRow`'s, its `…` always shown, inside the row. */
export function NavFolder(props: { label: string; open: boolean; icon: IconName; menu?: () => MenuEntries; onToggle: () => void }) {
  return (
    <View class="flex-row items-center" onContextMenu={(event) => props.menu && popupMenu(props.menu(), event.pagePosition)}>
      <Button accessibilityLabel={props.label} class={`${ROW} ${look().rings.nav} pl-1 ${props.menu ? "pr-8" : "pr-2"}`} onPress={props.onToggle}>
        <Icon name={props.open ? "chevron-down" : "chevron-right"} size={14} tint="muted-foreground" />
        <Icon name={props.icon} tint="sidebar-foreground" />
        <Text class="flex-1 text-sm text-sidebar-foreground" style={{ numberOfLines: 1 }}>
          {props.label}
        </Text>
      </Button>
      <If when={props.menu}>{(items) => <RowMenu label={props.label} shown menu={items()} />}</If>
    </View>
  );
}
