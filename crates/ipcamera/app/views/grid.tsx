import { type Accessor, type AnimatedValue, computed, effect, type JSX, keymap, List, Scroll, type ScrollRef, signal, Text, View, type ViewStyle } from "nsh";
import { ResizablePanel, ResizablePanelGroup } from "nsh/base";
import { Button, CheckBox, GroupTitle, HEADER_BAR, HEADER_COLLAPSE, LINE_HANDLE, SelectionBar, TABLE_ROW, TableRow, TILE_PAD, type TileSize, TitleRow } from "../../../dp/ui";
import { countLabel, deleteLabel, type Item, type Library, type Row, rowsOf } from "./library";

const PICTURES: Record<Exclude<TileSize, "list">, { least: number; most: number; caption: boolean }> = {
  large: { least: 340, most: 4, caption: true },
  medium: { least: 220, most: 6, caption: true },
  small: { least: 120, most: 12, caption: false },
};
const GAP = 16;
const GUTTER = 24;
const TABLE_GUTTER = GUTTER - 8;
const CHECK = 40;
const TILE_EXTRA = 2 * TILE_PAD + 44 + (GAP - 2 * TILE_PAD);
const LIST_ROW = 45 + 2 * TILE_PAD + (GAP - 2 * TILE_PAD);
const LIST_LINE_INSET = GUTTER - TILE_PAD + 8;
const BAR_ROOM = 96;
const HEADER_OPEN = HEADER_COLLAPSE + HEADER_BAR;
const ASIDE_TOP = 8;
const ASIDE_BOTTOM = 16;
const ASIDE_BEFORE = 8;
const ASIDE_EDGES = ASIDE_BEFORE + GUTTER;

/** How many pictures of `size` fit across `width`: one to a row in a list. */
export function columnsFor(width: number, size: TileSize = "medium"): number {
  if (size === "list") return 1;
  const { least, most } = PICTURES[size];
  return Math.max(1, Math.min(most, Math.floor((width - 2 * GUTTER + GAP) / (least + GAP))));
}

/** Draws one item's tile from an accessor, so the tile follows its item as it changes. */
type TileOf<T> = (item: Accessor<T>) => JSX.Element;

/** A page's table, shown at the List size in place of pictures; the grid keeps the checkboxes, the fill and the headings. */
export type LibraryTable<T> = {
  titles: () => JSX.Element;
  cells: (item: Accessor<T>, beside: Accessor<boolean>) => JSX.Element;
  label: (item: T) => string;
  /** The cells' widest, in points. */
  width: number;
  /** The cells' narrowest beside the aside, in points. */
  least: number;
  /** Whether `item` is the one the aside shows. */
  current?: (item: T) => boolean;
  /** Moves the aside's file by `by` rows, returning the one it moved to, or `null`. */
  step?: (by: number) => string | null;
};

/** A panel beside a page's table; `tableWidth` is the width the table was dragged to, `null` for its own. */
export type TableAside = {
  least: number;
  view: (room: Accessor<{ width: number; height: number }>) => JSX.Element;
  tableWidth: Accessor<number | null>;
  onTableWidth: (width: number) => void;
};

type Line<T> = Row<T> | { kind: "titles"; key: string };

type Heading = Extract<Row<Item>, { kind: "heading" }>;

function SectionHeading<T extends Item>(props: { library: Library<T>; row: Heading; table: boolean }) {
  const picked = () => props.library.sectionPicked(props.row.section);
  const check = () => (
    <CheckBox
      label={`Select all of ${props.row.title}`}
      checked={picked() === "all"}
      mixed={picked() === "some"}
      onChange={() => props.library.toggleSection(props.row.section)}
    />
  );
  const title = () => <GroupTitle title={props.row.title} count={countLabel(props.row.count)} open={props.row.open} onToggle={() => props.library.toggleCollapsed(props.row.section)} />;
  return (
    <If
      when={props.table}
      else={
        <View class="h-12 justify-end bg-background pb-1" style={{ paddingHorizontal: GUTTER }}>
          <View class="flex-row items-center gap-1">
            {check()}
            {title()}
          </View>
        </View>
      }
    >
      <View class="flex-row items-end bg-background pb-1" style={{ height: TABLE_ROW, paddingHorizontal: TABLE_GUTTER }}>
        <View class="h-7 items-center justify-center" style={{ width: CHECK }}>
          {check()}
        </View>
        {title()}
      </View>
    </If>
  );
}

function TileRow<T extends Item>(props: { items: T[]; columns: number; tile: TileOf<T>; line: boolean }) {
  const places = () => Array.from({ length: props.columns }, (_, index) => props.items[index] ?? null);
  return (
    <View class="flex-row" style={{ gap: GAP - 2 * TILE_PAD, paddingHorizontal: GUTTER - TILE_PAD, paddingBottom: GAP - 2 * TILE_PAD }}>
      <Each of={places()} key={(item, index) => item?.name ?? `empty:${index}`}>
        {(item) => (
          <If when={item()} else={<View class="flex-1" style={{ padding: TILE_PAD }} />}>
            {(shown) => props.tile(shown)}
          </If>
        )}
      </Each>
      <If when={props.line}>
        <View class="absolute h-px bg-border" style={{ left: LIST_LINE_INSET, right: LIST_LINE_INSET, bottom: (GAP - 2 * TILE_PAD) / 2, pointerEvents: "none" }} />
      </If>
    </View>
  );
}

function Titles<T extends Item>(props: { library: Library<T>; table: LibraryTable<T> }) {
  const chosen = () => props.library.chosen().length;
  const all = () => chosen() > 0 && chosen() === props.library.ordered().length;
  return (
    <View class="bg-background" style={{ paddingHorizontal: TABLE_GUTTER }}>
      <TitleRow>
        <View class="items-center" style={{ width: CHECK }}>
          <CheckBox
            label={all() ? "Clear the selection" : "Select all"}
            checked={all()}
            mixed={chosen() > 0 && !all()}
            onChange={() => (all() ? props.library.clearSelection() : props.library.selectAll())}
          />
        </View>
        {props.table.titles()}
      </TitleRow>
    </View>
  );
}

function TableLine<T extends Item>(props: { library: Library<T>; item: Accessor<T>; table: LibraryTable<T>; beside: boolean; line: boolean }) {
  const on = () => props.library.isSelected(props.item().name);
  const current = () => props.beside && (props.table.current?.(props.item()) ?? false);
  return (
    <View style={{ paddingHorizontal: TABLE_GUTTER }}>
      <TableRow selected={on() || current()} line={props.line}>
        <View class="items-center" style={{ width: CHECK }}>
          <CheckBox label={`Select ${props.table.label(props.item())}`} checked={on()} onChange={() => props.library.pick(props.item().name, "toggle")} />
        </View>
        {props.table.cells(props.item, () => props.beside)}
      </TableRow>
    </View>
  );
}

/** A row of the list; cells are reused, so it follows `row` as it changes. */
function GridRow<T extends Item>(props: { library: Library<T>; row: Line<T>; columns: number; tile: TileOf<T>; table: LibraryTable<T> | undefined; beside: boolean; line: boolean }) {
  const titles = () => (props.row.kind === "titles" ? props.table : undefined);
  const heading = () => (props.row.kind === "heading" ? props.row : undefined);
  const tiles = () => (props.row.kind === "tiles" ? props.row : undefined);
  return (
    <View>
      <If when={titles()}>{(table) => <Titles library={props.library} table={table()} />}</If>
      <If when={heading()}>{(shown) => <SectionHeading library={props.library} row={shown()} table={props.table !== undefined} />}</If>
      <If when={tiles()}>
        {(shown) => (
          <If when={props.table} else={<TileRow items={shown().items} columns={props.columns} tile={props.tile} line={props.line} />}>
            {(table) => (
              <Each of={shown().items} key={(item) => item.name}>
                {(item) => <TableLine library={props.library} item={item} table={table()} beside={props.beside} line={props.line} />}
              </Each>
            )}
          </If>
        )}
      </If>
    </View>
  );
}

function SelectionKeys<T extends Item>(props: { library: Library<T>; busy: () => boolean; onDelete: (items: T[]) => void }) {
  keymap.bind("mod+a", () => !props.busy() && props.library.selectAll(), { when: "notEditing" });
  return (
    <If when={props.library.selecting()}>
      <ChosenKeys library={props.library} busy={props.busy} onDelete={props.onDelete} />
    </If>
  );
}

function StepKeys(props: { step: (by: number) => string | null; busy: () => boolean; reveal: (name: string) => void }) {
  const go = (by: number) => {
    if (props.busy()) return;
    const name = props.step(by);
    if (name !== null) props.reveal(name);
  };
  keymap.bind("up", () => go(-1), { when: "notEditing", repeat: true });
  keymap.bind("down", () => go(1), { when: "notEditing", repeat: true });
  return null;
}

/** Bound only while something is selected, so Delete and Escape stay the app's otherwise. */
function ChosenKeys<T extends Item>(props: { library: Library<T>; busy: () => boolean; onDelete: (items: T[]) => void }) {
  keymap.bind("delete", () => !props.busy() && props.onDelete(props.library.chosen()), { when: "notEditing" });
  keymap.bind("escape", () => !props.busy() && props.library.clearSelection(), { when: "notEditing" });
  return null;
}

function Selection<T extends Item>(props: { library: Library<T>; count: (count: number) => string; onDelete: (items: T[]) => void }) {
  const chosen = () => props.library.chosen().length;
  return (
    <If when={chosen() > 0}>
      <SelectionBar onClear={props.library.clearSelection}>
        <If when={chosen() < props.library.ordered().length}>
          <Button kind="ghost" label="Select all" accessibilityLabel={`Select all ${props.count(props.library.ordered().length)}`} onPress={props.library.selectAll} />
        </If>
        <Button kind="danger" label={deleteLabel(chosen())} accessibilityLabel={`Delete ${props.count(chosen())}`} onPress={() => props.onDelete(props.library.chosen())} />
      </SelectionBar>
    </If>
  );
}

/** A library page's body: its header over the library's sections as pictures, or as `table` at the List size. */
export function LibraryGrid<T extends Item>(props: {
  testID: string;
  scroll: AnimatedValue;
  library: Library<T>;
  header: JSX.Element;
  empty: JSX.Element;
  tile: TileOf<T>;
  table?: LibraryTable<T>;
  aside?: TableAside;
  count: (count: number) => string;
  onDelete: (items: T[]) => void;
  busy: () => boolean;
}) {
  const width = signal(0);
  const height = signal(0);
  const size = () => props.library.size();
  const table = () => (size() === "list" ? props.table : undefined);
  const tableWidth = () => {
    const shown = table();
    return shown === undefined ? undefined : 2 * TABLE_GUTTER + CHECK + shown.width;
  };
  const tableLeast = () => {
    const shown = table();
    return shown === undefined ? 0 : 2 * TABLE_GUTTER + CHECK + shown.least;
  };
  const beside = computed(() => {
    if (props.aside === undefined || table() === undefined) return false;
    return width() - tableLeast() - ASIDE_EDGES >= props.aside.least;
  });
  const tableSize = () => {
    const aside = props.aside;
    if (aside === undefined) return 0;
    const wanted = aside.tableWidth() ?? tableWidth() ?? 0;
    return Math.max(tableLeast(), Math.min(wanted, width() - ASIDE_EDGES - aside.least));
  };
  const asideWidth = signal(0);
  const asideRoom = computed(() => ({ width: asideWidth(), height: height() - HEADER_OPEN - ASIDE_TOP - ASIDE_BOTTOM }));
  const columns = computed(() => columnsFor(width(), size()));
  const rows = computed((): Line<T>[] => {
    const shown = rowsOf(props.library.sections(), columns(), props.library.isCollapsed);
    return table() && shown.length > 0 ? [{ kind: "titles", key: "titles" }, ...shown] : shown;
  });
  const ruled = computed(() => {
    const shown = rows();
    return new Set(shown.flatMap((row, index) => (row.kind === "tiles" && shown[index + 1]?.kind === "tiles" ? [row.key] : [])));
  });
  // A list sticks one row at a time, so a table's titles stick in place of its headings.
  const sticky = computed(() => {
    if (table()) return rows().length > 0 ? [0] : [];
    return rows().flatMap((row, index) => (row.kind === "heading" ? [index] : []));
  });
  const rowHeight = () => {
    const chosen = size();
    if (chosen === "list") return table() ? TABLE_ROW : LIST_ROW;
    const n = columns();
    const tile = (width() - 2 * GUTTER - (n - 1) * GAP) / n;
    return Math.max(60, tile * (9 / 16) + (PICTURES[chosen].caption ? TILE_EXTRA : GAP));
  };

  // Reads `ordered` so the selection drops what the filters hide.
  effect(() => {
    props.library.ordered();
    props.library.keepShown();
  });

  let scroller: ScrollRef | undefined;
  /** Every table row is `TABLE_ROW` tall, so a row's place is its index. */
  const reveal = (name: string) => {
    const index = rows().findIndex((row) => row.kind === "tiles" && row.items.some((item) => item.name === name));
    if (index < 0) return;
    const top = HEADER_OPEN + index * TABLE_ROW;
    props.scroll.getValue().then((at) => {
      const from = Math.max(HEADER_OPEN, at + HEADER_BAR) + TABLE_ROW;
      const seen = height() - (props.library.selecting() ? BAR_ROOM : 0);
      if (top < from) scroller?.scrollTo({ y: Math.max(0, top - HEADER_BAR - TABLE_ROW), animated: false });
      else if (top + TABLE_ROW > at + seen) scroller?.scrollTo({ y: top + TABLE_ROW - seen, animated: false });
    });
  };

  const list = (style: () => ViewStyle) => (
    <List
      viewport="parent"
      each={rows()}
      keyExtractor={(row) => row.key}
      empty={props.empty}
      stickyHeaderIndices={sticky()}
      stickyHeaderOffset={HEADER_BAR}
      estimatedItemSize={rowHeight()}
      contentStyle={{ paddingBottom: BAR_ROOM }}
      style={style()}
    >
      {(row) => <GridRow library={props.library} row={row()} columns={columns()} tile={props.tile} table={table()} beside={beside()} line={size() === "list" && ruled().has(row().key)} />}
    </List>
  );

  return (
    <View
      class="flex-1"
      onLayout={(event) => {
        width.set(event.bounds.size.width);
        height.set(event.bounds.size.height);
      }}
    >
      <SelectionKeys library={props.library} busy={props.busy} onDelete={props.onDelete} />
      <If when={beside() ? table()?.step : undefined}>{(step) => <StepKeys step={step()} busy={props.busy} reveal={reveal} />}</If>
      <Scroll ref={(ref: ScrollRef) => (scroller = ref)} testID={props.testID} scrollValue={props.scroll} class="flex-1">
        {props.header}
        <If when={props.aside} else={<View class="flex-row items-start">{list(() => ({ flexGrow: 1, flexShrink: 1, flexBasis: 0, minWidth: 0, maxWidth: tableWidth() }))}</View>}>
          {(aside) => (
            <ResizablePanelGroup axis="horizontal" class="h-auto" handleClass={LINE_HANDLE.line} handleThumbClass={LINE_HANDLE.thumb} handleAccessibilityLabel="Resize the table">
              <ResizablePanel size={beside() ? tableSize() : null} minSize={beside() ? tableLeast() : 0} onResize={({ size }) => aside().onTableWidth(size)}>
                {list(() => ({ minWidth: 0, maxWidth: beside() ? undefined : tableWidth() }))}
              </ResizablePanel>
              <ResizablePanel visible={beside()} minSize={aside().least + ASIDE_EDGES}>
                <If when={beside()}>
                  <View
                    style={{ position: "sticky", top: HEADER_BAR + ASIDE_TOP, marginTop: ASIDE_TOP, marginLeft: ASIDE_BEFORE, marginRight: GUTTER }}
                    onLayout={(event) => asideWidth.set(event.bounds.size.width)}
                  >
                    {aside().view(() => asideRoom())}
                  </View>
                </If>
              </ResizablePanel>
            </ResizablePanelGroup>
          )}
        </If>
      </Scroll>
      <Selection library={props.library} count={props.count} onDelete={props.onDelete} />
    </View>
  );
}

/** Says no `what` match the filters, with the button that clears them. */
export function NothingMatches(props: { what: string; onClear: () => void }) {
  return (
    <View class="items-center gap-3 px-6 py-12">
      <Text class="text-sm text-muted-foreground">{`No ${props.what} match these filters.`}</Text>
      <Button label="Clear filters" onPress={props.onClear} />
    </View>
  );
}
