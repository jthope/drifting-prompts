import { Text, View } from "nsh";
import { Tree, type TreeItem, type TreeRef } from "nsh/base";
import { look } from "./brand";
import { CountBadge } from "./count";
import { Icon, type IconName } from "./icon";
import { type MenuEntries, popupMenu } from "./menu";
import { Press } from "./press";

const INDENT = 14;

/** A press on a row of a `TreeList`: its click count, and the keys held (`control` is Command on a Mac). */
export type TreePress = { clickCount: number; control: boolean; shift: boolean };

/** nsh's `Tree` in the theme's rows; it selects one item only, so `marked` and `onPress` let the app choose several. */
export function TreeList(props: {
  label: string;
  items: TreeItem[];
  expanded: string[];
  selected: string | null;
  marked?: string[];
  icon: (id: string, folder: boolean) => IconName | null;
  lead?: (id: string) => string;
  detail?: (id: string) => string;
  found?: (id: string) => [number, number] | null;
  count?: (id: string) => number;
  dimmed?: (id: string) => boolean;
  menu?: (id: string) => MenuEntries;
  testID?: string;
  onExpandedChange: (expanded: string[]) => void;
  onSelect: (id: string) => void;
  onAction?: (id: string) => void;
  onPress?: (id: string, press: TreePress) => void;
}) {
  let tree: TreeRef | undefined;
  const marked = () => props.marked ?? [];
  return (
    <Tree
      ref={(ref) => {
        tree = ref;
      }}
      testID={props.testID}
      accessibilityLabel={props.label}
      class="flex-1"
      listClass="gap-0.5 px-2 pb-2"
      itemClass={`rounded-md ${look().rings.visible}`}
      items={props.items}
      expanded={props.expanded}
      selected={props.selected}
      onExpandedChange={(event) => props.onExpandedChange(event.expanded)}
      onSelectedChange={(event) => props.onSelect(event.selected)}
      onAction={(event) => props.onAction?.(event.id)}
      renderItem={(entry) => {
        const chosen = () => (marked().length > 0 ? marked().includes(entry.id) : props.selected === entry.id);
        const open = () => props.expanded.includes(entry.id);
        const detail = () => (props.detail ? props.detail(entry.id) : "");
        const lead = () => (props.lead ? props.lead(entry.id) : "");
        const icon = () => props.icon(entry.id, entry.folder);
        const parts = (): [string, string, string] => {
          const at = props.found?.(entry.id);
          return at ? [entry.label.slice(0, at[0]), entry.label.slice(at[0], at[1]), entry.label.slice(at[1])] : [entry.label, "", ""];
        };
        const ink = () => (chosen() ? (look().listRow.selectedText ?? "text-foreground") : "text-foreground");
        return (
          <View
            class={`h-7 flex-1 flex-row items-center gap-1.5 rounded-md pr-2 ${chosen() ? look().listRow.selected : look().listRow.hover} ${props.dimmed?.(entry.id) ? "opacity-50" : ""}`}
            style={{ paddingLeft: 4 + entry.depth * INDENT }}
            onContextMenu={(event) => props.menu && popupMenu(props.menu(entry.id), event.pagePosition)}
            onPress={
              props.onPress
                ? (event) => {
                    // Keeps the keyboard in the tree, as its own press would.
                    tree?.focus();
                    props.onPress?.(entry.id, {
                      clickCount: event.clickCount,
                      control: event.modifiers.control || event.modifiers.platform,
                      shift: event.modifiers.shift,
                    });
                  }
                : undefined
            }
          >
            <View class="w-3.5 items-center">
              <If when={entry.folder}>
                <Icon name={open() ? "chevron-down" : "chevron-right"} size={14} tint="muted-foreground" />
              </If>
            </View>
            <If when={lead() !== ""}>
              <Text class="min-w-6 text-right text-xs text-muted-foreground" style={{ fontVariant: ["tabular-nums"] }}>
                {lead()}
              </Text>
            </If>
            <If when={icon()}>{(name) => <Icon name={name()} size={16} tint={entry.folder ? "muted-foreground" : "foreground"} />}</If>
            <Text class={`shrink text-sm ${ink()}`} style={{ numberOfLines: 1 }}>
              {parts()[0]}
              <Text style={{ backgroundColor: look().found.match }}>{parts()[1]}</Text>
              {parts()[2]}
            </Text>
            <Text class="flex-1 text-xs text-muted-foreground" style={{ numberOfLines: 1 }}>
              {detail()}
            </Text>
            <CountBadge count={props.count ? props.count(entry.id) : 0} />
          </View>
        );
      }}
    />
  );
}

/** A line of an outline: its title, its depth (0 at the top), and the matches found under it. */
export type OutlineLine = { title: string; depth: number; count?: number };

/** An outline beside a page, `current` the line the reader is in. */
export function OutlineList(props: { lines: OutlineLine[]; current: number; onSelect: (index: number) => void }) {
  return (
    <View class="gap-0.5">
      <Each of={props.lines.map((line, index) => ({ ...line, index }))} key={(line) => `${line.index}:${line.title}`}>
        {(line) => {
          const here = () => props.current === line().index;
          return (
            <Press
              label={line().title}
              selected={here()}
              scale={1}
              class={`min-h-7 flex-row items-center rounded-md py-1 pr-2 ${here() ? look().listRow.selected : look().listRow.hover} ${look().rings.visible}`}
              style={{ paddingLeft: 8 + line().depth * INDENT }}
              onPress={() => props.onSelect(line().index)}
            >
              <Text
                class={`flex-1 text-xs ${here() ? (look().listRow.selectedText ?? "text-foreground") : line().depth === 0 ? "text-foreground" : "text-muted-foreground"}`}
                style={{ numberOfLines: 2 }}
              >
                {line().title === "" ? "Untitled" : line().title}
              </Text>
              <CountBadge count={line().count ?? 0} class="ml-1" />
            </Press>
          );
        }}
      </Each>
    </View>
  );
}
