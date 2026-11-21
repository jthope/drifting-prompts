// Dragging items into order. nsh has no sortable list, so this is one, of its pans and animated values:
// no JS runs while the pointer moves, so the list can't scroll itself during a drag.
import { type AnimatedNode, type AnimatedValue, batch, createAnimatedValue, type JSX, reduceMotion, signal, untracked, View } from "nsh";
import { look } from "./brand";
import { Icon } from "./icon";

/** The slide's ramp either side of the meeting point, as a share of the dragged item's length. */
const SLIDE = 0.25;
const SETTLE_MS = 150;

/** Which way a list's items run: down a column (rows) or along a row (tabs). */
export type ReorderAxis = "vertical" | "horizontal";

type Drag = { index: number; value: AnimatedValue };

type Span = { start: number; size: number; line: number };

/** One list's items being put in another order. */
export interface Reorder {
  axis: ReorderAxis;
  dragging: () => number | null;
  /** How far the item at `index` stands off its place. */
  offset: (index: number, own: AnimatedValue) => AnimatedNode;
  /** Where the item at `index` lies, from its layout. */
  measure: (index: number, bounds: { origin: { x: number; y: number }; size: { width: number; height: number } }) => void;
  /** The item at `index` began a drag. */
  begin: (index: number, own: AnimatedValue) => void;
  /** The item at `index` was let go `moved` points along the list, or `cancelled`. */
  end: (index: number, own: AnimatedValue, moved: number, cancelled: boolean) => void;
}

const within = (value: AnimatedNode, low: number, high: number): AnimatedNode =>
  low === high ? value.mul(0).add(low) : value.interpolate({ inputRange: [low, high], outputRange: [low, high], extrapolate: "clamp" });

/** The order of a list of `Reorderable`s; `onMove` resolves, once the list shows its new order, with whether it moved. */
export function createReorder(options: {
  count: () => number;
  onMove: (from: number, to: number) => Promise<boolean>;
  axis?: ReorderAxis;
  within?: (index: number) => [number, number];
  wraps?: () => boolean;
}): Reorder {
  const axis = options.axis ?? "vertical";
  const drag = signal<Drag | null>(null);
  const spans = new Map<number, Span>();

  const span = (index: number): Span => spans.get(index) ?? { start: 0, size: 0, line: 0 };
  const end = (index: number) => span(index).start + span(index).size;
  const together = (a: number, b: number) => spans.has(a) && spans.has(b) && (!options.wraps?.() || Math.abs(span(a).line - span(b).line) < 1);
  const range = (index: number): [number, number] => {
    const last = options.count() - 1;
    const [given, most] = options.within?.(index) ?? [0, last];
    let low = Math.max(0, Math.min(given, index));
    let high = Math.min(last, Math.max(most, index));
    if (options.wraps?.()) {
      // A line's items are a run of indexes.
      while (low < index && !together(low, index)) low++;
      while (high > index && !together(high, index)) high--;
    }
    return [low, high];
  };
  const shift = (from: number) => {
    const gap = together(from, from + 1) ? span(from + 1).start - end(from) : together(from, from - 1) ? span(from).start - end(from - 1) : 0;
    return span(from).size + gap;
  };
  const meet = (index: number, from: number) => {
    const middle = span(from).start + span(from).size / 2;
    return index > from ? span(index).start - middle : end(index) - middle;
  };
  const travel = (from: number, to: number) => (to > from ? end(to) - end(from) : to < from ? span(to).start - span(from).start : 0);

  const settle = async (value: AnimatedValue, to: number) => {
    if (reduceMotion()) value.set(to);
    else await value.timing(to, { duration: SETTLE_MS, easing: "easeOut" });
  };

  /** By now the list shows its new order, so the offsets go in the same turn. */
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

/** One item of a `Reorder`'s list; `children` gets its grip (`grip:<label>` to a test), none with `whole`. */
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
