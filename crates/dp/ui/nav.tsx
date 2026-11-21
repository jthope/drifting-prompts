import { type JSX, Text, View } from "nsh";
import { Button, Tab, Tabs } from "nsh/base";
import { look } from "./brand";
import { IconButton } from "./button";
import { Icon, type IconName } from "./icon";
import { Frame } from "./island";
import { Logo } from "./logo";
import type { Product } from "./logos";
import { type MenuEntries, MenuOpener, popupMenu } from "./menu";

const ROW_LOOK = "h-8 flex-row items-center gap-2 rounded-md hover:bg-sidebar-accent active:bg-sidebar-accent";
const ROW = `min-w-0 flex-1 ${ROW_LOOK}`;

// Over the row's button, not in it, so a press leaves the row as it is. The fade is on the button,
// not on `MenuOpener`, whose drawn menu on the web must not fade with it.
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

/** A row of the sidebar; its `…` keeps its room, so the label never runs under it. */
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
        {/* Inside the button, so a press on the grip opens the row. */}
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

/** One of `NavTabs`' rows; `flagged` marks a section with a field to fix. */
export type NavTab<T extends string> = { value: T; title: string; icon: IconName; flagged?: boolean; disabled?: boolean };

/** Rows in the sidebar's look that are tabs: a dialog's sections down its side. */
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

/** The sidebar on the window's frame, topped by the title bar's left end, which drags the window. */
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

/** A heading that folds the rows under it; `menu` is as a `NavRow`'s, its `…` always shown. */
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
