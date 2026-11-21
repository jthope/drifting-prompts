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
  /** The file's name. */
  name: string;
  /** Its folder, beside another file of the same name, or `""`; said, not drawn. */
  detail: string;
  /** Kept first, its pin always shown. */
  pinned: boolean;
  /** Whether it can be pinned; true unless said. */
  pinnable?: boolean;
  /** The preview, which the next file opened takes over. */
  preview: boolean;
  /** How many matches a search found in it; 0 or absent for none. */
  count?: number;
  /** Its icon, for a tab holding something other than a file. */
  icon?: IconName;
};

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
  /** Hidden by opacity, so the tab keeps its width. */
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
      <If when={props.selected}>
        <View class={`absolute inset-x-1.5 top-0 h-0.5 rounded-full ${look().tabs.indicator}`} />
      </If>
      {/* Takes its own presses, for their count; a test finds it as `id:tab:<id>`. */}
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

/** The tabs over a reader, one per open file, scrolling in one row or wrapping with `wrap`. */
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
      {/* `pt-2` and a 28-point tab make the strip's 36 points. */}
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
