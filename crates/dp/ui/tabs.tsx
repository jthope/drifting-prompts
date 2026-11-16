// Tabs of two kinds. A page header's tabs (`PageTabs`, in `collapsing.tsx`'s
// bar, where they stay in reach however far the page scrolls): plain names,
// the selected one underlined in the theme's line along the banner's bottom
// edge. And the tabs over a reader (`FileTabs`), one for each file open, as an
// editor has them: the file's name alone, pinned ones first, a preview in
// italics, a pin and a close button beside the name (the pin always on a
// pinned tab, both on the one in front and on the one under the pointer), a
// right-click menu, dragged along the strip to another place (`reorder.tsx`),
// and how many matches a search found in each. The tab in front is a card of
// the document's color with rounded top corners, which joins the breadcrumbs
// under the strip (`breadcrumbs.tsx`); the one under the pointer takes the
// theme's hover in the same shape, and the others are text on the strip,
// apart by a little room. A tab can hold something else, with an icon of its
// own (a search's results).
import { type JSX, Scroll, Text, View } from "nsh";
import { Tab, Tabs } from "nsh/base";
import { EdgeLine, look, solid } from "./brand";
import { Breadcrumbs, type Crumb } from "./breadcrumbs";
import { Hint, IconButton } from "./button";
import { CountBadge } from "./count";
import { Icon, type IconName } from "./icon";
import { type MenuEntries, popupMenu } from "./menu";
import { createReorder, type Reorder, Reorderable } from "./reorder";

/** A page's tabs: `items` in order, `value` the selected one's, `onChange` told of a press. */
export function PageTabs<T extends string>(props: { items: readonly { value: T; title: string }[]; value: T; onChange: (value: T) => void }) {
  return (
    <Tabs class="flex-row gap-5" indicatorClass={`bottom-0 h-0.5 rounded-full ${look().tabs.indicator}`}>
      <Each of={[...props.items]} key={(each) => each.value}>
        {(each) => (
          <Tab
            selected={props.value === each().value}
            class={`justify-center rounded-sm px-0.5 ${look().rings.visible}`}
            onPress={() => props.onChange(each().value)}
          >
            <Text class={`text-sm font-medium ${props.value === each().value ? look().tabs.on : look().tabs.off}`}>{each().title}</Text>
          </Tab>
        )}
      </Each>
    </Tabs>
  );
}

/** A tab of `FileTabs`: a file open in a reader. */
export type FileTab = {
  /** What the app knows it by (the file's path). */
  id: string;
  /** What it says: the file's name. */
  name: string;
  /**
   * Its folder, beside another file of the same name, or `""`. Not drawn on
   * the tab: a screen reader says it, and so does the hint while the pointer
   * rests on the tab.
   */
  detail: string;
  /** Kept first, its pin always shown. */
  pinned: boolean;
  /** Whether it can be pinned, with a pin beside its close button; true unless said (a search's results can't be). */
  pinnable?: boolean;
  /** The preview, which the next file opened takes over: its name in italics. */
  preview: boolean;
  /** How many matches a search found in the file, shown after its name; 0 or absent for none. */
  count?: number;
  /** Its icon, for a tab that holds something other than a file (a search's results); a file's by default. */
  icon?: IconName;
};

/** One tab of `FileTabs`. */
function FileTabView(props: {
  tab: FileTab;
  selected: boolean;
  menu: (id: string) => MenuEntries;
  onSelect: (id: string) => void;
  onKeep: (id: string) => void;
  onClose: (id: string) => void;
  onPin: (id: string, pinned: boolean) => void;
}) {
  const id = () => props.tab.id;
  const count = () => props.tab.count ?? 0;
  const said = () =>
    `${props.tab.name}${props.tab.detail === "" ? "" : `, in ${props.tab.detail}`}${props.tab.pinned ? ", pinned" : ""}${props.tab.preview ? ", preview" : ""}${count() === 0 ? "" : `, ${count()} ${count() === 1 ? "match" : "matches"}`}`;
  /** The classes of a button shown only while the tab is in front or under the pointer: it keeps its room while hidden, so the tab keeps its width. */
  const shownOnHover = () => (props.selected ? "" : "opacity-0 group-hover:opacity-100");
  return (
    <Tab
      selected={props.selected}
      accessibilityLabel={said()}
      tooltip={props.tab.detail === "" ? undefined : <Hint text={`In ${props.tab.detail}`} />}
      class={`group h-7 max-w-60 flex-row items-center gap-1 rounded-t-md pl-2 pr-0.5 ${props.selected ? "" : look().listRow.hover} ${look().rings.visible}`}
      style={props.selected ? { backgroundImage: solid(look().tiers.document) } : {}}
      onPress={() => props.onSelect(id())}
    >
      {/* The theme's line along the top, clear of the rounded corners. */}
      <If when={props.selected}>
        <View class={`absolute inset-x-1.5 top-0 h-0.5 rounded-full ${look().tabs.indicator}`} />
      </If>
      {/* The name takes its own presses, which say how many: a double press keeps a preview. A test finds it as `id:tab:<id>`. */}
      <View
        testID={`tab:${id()}`}
        class="min-w-0 shrink flex-row items-center gap-1.5"
        onPress={(event) => {
          props.onSelect(id());
          if (event.clickCount >= 2) props.onKeep(id());
        }}
        onContextMenu={(event) => popupMenu(props.menu(id()), event.pagePosition)}
      >
        <Icon name={props.tab.icon ?? "file-text"} size={14} tint={props.selected ? "foreground" : "muted-foreground"} />
        <Text class={`shrink text-sm ${props.tab.preview ? "italic" : ""} ${props.selected ? look().tabs.on : look().tabs.off}`} style={{ numberOfLines: 1 }}>
          {props.tab.name}
        </Text>
        <CountBadge count={count()} />
      </View>
      <View class="flex-row items-center">
        <If when={props.tab.pinnable ?? true}>
          {/* Pinned, the pin is the tab's state, always shown and in the icons' own ink; unpinned, it is quieter. */}
          <View class={props.tab.pinned ? "" : shownOnHover()}>
            <IconButton
              icon="pin"
              label={`${props.tab.pinned ? "Unpin" : "Pin"} ${props.tab.name}`}
              size="tiny"
              tint={props.tab.pinned ? undefined : "muted-foreground"}
              onPress={() => props.onPin(id(), !props.tab.pinned)}
            />
          </View>
        </If>
        <View class={shownOnHover()}>
          <IconButton icon="x" label={`Close ${props.tab.name}`} size="tiny" onPress={() => props.onClose(id())} />
        </View>
      </View>
    </Tab>
  );
}

/** The tabs of `FileTabs` in order, each draggable along its row. */
function TabList(props: {
  tabs: FileTab[];
  selected: string;
  order: Reorder;
  wrap: boolean;
  menu: (id: string) => MenuEntries;
  onSelect: (id: string) => void;
  onKeep: (id: string) => void;
  onClose: (id: string) => void;
  onPin: (id: string, pinned: boolean) => void;
}) {
  return (
    <Tabs class={`flex-row items-end gap-x-0.5 ${props.wrap ? "flex-wrap gap-y-1" : ""}`}>
      <Each of={props.tabs} key={(tab) => tab.id}>
        {(tab, index) => (
          <Reorderable reorder={props.order} index={index()} label={tab().name} whole>
            {() => (
              <FileTabView
                tab={tab()}
                selected={props.selected === tab().id}
                menu={props.menu}
                onSelect={props.onSelect}
                onKeep={props.onKeep}
                onClose={props.onClose}
                onPin={props.onPin}
              />
            )}
          </Reorderable>
        )}
      </Each>
    </Tabs>
  );
}

/**
 * The tabs over a reader, one for each open file, at the top of the document's
 * island (`Island`), in a strip as tall as a `PaneHeader` beside it and in a
 * tool pane's color, so the strip lines up with the panes' headers. The tabs
 * stand on the strip's bottom edge, shorter than it: `selected` is the one in
 * front, a card of the document's color with the theme's line along its top,
 * one with the breadcrumbs under it (`crumbs`, the way to what it shows), which
 * the document follows under a panel's line. A press brings a tab to the front
 * (`onSelect`), a double press keeps a preview (`onKeep`), its pin pins or
 * unpins it (`onPin`, with the state asked for), its close button or the menu
 * closes it (`onClose`, `menu`), and a drag along its row moves it (`onMove`),
 * within the indexes `within` gives. `start` and `end` go before and after the
 * tabs (a hidden pane's button), beside the last row. Tabs that don't fit
 * scroll sideways in one row, or with `wrap` take more rows, the strip growing
 * to hold them; there a tab is dragged along its own row only.
 */
export function FileTabs(props: {
  tabs: FileTab[];
  selected: string;
  crumbs?: Crumb[];
  wrap?: boolean;
  start?: JSX.Element;
  end?: JSX.Element;
  within: (index: number) => [number, number];
  menu: (id: string) => MenuEntries;
  onSelect: (id: string) => void;
  onKeep: (id: string) => void;
  onClose: (id: string) => void;
  onPin: (id: string, pinned: boolean) => void;
  onMove: (from: number, to: number) => Promise<boolean>;
}) {
  const wrap = () => props.wrap ?? false;
  const order = createReorder({
    axis: "horizontal",
    count: () => props.tabs.length,
    within: (index) => props.within(index),
    wraps: wrap,
    onMove: props.onMove,
  });
  const list = () => (
    <TabList
      tabs={props.tabs}
      selected={props.selected}
      order={order}
      wrap={wrap()}
      menu={props.menu}
      onSelect={props.onSelect}
      onKeep={props.onKeep}
      onClose={props.onClose}
      onPin={props.onPin}
    />
  );
  const crumbs = () => props.crumbs ?? [];
  return (
    <View>
      {/* Room above the tabs, so a row of them is as tall as the strip: 8 points and a 28-point tab make 36. */}
      <View class={`${wrap() ? "min-h-9" : "h-9"} flex-row items-stretch`} style={{ backgroundImage: solid(look().tiers.tool) }}>
        <EdgeLine edge="bottom" place="panel" />
        {props.start}
        <If when={wrap()} else={<Scroll horizontal testID="tabs" class="flex-1" contentClass="flex-row px-1 pt-2">{list()}</Scroll>}>
          <View testID="tabs" class="min-w-0 flex-1 px-1 pt-2">
            {list()}
          </View>
        </If>
        {props.end}
      </View>
      <If when={crumbs().length > 0}>
        <View class="h-7 flex-row items-center px-2" style={{ backgroundImage: solid(look().tiers.document) }}>
          <Breadcrumbs crumbs={crumbs()} testID="crumbs" class="flex-1" />
          <EdgeLine edge="bottom" place="panel" />
        </View>
      </If>
    </View>
  );
}
