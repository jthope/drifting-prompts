import { effect, type JSX, signal, Text, untracked, View, type ViewStyle, windowSize } from "nsh";
import { Menu, MenuItem, MenuSeparator, Submenu } from "nsh/base";
import { isAvailable as hasSystemMenus, type MenuItem as SystemMenuItem, Menu as SystemMenu } from "nsh/desktop";
import { look } from "./brand";
import { Icon, type IconName } from "./icon";

type Point = { x: number; y: number };

/** A menu's items, as the system's menus take them: `false`, `null` and `undefined` leave an item out (`cond && { … }`). */
export type MenuEntries = readonly (SystemMenuItem | false | null | undefined)[];

/**
 * The surface of what floats over the window, as the theme draws it: a menu's
 * or a list's popup, a toast, a dialog's panel. `extra` adds classes (padding, width).
 */
export function popupSurface(extra = ""): { class: string; style: ViewStyle } {
  const popup = look().popup;
  return { class: `${popup.class} ${extra}`, style: popup.style };
}

/**
 * A popup menu that opens from `trigger`, which must be a plain view with
 * `role="button"` (a `ChipTrigger` with `menu`): a menu gives its trigger its
 * presses, focus and open state, so a `Button` there would take the press for itself.
 */
export function PopupMenu(props: { trigger: JSX.Element; placement?: "top" | "bottom"; children: JSX.Element }) {
  const surface = () => popupSurface("min-w-48 p-1");
  return (
    <Menu offset={4} placement={props.placement} popupClass={surface().class} popupStyle={surface().style} trigger={props.trigger}>
      {props.children}
    </Menu>
  );
}

/**
 * The button of a menu or a list of choices, showing the current choice
 * (`children`) with a chevron to say it opens. `menu` makes it a `PopupMenu`'s
 * trigger, which takes a plain view with the button role; a `Select` gives its
 * own. Its outline turns the theme's color while it is open.
 */
export function ChipTrigger(props: { menu?: boolean; accessibilityLabel?: string; class?: string; children: JSX.Element }) {
  return (
    <View
      role={props.menu ? "button" : undefined}
      accessibilityLabel={props.accessibilityLabel}
      class={`h-8 flex-row items-center gap-1.5 rounded-md border pl-2.5 pr-2 ${look().chip.frame} ${look().chipOpen} ${look().rings.visible} ${props.class ?? ""}`}
      style={look().control}
    >
      {props.children}
      <Icon name="chevron-down" size={14} tint="muted-foreground" />
    </View>
  );
}

/**
 * A `PopupMenu` trigger that shows the current choice beside an icon. `label`
 * is what it is set to; `accessibilityLabel` says what it chooses, for a screen reader.
 */
export function MenuChip(props: { icon: IconName; label: string; accessibilityLabel: string }) {
  return (
    <ChipTrigger menu accessibilityLabel={props.accessibilityLabel}>
      <Icon name={props.icon} size={14} tint="foreground" />
      <Text class={`text-sm font-medium ${look().chip.text}`} style={{ numberOfLines: 1 }}>
        {props.label}
      </Text>
    </ChipTrigger>
  );
}

/**
 * A menu item's icon. Where the theme recolors an item's text while it is
 * hovered or focused (`Look.menuItem.activeInk`, a theme file's `menu-selection`),
 * a second icon in that color shows over the first then, as an icon is drawn
 * in one color and can't follow a class. The item must have the `group` class.
 */
function MenuIcon(props: { name: IconName }) {
  return (
    <If when={look().menuItem.activeInk} else={<Icon name={props.name} size={14} tint="popover-foreground" />}>
      {(active) => (
        <View class="size-3.5">
          <View class="absolute inset-0 group-hover:opacity-0 group-focus:opacity-0">
            <Icon name={props.name} size={14} tint="popover-foreground" />
          </View>
          <View class="absolute inset-0 opacity-0 group-hover:opacity-100 group-focus:opacity-100">
            <Icon name={props.name} size={14} color={active()} />
          </View>
        </View>
      )}
    </If>
  );
}

/**
 * An item of a `PopupMenu`. `checked` makes it a setting the menu shows the
 * state of, its check in the theme's ink. `leading` takes the icon's place (a
 * small picture of a layout, say), and `tall` makes room for a larger one (a
 * theme's window). Hovered or focused, it takes the theme's fill, and its text
 * and icons the color the theme gives them on it.
 */
export function MenuRow(props: { label: string; icon?: IconName; leading?: JSX.Element; tall?: boolean; checked?: boolean; disabled?: boolean; onSelect: () => void }) {
  return (
    <MenuItem
      role={props.checked === undefined ? undefined : "menuitemradio"}
      checked={props.checked}
      disabled={props.disabled}
      onSelect={props.onSelect}
      class={`${props.tall ? "h-11" : "h-8"} flex-row items-center gap-2 rounded-md px-2 disabled:opacity-50 ${look().menuItem.row}`}
      indicatorClass="w-4 items-center opacity-0 checked:opacity-100"
      indicator={<MenuIcon name="check" />}
    >
      <If when={props.icon}>{(icon) => <MenuIcon name={icon()} />}</If>
      {props.leading}
      <Text class={`text-sm ${look().menuItem.text}`}>{props.label}</Text>
    </MenuItem>
  );
}

/** A line between groups of a `PopupMenu`. */
export function MenuLine() {
  return <MenuSeparator class={`my-1 h-px ${look().menuItem.line}`} />;
}

// The menus of buttons and right clicks. On the desktop they are the system's own (`nsh/desktop`'s
// `Menu.popup`). nsh 0.5.1 has none in a browser, where `Menu.popup` does nothing, so there the
// same items are drawn with `nsh/base`'s `Menu`, as `PopupMenu`'s are. A release with browser menus
// in `Menu.popup` replaces `MenuOpener`'s and `popupMenu`'s drawn half and `MenuLayer`.

/** The row of a drawn menu that opens a submenu, or, with `back`, returns from one on a touch screen: a plain view, which `Submenu` makes the item. */
function SubmenuRow(props: { label: string; back?: boolean }) {
  return (
    <View class={`h-8 flex-row items-center gap-2 rounded-md px-2 ${look().menuItem.row}`}>
      <View class="w-4 items-center">
        <If when={props.back}>
          <MenuIcon name="chevron-left" />
        </If>
      </View>
      <Text class={`flex-1 text-sm ${look().menuItem.text}`}>{props.label}</Text>
      <If when={!props.back}>
        <MenuIcon name="chevron-right" />
      </If>
    </View>
  );
}

/** One of a drawn menu's items: a line, a submenu cascading beside it, or a row. */
function DrawnItem(props: { item: SystemMenuItem }) {
  const item = props.item;
  if ("type" in item) return <MenuLine />;
  const label = item.label ?? "";
  if (item.submenu) {
    const items = item.submenu;
    return (
      <Submenu offset={4} disabled={item.disabled} trigger={<SubmenuRow label={label} />} back={<SubmenuRow label={label} back />}>
        <DrawnItems items={items} />
      </Submenu>
    );
  }
  return <MenuRow label={label} checked={item.checked} disabled={item.disabled} onSelect={() => item.onSelect?.()} />;
}

/** `items` as a drawn menu's items. */
function DrawnItems(props: { items: MenuEntries }) {
  return <Each of={props.items.filter((item): item is SystemMenuItem => Boolean(item))}>{(item) => <DrawnItem item={item()} />}</Each>;
}

/** A drawn menu's items at the top of it, where a test finds them as `id:menu`. */
function DrawnMenu(props: { items: MenuEntries }) {
  return (
    <View testID="menu">
      <DrawnItems items={props.items} />
    </View>
  );
}

/**
 * A button that opens a menu. `children` is the button, given `open`, which
 * its press calls with the menu's items. On the desktop that is the system's
 * own menu, at the pointer; in a browser, a drawn one under the button. `class`
 * places the button and its menu together, as the button's own would place it.
 */
export function MenuOpener(props: { class?: string; children: (open: (items: MenuEntries) => void) => JSX.Element }) {
  if (hasSystemMenus) return <View class={props.class}>{props.children((items) => void SystemMenu.popup(items))}</View>;
  const shown = signal<MenuEntries | null>(null);
  const surface = () => popupSurface("min-w-48 p-1");
  return (
    <Menu
      class={props.class}
      open={shown() !== null}
      offset={4}
      popupClass={surface().class}
      popupStyle={surface().style}
      onOpenChange={(event) => {
        if (!event.open) shown.set(null);
      }}
      // Only where the menu stands: the button inside keeps its press for itself, and opens it.
      trigger={<View>{props.children((items) => shown.set(items))}</View>}
    >
      <DrawnMenu items={shown() ?? []} />
    </Menu>
  );
}

/** The menu `popupMenu` drew, and where. */
type Pointed = { items: MenuEntries; at: Point };

const pointed = signal<Pointed | null>(null);

/** The `MenuLayer`s shown, the newest last, which `popupMenu`'s menus open in. */
const layers = signal<symbol[]>([]);

/**
 * Opens a menu of `items` with its corner at `at`: an event's `pagePosition`,
 * in the window's coordinates or those of the dialog it happened in. Without
 * `at` the system's opens at the pointer; a drawn one, in a browser, at the
 * top of the middle, as every caller there says where.
 */
export function popupMenu(items: MenuEntries, at?: Point) {
  if (hasSystemMenus) {
    void SystemMenu.popup(items, at);
    return;
  }
  const size = windowSize();
  pointed.set({ items, at: at ?? { x: size.width / 2, y: size.height / 4 } });
}

/** A drawn menu with its corner at a point. A menu opens from a trigger, so a point-sized one stands there. */
function PointedMenu(props: { menu: Pointed }) {
  const menu = props.menu;
  const surface = () => popupSurface("min-w-48 p-1");
  return (
    <Menu
      open
      class="absolute"
      style={{ left: menu.at.x, top: menu.at.y }}
      popupClass={surface().class}
      popupStyle={surface().style}
      onOpenChange={(event) => {
        if (!event.open && pointed() === menu) pointed.set(null);
      }}
      trigger={<View style={{ width: 1, height: 1 }} />}
    >
      <DrawnMenu items={menu.items} />
    </Menu>
  );
}

/**
 * Where a browser draws `popupMenu`'s menus: one over the window, in
 * `App.tsx`, and one in each dialog that covers it, the menus opening in the
 * newest shown. Place it last in what it covers. Nothing on the desktop.
 */
export function MenuLayer() {
  if (hasSystemMenus) return null;
  const me = Symbol("menu layer");
  effect(() => {
    untracked(() => layers.update((list) => [...list, me]));
    return () => layers.update((list) => list.filter((each) => each !== me));
  });
  const shown = () => {
    const menu = pointed();
    const list = layers();
    return menu && list[list.length - 1] === me ? [menu] : [];
  };
  // A new menu is a new object, so a menu opened over another starts afresh.
  return <Each of={shown()}>{(menu) => <PointedMenu menu={menu()} />}</Each>;
}

/** A small heading over a group of rows, in a sidebar or a form. */
export function SectionLabel(props: { text: string }) {
  return (
    <View class="px-2 pb-1 pt-3">
      <Text class="text-xs font-medium text-muted-foreground">{props.text}</Text>
    </View>
  );
}
