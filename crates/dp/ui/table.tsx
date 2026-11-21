import { type JSX, Text, View, type ViewStyle } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { Appear } from "./appear";
import { look } from "./brand";
import { IconButton } from "./button";
import { InlineSelect } from "./field";
import { Icon } from "./icon";
import { popupSurface } from "./menu";

const tabular = { fontVariant: ["tabular-nums" as const] };

/** Which way a column sorts the table, or `null` when it doesn't. */
export type SortDirection = "ascending" | "descending" | null;

/** The height of a table's row, and of its titles' row, in points. */
export const TABLE_ROW = 44;

/** A column's title; with `onSort`, a button showing which way the column sorts. */
export function ColumnTitle(props: { title: string; sort?: SortDirection; onSort?: () => void; style?: ViewStyle }) {
  const label = () => (
    <>
      <Text class={`text-xs font-medium ${props.sort ? "text-foreground" : "text-muted-foreground"}`} style={{ numberOfLines: 1 }}>
        {props.title}
      </Text>
      <If when={props.sort}>{(sort) => <Icon name={sort() === "ascending" ? "arrow-up" : "arrow-down"} size={12} tint="foreground" />}</If>
    </>
  );
  return (
    <View class="flex-row items-center" style={props.style}>
      <If when={props.onSort} else={<View class="flex-row items-center gap-1 px-2">{label()}</View>}>
        {(onSort) => (
          <BaseButton
            accessibilityLabel={`Sort by ${props.title.toLowerCase()}`}
            class={`h-7 min-w-0 flex-row items-center gap-1 rounded-md px-2 ${look().buttons.ghost.frame} ${look().rings.visible}`}
            onPress={() => onSort()()}
          >
            {label()}
          </BaseButton>
        )}
      </If>
    </View>
  );
}

/** The title of a run of rows or pictures, which folds them away. */
export function GroupTitle(props: { title: string; count: string; open: boolean; onToggle: () => void }) {
  return (
    <View
      role="button"
      accessibilityLabel={`${props.title}, ${props.count}`}
      accessibilityState={{ expanded: props.open }}
      class={`h-7 min-w-0 flex-row items-center gap-1.5 rounded-md pl-1 pr-2 ${look().buttons.ghost.frame} ${look().rings.visible}`}
      onPress={props.onToggle}
    >
      <Icon name={props.open ? "chevron-down" : "chevron-right"} size={14} tint="muted-foreground" />
      <Text role="header" class="text-sm font-semibold text-foreground" style={{ numberOfLines: 1 }}>
        {props.title}
      </Text>
      <Text class="text-xs text-muted-foreground" style={tabular}>
        {props.count}
      </Text>
    </View>
  );
}

/** The row of column titles, on the page's color so the rows pass under it. */
export function TitleRow(props: { children: JSX.Element }) {
  return (
    <View class="flex-row items-center border-b border-border bg-background" style={{ height: TABLE_ROW }}>
      {props.children}
    </View>
  );
}

/** The class of a row's main text, selected or not. */
export const rowText = (selected: boolean) => (selected ? (look().listRow.selectedText ?? "text-foreground") : "text-foreground");

/** Keeps a row's fill clear of the line under it. */
const ROW_FILL_INSET = 2;

/** A row of a table; `line` draws a line along its foot, for every row with another after it. */
export function TableRow(props: { selected: boolean; line?: boolean; children: JSX.Element }) {
  const fill = () => (props.selected ? look().listRow.selected : look().listRow.hover);
  return (
    <View style={{ height: TABLE_ROW }}>
      <View class={`flex-1 flex-row items-center rounded-md ${fill()}`} style={{ marginVertical: ROW_FILL_INSET }}>
        {props.children}
      </View>
      <If when={props.line}>
        <View class="absolute bottom-0 left-2 right-2 h-px bg-border" style={{ pointerEvents: "none" }} />
      </If>
    </View>
  );
}

/** The bar floating over a page's foot while items are selected, at its start, clear of the toasts. */
export function SelectionBar(props: { count?: string; onClear: () => void; children: JSX.Element }) {
  const surface = () => popupSurface(`flex-row items-center gap-1 py-1.5 pr-1.5 ${props.count === undefined ? "pl-1.5" : "pl-4"}`);
  return (
    // `box-none` lets presses through beside the bar.
    <View class="absolute bottom-6 left-0 right-0 items-start px-4" style={{ pointerEvents: "box-none" }}>
      <Appear>
        <View class={surface().class} style={surface().style}>
          <If when={props.count}>
            {(count) => (
              <Text class="mr-2 text-sm font-medium text-popover-foreground" style={{ numberOfLines: 1, fontVariant: ["tabular-nums"] }}>
                {count()}
              </Text>
            )}
          </If>
          {props.children}
          <View class={`mx-1 h-5 w-px ${look().menuItem.line}`} />
          <IconButton icon="x" label="Clear selection" onPress={props.onClear} />
        </View>
      </Appear>
    </View>
  );
}

/** The height of a table's footer, in points: as tall as a page header's bar. */
export const TABLE_FOOTER = 48;

/** The bar under a table that stays put while its rows scroll: its counts and its pager. */
export function TableFooter(props: { pager?: JSX.Element; children: JSX.Element }) {
  return (
    <View class="flex-row items-center gap-4 border-t border-border bg-background px-6" style={{ height: TABLE_FOOTER }}>
      <View class="min-w-0 flex-1 flex-row items-center gap-2">{props.children}</View>
      {props.pager}
    </View>
  );
}

/** A table's pager: rows per page, which page (from 0) of how many, and the buttons to turn. */
export function Pager(props: { page: number; pages: number; size: number; sizes: number[]; onPage: (index: number) => void; onSize: (size: number) => void }) {
  const first = () => props.page <= 0;
  const last = () => props.page >= props.pages - 1;
  return (
    <View class="flex-row items-center gap-1">
      <Text class="mr-1 text-sm text-muted-foreground max-lg:hidden" style={{ numberOfLines: 1 }}>
        Rows per page
      </Text>
      <InlineSelect
        label="Rows per page"
        items={props.sizes.map((size) => ({ value: String(size), title: String(size) }))}
        value={String(props.size)}
        class="mr-4 w-20"
        onChange={(value) => props.onSize(Number(value))}
      />
      <Text class="mr-2 text-sm text-muted-foreground" style={{ numberOfLines: 1, ...tabular }}>
        {`Page ${(props.page + 1).toLocaleString()} of ${props.pages.toLocaleString()}`}
      </Text>
      <IconButton icon="chevrons-left" label="First page" disabled={first()} onPress={() => props.onPage(0)} />
      <IconButton icon="chevron-left" label="Previous page" disabled={first()} onPress={() => props.onPage(props.page - 1)} />
      <IconButton icon="chevron-right" label="Next page" disabled={last()} onPress={() => props.onPage(props.page + 1)} />
      <IconButton icon="chevrons-right" label="Last page" disabled={last()} onPress={() => props.onPage(props.pages - 1)} />
    </View>
  );
}
