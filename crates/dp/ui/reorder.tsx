// Items the user puts in another order by dragging them: the sidebar's
// cameras, by a grip, down a column; a reader's tabs, whole, along a row, or
// along their own row where the tabs wrap onto several (`wraps`). The drag
// moves its item's own animated value on the UI thread (`pan`), so the item
// follows the pointer in the same frame. Once JS hears the drag began
// (`onPanStart`), the item lifts off the list (`look().dragged`) and every
// other item follows that same value through `interpolate`, sliding aside by
// the dragged item's length as the dragged item's middle passes its near edge,
// so no JS runs while the pointer moves. Let go (`onPanEnd`), the item settles
// into the place it reached and the list is told the move; the offsets go in
// the same turn as the list shows its new order, so nothing jumps. Items may
// differ in length (tabs do): each one's place is measured as it lays out.
//
// nsh 0.5.1 has no sortable list, so this is one, made of its pans and
// animated values. What they leave out is dropped rather than worked around:
// the list does not scroll by itself while an item nears its edge (a pan tells
// JS nothing while it moves), an item of a list that wraps moves along its own
// line only (its drag follows one axis, and the others slide along theirs),
// and an item is picked up by its grip or, dragged whole, by a drag past the
// press slop, never by a long press. A touch screen has no hover, so a grip
// also shows on the selected row, and dragging anywhere else on a row with a
// grip still scrolls the list.
import { type AnimatedNode, type AnimatedValue, batch, createAnimatedValue, type JSX, reduceMotion, signal, untracked, View } from "nsh";
import { look } from "./brand";
import { Icon } from "./icon";

/** How far an item slides over, as a share of the distance it slides either side of the point where the dragged item's middle meets its near edge. */
const SLIDE = 0.25;
/** How long an item let go takes to settle into its place, in milliseconds. */
const SETTLE_MS = 150;

/** Which way a list's items run: down a column (rows) or along a row (tabs). */
export type ReorderAxis = "vertical" | "horizontal";

/** The item being dragged: where it was, and the value its drag drives. */
type Drag = { index: number; value: AnimatedValue };

/** Where an item lies along the list: its start and its length, and where its line starts across the list (a wrapped row's top). */
type Span = { start: number; size: number; line: number };

/** The items of one list being put in another order: made by `createReorder`, and given to each of its `Reorderable` items. */
export interface Reorder {
  /** Which way the items run. */
  axis: ReorderAxis;
  /** The index of the item being dragged, or `null`. */
  dragging: () => number | null;
  /** How far the item at `index` stands off its place: `own`, its drag's value, while it is dragged or nothing is; the dragged item's value, made into a slide, while another is. */
  offset: (index: number, own: AnimatedValue) => AnimatedNode;
  /** Where the item at `index` lies in the list, as its layout says. */
  measure: (index: number, bounds: { origin: { x: number; y: number }; size: { width: number; height: number } }) => void;
  /** The item at `index` began a drag. */
  begin: (index: number, own: AnimatedValue) => void;
  /** The item at `index` was let go `moved` points along the list (back toward its start when negative), or `cancelled` by the system. */
  end: (index: number, own: AnimatedValue, moved: number, cancelled: boolean) => void;
}

/** `low` to `high`, as an animated node of `value` within them. */
const within = (value: AnimatedNode, low: number, high: number): AnimatedNode =>
  low === high ? value.mul(0).add(low) : value.interpolate({ inputRange: [low, high], outputRange: [low, high], extrapolate: "clamp" });

/**
 * The order of a list of `count()` items, each a `Reorderable`, running along
 * `axis` (down a column unless said). `within` says which indexes the item at
 * an index may go to (a pinned tab among the pinned), every one unless given.
 * While `wraps()` says the list wraps onto more lines (a row of tabs in
 * several rows), an item goes only to the places on its own line.
 * `onMove` is called with where an item was and where it was let go, once it
 * settles there; it resolves with whether the move was made, and the list
 * shows the new order by the time it does. A move not made settles the item
 * back where it was.
 */
export function createReorder(options: {
  count: () => number;
  onMove: (from: number, to: number) => Promise<boolean>;
  axis?: ReorderAxis;
  within?: (index: number) => [number, number];
  wraps?: () => boolean;
}): Reorder {
  const axis = options.axis ?? "vertical";
  const drag = signal<Drag | null>(null);
  /** Each item's place along the list, by index: an item reports again whenever it moves. */
  const spans = new Map<number, Span>();

  const span = (index: number): Span => spans.get(index) ?? { start: 0, size: 0, line: 0 };
  const end = (index: number) => span(index).start + span(index).size;
  /** Whether the items at `a` and `b` are both measured and on one line: always, in a list that doesn't wrap. */
  const together = (a: number, b: number) => spans.has(a) && spans.has(b) && (!options.wraps?.() || Math.abs(span(a).line - span(b).line) < 1);
  /** The first and last index the item at `index` may go to: within its line, in a list that wraps. */
  const range = (index: number): [number, number] => {
    const last = options.count() - 1;
    const [given, most] = options.within?.(index) ?? [0, last];
    let low = Math.max(0, Math.min(given, index));
    let high = Math.min(last, Math.max(most, index));
    if (options.wraps?.()) {
      // The items of a line are a run of indexes, so the range closes in on the run around `index`.
      while (low < index && !together(low, index)) low++;
      while (high > index && !together(high, index)) high--;
    }
    return [low, high];
  };
  /** How far the others slide as the item at `from` passes them: its length and the gap after it (or before it, last on its line). */
  const shift = (from: number) => {
    const gap = together(from, from + 1) ? span(from + 1).start - end(from) : together(from, from - 1) ? span(from).start - end(from - 1) : 0;
    return span(from).size + gap;
  };
  /** How far the item at `from` has gone when its middle meets the near edge of the item at `index`. */
  const meet = (index: number, from: number) => {
    const middle = span(from).start + span(from).size / 2;
    return index > from ? span(index).start - middle : end(index) - middle;
  };
  /** How far the item at `from` goes to take the place of the item at `to`. */
  const travel = (from: number, to: number) => (to > from ? end(to) - end(from) : to < from ? span(to).start - span(from).start : 0);

  /** Moves `value` to `to`, at once when the system asks for less motion. */
  const settle = async (value: AnimatedValue, to: number) => {
    if (reduceMotion()) value.set(to);
    else await value.timing(to, { duration: SETTLE_MS, easing: "easeOut" });
  };

  /** Puts every item back in its place: the list shows the order it has by now. */
  const finish = (value: AnimatedValue) =>
    batch(() => {
      drag.set(null);
      value.set(0);
    });

  return {
    axis,
    dragging: () => drag()?.index ?? null,
    offset: (index, own) => {
      const current = drag();
      if (current === null) return own;
      const { index: from, value } = current;
      const [low, high] = range(from);
      if (index === from) return within(own, travel(from, low), travel(from, high));
      if (index < low || index > high) return own;
      const at = meet(index, from);
      const length = shift(from);
      const ramp = length * SLIDE;
      return value.interpolate({
        inputRange: [at - ramp, at + ramp],
        outputRange: index > from ? [0, -length] : [length, 0],
        extrapolate: "clamp",
      });
    },
    measure: (index, bounds) =>
      spans.set(
        index,
        axis === "vertical"
          ? { start: bounds.origin.y, size: bounds.size.height, line: bounds.origin.x }
          : { start: bounds.origin.x, size: bounds.size.width, line: bounds.origin.y },
      ),
    begin: (index, own) => {
      if (untracked(drag) !== null || options.count() < 2 || !spans.has(index)) return;
      const [low, high] = range(index);
      if (low < high) drag.set({ index, value: own });
    },
    end: (index, own, moved, cancelled) => {
      const current = untracked(drag);
      if (current === null || current.index !== index) {
        own.set(0);
        return;
      }
      const [low, high] = range(index);
      // The item stops where it is drawn, within its range, and settles from there.
      const gone = Math.max(travel(index, low), Math.min(travel(index, high), moved));
      own.set(gone);
      let to = index;
      if (!cancelled) {
        for (let next = index + 1; next <= high && meet(next, index) < gone; next++) to = next;
        for (let next = index - 1; next >= low && meet(next, index) > gone; next--) to = next;
      }
      void (async () => {
        await settle(own, travel(index, to));
        if (to !== index && !(await options.onMove(index, to))) await settle(own, 0);
        finish(own);
      })();
    },
  };
}

/**
 * One item of a list that `reorder` puts in order, at `index`: it lifts and
 * follows the pointer while dragged, and slides aside while another passes it.
 * `children` gets the grip to place in the item, which shows while the pointer
 * is over the item, and always while `shown` (a selected row, as a touch
 * screen has no hover). With `whole`, the item is dragged by itself, a press
 * on it left as a press, and `children` gets no grip. `label` names the item
 * to a screen reader and a test (`grip:<label>`).
 */
export function Reorderable(props: {
  reorder: Reorder;
  index: number;
  label: string;
  shown?: boolean;
  whole?: boolean;
  children: (grip: JSX.Element | null) => JSX.Element;
}) {
  const own = createAnimatedValue(0);
  const vertical = props.reorder.axis === "vertical";
  const lifted = () => props.reorder.dragging() === props.index;
  const pan = vertical ? { y: own } : { x: own };
  const begin = () => props.reorder.begin(props.index, own);
  const end = (event: { translation: { x: number; y: number }; cancelled: boolean }) =>
    props.reorder.end(props.index, own, vertical ? event.translation.y : event.translation.x, event.cancelled);
  const grip = props.whole ? null : (
    <View
      testID={`grip:${props.label}`}
      accessibilityLabel={`Drag ${props.label} to move it`}
      class={`h-6 w-5 items-center justify-center ${props.shown || lifted() ? "opacity-100" : "opacity-0 group-hover/reorder:opacity-100"}`}
      style={{ cursor: lifted() ? "grabbing" : "grab" }}
      pan={pan}
      onPanStart={begin}
      onPanEnd={end}
    >
      <Icon name={vertical ? "grip-vertical" : "grip"} size={14} tint="muted-foreground" />
    </View>
  );
  return (
    <View
      class={`group/reorder ${lifted() ? look().dragged.class : ""}`}
      style={{
        ...(lifted() ? look().dragged.style : {}),
        zIndex: lifted() ? 1 : 0,
        transform: [vertical ? { translateY: props.reorder.offset(props.index, own) } : { translateX: props.reorder.offset(props.index, own) }],
      }}
      onLayout={(event) => props.reorder.measure(props.index, event.bounds)}
      pan={props.whole ? pan : undefined}
      onPanStart={props.whole ? begin : undefined}
      onPanEnd={props.whole ? end : undefined}
    >
      {props.children(grip)}
    </View>
  );
}
