// What a page's table does with its rows, whatever they are: the order a
// column title sorts them in, which of them are selected, and the page of them
// shown. The Cameras page (`roster.ts`) and the Groups page (`grouplist.ts`)
// each make their own with `listingOf`, over the rows their search and filters
// keep. Like `library.ts`'s, what it makes are module-level signals, so a
// page's header, table, selection bar and dialogs share them, and coming back
// to the page finds them as they were left.
import { type Accessor, computed, type Signal, signal, untracked } from "nsh";

/** What a table is sorted by: a column's key and which way; `null` for the saved order. */
export type Sort<K extends string> = { key: K; descending: boolean } | null;

/** How a press of a row changes the selection: that row alone, that row in or out, or every row from the last one pressed to it. */
export type Pick = "only" | "toggle" | "range";

/** The kind of pick a press with these keys held makes, as Explorer has it. */
export const pickOf = (keys: { shift: boolean; control: boolean; platform: boolean }): Pick => (keys.shift ? "range" : keys.control || keys.platform ? "toggle" : "only");

/** How many rows a page can hold, as the footer offers them. */
export const PAGE_SIZES = [10, 25, 50, 100];

/**
 * "Camera 2" before "Camera 10", and 10.0.0.9 before 10.0.0.20, whatever the
 * case: runs of digits compare as numbers, the rest as text. By hand, because
 * the engine nsh runs on has no `Intl.Collator`.
 */
export function byText(a: string, b: string): number {
  const runs = (text: string) => text.toLowerCase().match(/\d+|\D+/g) ?? [];
  const [left, right] = [runs(a), runs(b)];
  for (let at = 0; at < Math.min(left.length, right.length); at += 1) {
    const [x, y] = [left[at], right[at]];
    const order = /^\d/.test(x) && /^\d/.test(y) ? Number(x) - Number(y) : x.localeCompare(y);
    if (order !== 0) return order;
  }
  return left.length - right.length;
}

/**
 * The name a copy of `name` takes, as Duplicate makes one: "Garage copy", then
 * "Garage copy 2" when that is taken (`taken`, every name there is). A copy of
 * a copy is named after the original: "Garage copy 2", not "Garage copy copy".
 */
export function copyName(name: string, taken: Iterable<string>): string {
  const base = name.trim().replace(/ copy(?: \d+)?$/, "");
  const names = new Set([...taken].map((each) => each.trim()));
  let copy = `${base} copy`;
  for (let number = 2; names.has(copy); number += 1) copy = `${base} copy ${number}`;
  return copy;
}

/** One page's table: its order, its selection and its pages, over the rows its search and filters keep. */
export type Listing<R extends { id: string }, K extends string> = {
  /** What the table is sorted by; `null` for the saved order. */
  sort: Signal<Sort<K>>;
  /** A column title's press: by that column, then the other way, then back to the saved order. */
  sortBy: (key: K) => void;
  /** The rows the search and filters keep, in the chosen order. */
  shown: Accessor<R[]>;
  /** The ids of the selected rows. Only rows the table shows stay selected (`keepShown`). */
  selected: Signal<string[]>;
  /** The selected rows, in the table's order: what every action on the selection acts on. */
  chosen: Accessor<R[]>;
  /** A press of row `id`, as `how` says. */
  pick: (id: string, how: Pick) => void;
  /** Selects every row on the page shown, or, when every one already is, takes them out: the title row's checkbox. Rows selected on other pages stay as they are. */
  togglePage: () => void;
  /** Selects every row the search and filters keep, on every page: Control+A. */
  selectAll: () => void;
  /** Selects `ids` alone, as rows just made are: the first is where a Shift press runs from. */
  selectOnly: (ids: string[]) => void;
  clearSelection: () => void;
  /** Drops from the selection what the table no longer shows: filtered out, or deleted. The page calls it as the rows change. */
  keepShown: () => void;
  /** How many rows a page holds. */
  pageSize: Signal<number>;
  /** How many pages the rows shown take: at least one, empty or not. */
  pageCount: Accessor<number>;
  /**
   * The page shown, from 0: the one turned to, unless the search, a filter,
   * the order or the page size changed since, which start again at the first;
   * and never past the last, which deleting rows can leave it.
   */
  currentPage: Accessor<number>;
  /** Turns to page `index`, from 0, kept within the pages there are. */
  turnTo: (index: number) => void;
  /** The rows on the page shown, in the table's order. */
  onPage: Accessor<R[]>;
  /** Which of the rows shown the page holds, from 1, and how many there are: the footer's "11–20 of 42". */
  pageSpan: Accessor<{ from: number; to: number; total: number }>;
};

/**
 * A table over `kept`, the rows the page's search and filters keep, in the
 * saved order. `compare` says how two rows compare by a column, before the
 * direction is applied; rows `blank` says have nothing in that column go last
 * either way, so they are found in one place. `filters` is what the search and
 * filters are set to: a change to it starts the pages again at the first.
 */
export function listingOf<R extends { id: string }, K extends string>(options: {
  kept: Accessor<R[]>;
  compare: (a: R, b: R, key: K) => number;
  blank?: (row: R, key: K) => boolean;
  filters: Accessor<unknown>;
}): Listing<R, K> {
  const blank = options.blank ?? (() => false);

  // The order.

  const sort = signal<Sort<K>>(null);

  const sortBy = (key: K) => {
    const now = sort();
    if (now?.key !== key) sort.set({ key, descending: false });
    else if (!now.descending) sort.set({ key, descending: true });
    else sort.set(null);
  };

  const shown = computed((): R[] => {
    const kept = options.kept();
    const by = sort();
    if (by === null) return kept;
    const sign = by.descending ? -1 : 1;
    // Rows that compare the same keep the saved order between them.
    return kept
      .map((row, index) => ({ row, index }))
      .sort((a, b) => Number(blank(a.row, by.key)) - Number(blank(b.row, by.key)) || sign * options.compare(a.row, b.row, by.key) || a.index - b.index)
      .map((each) => each.row);
  });

  // Pages.

  const pageSize = signal(25);

  /** What the page turned to is good for: a new search, filter, order or page size starts again at the first. */
  const pagingKey = computed(() => JSON.stringify([options.filters(), sort(), pageSize()]));

  /** The page last turned to, from 0, with the paging it was turned to under. */
  const turnedTo = signal({ key: "", index: 0 });

  const pageCount = computed(() => Math.max(1, Math.ceil(shown().length / pageSize())));

  const currentPage = computed(() => {
    const turned = turnedTo();
    return turned.key === pagingKey() ? Math.min(turned.index, pageCount() - 1) : 0;
  });

  const turnTo = (index: number) => {
    turnedTo.set({ key: untracked(pagingKey), index: Math.max(0, Math.min(index, untracked(pageCount) - 1)) });
  };

  const onPage = computed(() => shown().slice(currentPage() * pageSize(), (currentPage() + 1) * pageSize()));

  const pageSpan = computed(() => {
    const total = shown().length;
    const first = currentPage() * pageSize();
    return { from: total === 0 ? 0 : first + 1, to: Math.min(total, first + pageSize()), total };
  });

  // The selection.

  const selected = signal<string[]>([]);
  /** Where a Shift press runs from: the row last pressed on its own or with Control. */
  let anchor: string | null = null;

  const chosen = computed(() => {
    const ids = new Set(selected());
    return shown().filter((row) => ids.has(row.id));
  });

  const pick = (id: string, how: Pick) => {
    const list = shown().map((row) => row.id);
    if (how === "range" && anchor !== null && list.includes(anchor)) {
      const from = list.indexOf(anchor);
      const to = list.indexOf(id);
      const span = list.slice(Math.min(from, to), Math.max(from, to) + 1);
      // Added to what is selected, as Shift with Control would be: a range never drops a row picked by hand.
      selected.set([...new Set([...selected(), ...span])]);
      return;
    }
    anchor = id;
    if (how === "toggle") selected.set(selected().includes(id) ? selected().filter((each) => each !== id) : [...selected(), id]);
    else selected.set([id]);
  };

  const togglePage = () => {
    const list = onPage().map((row) => row.id);
    const all = list.length > 0 && list.every((id) => selected().includes(id));
    selected.set(all ? selected().filter((id) => !list.includes(id)) : [...new Set([...selected(), ...list])]);
  };

  const selectAll = () => selected.set(shown().map((row) => row.id));

  const selectOnly = (ids: string[]) => {
    selected.set(ids);
    anchor = ids[0] ?? null;
  };

  const clearSelection = () => {
    selected.set([]);
    anchor = null;
  };

  const keepShown = () => {
    const ids = new Set(shown().map((row) => row.id));
    const now = untracked(selected);
    if (now.some((id) => !ids.has(id))) selected.set(now.filter((id) => ids.has(id)));
  };

  return {
    sort,
    sortBy,
    shown,
    selected,
    chosen,
    pick,
    togglePage,
    selectAll,
    selectOnly,
    clearSelection,
    keepShown,
    pageSize,
    pageCount,
    currentPage,
    turnTo,
    onPage,
    pageSpan,
  };
}
