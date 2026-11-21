import { effect, type JSX, signal, Text, untracked, View, type ViewStyle, windowSize } from "nsh";
import { Menu, MenuItem, MenuSeparator, Submenu } from "nsh/base";
import { isAvailable as hasSystemMenus, type MenuItem as SystemMenuItem, Menu as SystemMenu } from "nsh/desktop";
import { look } from "./brand";
import { Icon, type IconName } from "./icon";

type Point = { x: number; y: number };

/** A menu's items as the system's menus take them; `false`, `null` and `undefined` leave one out. */
export type MenuEntries = readonly (SystemMenuItem | false | null | undefined)[];

/** The theme's surface for what floats over the window; `extra` adds classes. */
export function popupSurface(extra = ""): { class: string; style: ViewStyle } {
  const popup = look().popup;
  return { class: `${popup.class} ${extra}`, style: popup.style };
}

/** A popup menu from `trigger`, a plain view with `role="button"`: a `Button` there would take its press. */
export function PopupMenu(props: { trigger: JSX.Element; placement?: "top" | "bottom"; children: JSX.Element }) {
  const surface = () => popupSurface("min-w-48 p-1");
  return (
    <Menu offset={4} placement={props.placement} popupClass={surface().class} popupStyle={surface().style} trigger={props.trigger}>
      {props.children}
    </Menu>
  );
}

/** A menu's or a list's button showing the current choice; `menu` makes it a `PopupMenu` trigger. */
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

/** A `PopupMenu` trigger showing the current choice beside an icon. */
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

/** An icon can't follow a class, so a second one in `activeInk` shows on hover; the item needs `group`. */
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

/** An item of a `PopupMenu`; `checked` shows a setting's state, `leading` takes the icon's place. */
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

// nsh has no browser menus (`Menu.popup` does nothing there), so these draw them with `Menu`.

/** A plain view: `Submenu` makes it the item. */
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

function DrawnItems(props: { items: MenuEntries }) {
  return <Each of={props.items.filter((item): item is SystemMenuItem => Boolean(item))}>{(item) => <DrawnItem item={item()} />}</Each>;
}

/** A test finds a drawn menu's items as `id:menu`. */
function DrawnMenu(props: { items: MenuEntries }) {
  return (
    <View testID="menu">
      <DrawnItems items={props.items} />
    </View>
  );
}

/** A button that opens a menu: the system's on the desktop, a drawn one in a browser. */
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
      // Only a place to stand: the button inside keeps its press, and opens the menu.
      trigger={<View>{props.children((items) => shown.set(items))}</View>}
    >
      <DrawnMenu items={shown() ?? []} />
    </Menu>
  );
}

type Pointed = { items: MenuEntries; at: Point };

const pointed = signal<Pointed | null>(null);

const layers = signal<symbol[]>([]);

/** Opens a menu of `items` with its corner at `at`, an event's `pagePosition`. */
export function popupMenu(items: MenuEntries, at?: Point) {
  if (hasSystemMenus) {
    void SystemMenu.popup(items, at);
    return;
  }
  const size = windowSize();
  pointed.set({ items, at: at ?? { x: size.width / 2, y: size.height / 4 } });
}

/** A menu opens from a trigger, so a point-sized one stands at the point. */
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

/** Where a browser draws `popupMenu`'s menus; place it last in the window or dialog it covers. */
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
