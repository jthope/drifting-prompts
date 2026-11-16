// The grid the snapshots, videos and events pages share, after the photo
// stream of nsh's gallery example: under the page's collapsing header, the
// pictures in as many columns as fit at the size the View menu picks (or a
// list, one to a row), under a heading for each month, day or camera that
// sticks below the header's bar while its pictures scroll past. A page with a
// table of its own (`LibraryTable`, the events page's) shows it at the List
// size instead: its column titles stick there, and the headings scroll with
// the rows. A table ends at its columns' width rather than spreading them
// across a wide page, and a page can put a panel in the room beside it (the
// events' preview), which sticks under the header's bar while the rows scroll;
// the line between the two drags the table wider or narrower, and the panel
// takes what is left.
// A heading's chevron folds its section away and opens it again, and
// its checkbox selects it; while anything is selected, a bar over the page's
// foot says how many and deletes them, as the Cameras page's does.
import { type Accessor, type AnimatedValue, computed, effect, type JSX, keymap, List, Scroll, type ScrollRef, signal, Text, View, type ViewStyle } from "nsh";
import { ResizablePanel, ResizablePanelGroup } from "nsh/base";
import { Button, CheckBox, GroupTitle, HEADER_BAR, HEADER_COLLAPSE, LINE_HANDLE, SelectionBar, TABLE_ROW, TableRow, TILE_PAD, type TileSize, TitleRow } from "../../../dp/ui";
import { countLabel, deleteLabel, type Item, type Library, type Row, rowsOf } from "./library";

/** For each size of picture: the narrowest one gets before a row has one fewer, the most a row holds, and whether its caption shows. */
const PICTURES: Record<Exclude<TileSize, "list">, { least: number; most: number; caption: boolean }> = {
  large: { least: 340, most: 4, caption: true },
  medium: { least: 220, most: 6, caption: true },
  small: { least: 120, most: 12, caption: false },
};
/** Between pictures, across and down. */
const GAP = 16;
/** Either side of the grid, to the pictures' edges: the header and the headings line up with them. */
const GUTTER = 24;
/** Either side of a table's rows: their fill reaches a little past the pictures' edges, as the Cameras page's rows do. */
const TABLE_GUTTER = GUTTER - 8;
/** A table's column of checkboxes, as wide as the Cameras page's. */
const CHECK = 40;
/** What a row holds beside its picture: the tile's padding, the caption and what is left of the gap. */
const TILE_EXTRA = 2 * TILE_PAD + 44 + (GAP - 2 * TILE_PAD);
/** How tall a row of a list is: its small picture, its padding and the gap under it. */
const LIST_ROW = 45 + 2 * TILE_PAD + (GAP - 2 * TILE_PAD);
/** Room under the last row for the selection's bar to float over. */
const BAR_ROOM = 96;
/** The page's header at the top of the page (`LibraryHeader`'s, at its usual height): where the list starts. */
const HEADER_OPEN = HEADER_COLLAPSE + HEADER_BAR;
/** Above a table's aside, beside its column titles, and under it, the least room left to the page's foot. */
const ASIDE_TOP = 8;
const ASIDE_BOTTOM = 16;
/** Between the line that resizes the table and its aside, so the rows' edge, the line and the aside stand apart. */
const ASIDE_BEFORE = 8;
/** The aside's room beside it: before it, after the line; after it, the page's gutter. */
const ASIDE_EDGES = ASIDE_BEFORE + GUTTER;

/** How many pictures of `size` fit across `width`: one to a row in a list. */
export function columnsFor(width: number, size: TileSize = "medium"): number {
  if (size === "list") return 1;
  const { least, most } = PICTURES[size];
  return Math.max(1, Math.min(most, Math.floor((width - 2 * GUTTER + GAP) / (least + GAP))));
}

/** Draws one item's tile. It gets the item to read, so a tile follows its item as it changes (a recording's length). */
type TileOf<T> = (item: Accessor<T>) => JSX.Element;

/**
 * A page's table, which shows its files at the List size in place of rows of
 * pictures: `titles`, its column titles, after the checkbox that selects every
 * file; `cells`, a file's cells, after its own checkbox, told whether the
 * page's aside shows beside the table (`beside`); `label` saying which file
 * that checkbox selects. The grid keeps the checkboxes, the rows' fill and the
 * headings; the page's cells take the presses and give the widths, the same in
 * its titles and its rows.
 */
export type LibraryTable<T> = {
  titles: () => JSX.Element;
  cells: (item: Accessor<T>, beside: Accessor<boolean>) => JSX.Element;
  label: (item: T) => string;
  /** How wide the cells are at most, in points: the table ends there rather than spreading its columns across a wide page. */
  width: number;
  /** How narrow the cells get, in points: beside the page's aside, the table is dragged no narrower. */
  least: number;
  /** Whether `item` is the one the page's aside shows (`TableAside`): while the aside shows, its row is filled as a selected one's is. */
  current?: (item: T) => boolean;
  /** Moves the aside's file by `by` rows, Up's and Down's keys while the aside shows, returning the one it moved to, which the grid scrolls into view; `null` where it didn't move. */
  step?: (by: number) => string | null;
};

/**
 * A panel beside a page's table, in the room the table leaves: `least` is the
 * narrowest it shows at, below which the table goes alone, and `view` draws it
 * in the room it has, which fits under the header at the top of the page.
 * `tableWidth` is how wide the table was dragged beside it, which the page
 * keeps (`onTableWidth`, as each drag ends); `null` until then, for its
 * columns' own width.
 */
export type TableAside = {
  least: number;
  view: (room: Accessor<{ width: number; height: number }>) => JSX.Element;
  tableWidth: Accessor<number | null>;
  onTableWidth: (width: number) => void;
};

/** A row of the list: the library's, or the table's column titles first. */
type Line<T> = Row<T> | { kind: "titles"; key: string };

/** A section's heading row. */
type Heading = Extract<Row<Item>, { kind: "heading" }>;

/**
 * A section's heading: its checkbox, which selects every file under it, or
 * takes them out when they all are, with a dash while only some are; and its
 * title, which folds the section away and opens it again. Over pictures it
 * sticks under the header's bar while they scroll under it; in a table
 * (`table`), its checkbox sits in the rows' column of them.
 */
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

/** A row of tiles. Empty places fill out a short row, so its pictures are as wide as the rest. */
function TileRow<T extends Item>(props: { items: T[]; columns: number; tile: TileOf<T> }) {
  const places = () => Array.from({ length: props.columns }, (_, index) => props.items[index] ?? null);
  return (
    <View class="flex-row" style={{ gap: GAP - 2 * TILE_PAD, paddingHorizontal: GUTTER - TILE_PAD, paddingBottom: GAP - 2 * TILE_PAD }}>
      <Each of={places()} key={(item, index) => item?.name ?? `empty:${index}`}>
        {(item) => (
          // As padded as a tile, so the room left over is shared out the same.
          <If when={item()} else={<View class="flex-1" style={{ padding: TILE_PAD }} />}>
            {(shown) => props.tile(shown)}
          </If>
        )}
      </Each>
    </View>
  );
}

/**
 * The table's column titles, which stick under the header's bar while the rows
 * scroll: the checkbox that selects every file the filters keep, or takes them
 * all out when they all are, then the page's own.
 */
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

/**
 * A file's row of the table: its checkbox, then the page's cells, filled while
 * the pointer is over it or it is selected, and, while the page's aside shows
 * (`beside`), while it is the one the aside shows.
 */
function TableLine<T extends Item>(props: { library: Library<T>; item: Accessor<T>; table: LibraryTable<T>; beside: boolean }) {
  const on = () => props.library.isSelected(props.item().name);
  const current = () => props.beside && (props.table.current?.(props.item()) ?? false);
  return (
    <View style={{ paddingHorizontal: TABLE_GUTTER }}>
      <TableRow selected={on() || current()}>
        <View class="items-center" style={{ width: CHECK }}>
          <CheckBox label={`Select ${props.table.label(props.item())}`} checked={on()} onChange={() => props.library.pick(props.item().name, "toggle")} />
        </View>
        {props.table.cells(props.item, () => props.beside)}
      </TableRow>
    </View>
  );
}

/**
 * A row of the list, whichever kind it is. Cells are reused, so this follows
 * `row` as it changes, and `table`, the page's table while it shows (at the
 * List size), as the View menu changes; `beside`, whether its aside shows.
 */
function GridRow<T extends Item>(props: { library: Library<T>; row: Line<T>; columns: number; tile: TileOf<T>; table: LibraryTable<T> | undefined; beside: boolean }) {
  const titles = () => (props.row.kind === "titles" ? props.table : undefined);
  const heading = () => (props.row.kind === "heading" ? props.row : undefined);
  const tiles = () => (props.row.kind === "tiles" ? props.row : undefined);
  return (
    <View>
      <If when={titles()}>{(table) => <Titles library={props.library} table={table()} />}</If>
      <If when={heading()}>{(shown) => <SectionHeading library={props.library} row={shown()} table={props.table !== undefined} />}</If>
      <If when={tiles()}>
        {(shown) => (
          <If when={props.table} else={<TileRow items={shown().items} columns={props.columns} tile={props.tile} />}>
            {(table) => (
              <Each of={shown().items} key={(item) => item.name}>
                {(item) => <TableLine library={props.library} item={item} table={table()} beside={props.beside} />}
              </Each>
            )}
          </If>
        )}
      </If>
    </View>
  );
}

/**
 * The selection's keys, bound while the page shows: Control+A selects every
 * file the filters keep, and while anything is selected, Delete deletes it and
 * Escape clears it. `busy` holds them while the page's viewer or player is
 * open, whose own keys are newer and win anyway.
 */
function SelectionKeys<T extends Item>(props: { library: Library<T>; busy: () => boolean; onDelete: (items: T[]) => void }) {
  keymap.bind("mod+a", () => !props.busy() && props.library.selectAll(), { when: "notEditing" });
  return (
    <If when={props.library.selecting()}>
      <ChosenKeys library={props.library} busy={props.busy} onDelete={props.onDelete} />
    </If>
  );
}

/**
 * Up and Down, bound while a table that has a current file shows: they move it
 * a row, held or pressed, and `reveal` scrolls to the row it moved to. `busy`
 * holds them while the page's viewer is open.
 */
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

/** Delete and Escape, bound only while something is selected, so the rest of the time they are the app's. */
function ChosenKeys<T extends Item>(props: { library: Library<T>; busy: () => boolean; onDelete: (items: T[]) => void }) {
  keymap.bind("delete", () => !props.busy() && props.onDelete(props.library.chosen()), { when: "notEditing" });
  keymap.bind("escape", () => !props.busy() && props.library.clearSelection(), { when: "notEditing" });
  return null;
}

/**
 * The bar over the page's foot while some files are selected: select the rest,
 * delete them, in red as the delete question's own button is, with how many on
 * it (`Delete (3)`), or clear the selection.
 */
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

/**
 * A library page's body: `header` (its `LibraryHeader`, given `scroll`) over
 * the `library`'s sections at its size, each item drawn by `tile`, or at the
 * List size by `table` where the page has one, with `aside` beside it where
 * there is room. `empty` shows when the filters
 * leave nothing. `count` names a number of its files ("3 snapshots"),
 * `onDelete` deletes a selection (asking first), and `busy` says while its
 * viewer or player is open. `testID` is what `nsh test` scrolls it by.
 */
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
  /** The page's table while it shows: at the List size, for a page that has one. */
  const table = () => (size() === "list" ? props.table : undefined);
  /** How wide the table is at most, its gutters and checkboxes included; `undefined` while pictures show, which fill the page. */
  const tableWidth = () => {
    const shown = table();
    return shown === undefined ? undefined : 2 * TABLE_GUTTER + CHECK + shown.width;
  };
  /** How narrow the table gets beside the aside, its gutters and checkboxes included. */
  const tableLeast = () => {
    const shown = table();
    return shown === undefined ? 0 : 2 * TABLE_GUTTER + CHECK + shown.least;
  };
  /** Whether the aside shows: beside a table, where the page leaves it room at least `least` wide beside the table at its narrowest. */
  const beside = computed(() => {
    if (props.aside === undefined || table() === undefined) return false;
    return width() - tableLeast() - ASIDE_EDGES >= props.aside.least;
  });
  /** How wide the table is beside the aside: as it was dragged, else its columns' width, and never so wide the aside has less than its least. */
  const tableSize = () => {
    const aside = props.aside;
    if (aside === undefined) return 0;
    const wanted = aside.tableWidth() ?? tableWidth() ?? 0;
    return Math.max(tableLeast(), Math.min(wanted, width() - ASIDE_EDGES - aside.least));
  };
  /** How wide the aside is, as laid out: what the table leaves, as dragged. */
  const asideWidth = signal(0);
  /** The aside's room: as wide as it is laid out, and as tall as what the header leaves at the top of the page, so all of it shows before anything scrolls. */
  const asideRoom = computed(() => ({ width: asideWidth(), height: height() - HEADER_OPEN - ASIDE_TOP - ASIDE_BOTTOM }));
  const columns = computed(() => columnsFor(width(), size()));
  const rows = computed((): Line<T>[] => {
    const shown = rowsOf(props.library.sections(), columns(), props.library.isCollapsed);
    return table() && shown.length > 0 ? [{ kind: "titles", key: "titles" }, ...shown] : shown;
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

  // The selection holds only what the page shows.
  effect(() => {
    props.library.ordered();
    props.library.keepShown();
  });

  let scroller: ScrollRef | undefined;
  /**
   * Scrolls the table's row for the file `name` into view, as little as it
   * takes: every row of a table is `TABLE_ROW` tall, so its place is its
   * index. It shows under the column titles (and the header's bar, once the
   * header has collapsed) and over the selection's bar.
   */
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

  /** The rows, in the scroll view around them, laid out by `style`: in a row of their own, or in the table's panel beside the aside. */
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
      {(row) => <GridRow library={props.library} row={row()} columns={columns()} tile={props.tile} table={table()} beside={beside()} />}
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
        {/* A table ends at its width, with the page's aside beside it where there is room; pictures fill the page. */}
        <If when={props.aside} else={<View class="flex-row items-start">{list(() => ({ flexGrow: 1, flexShrink: 1, flexBasis: 0, minWidth: 0, maxWidth: tableWidth() }))}</View>}>
          {(aside) => (
            // The page's width, and as tall as what it holds (the rows or the aside) rather than the window, in the scroll view's content.
            <ResizablePanelGroup axis="horizontal" class="h-auto" handleClass={LINE_HANDLE.line} handleThumbClass={LINE_HANDLE.thumb} handleAccessibilityLabel="Resize the table">
              {/* Alone, it fills the page, and a table in it ends at its width; beside the aside, it is as wide as it was dragged. */}
              <ResizablePanel size={beside() ? tableSize() : null} minSize={beside() ? tableLeast() : 0} onResize={({ size }) => aside().onTableWidth(size)}>
                {list(() => ({ minWidth: 0, maxWidth: beside() ? undefined : tableWidth() }))}
              </ResizablePanel>
              <ResizablePanel visible={beside()} minSize={aside().least + ASIDE_EDGES}>
                <If when={beside()}>
                  {/* It sticks under the header's bar, reaching it as the header finishes collapsing, while the rows scroll on. */}
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

/** When the filters leave nothing, under the header that says so: `what` match none, and the button that clears them. */
export function NothingMatches(props: { what: string; onClear: () => void }) {
  return (
    <View class="items-center gap-3 px-6 py-12">
      <Text class="text-sm text-muted-foreground">{`No ${props.what} match these filters.`}</Text>
      <Button label="Clear filters" onPress={props.onClear} />
    </View>
  );
}
