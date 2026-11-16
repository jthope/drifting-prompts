// The dashboard's charts and cards, over nsh's plot (`nsh/base/plot`), which
// draws the marks in Rust, follows the pointer and the keys, and animates on
// the platform's animator: no JS runs as the pointer moves. These bring the
// look: the theme's chart colors (`chart-1` to `chart-5`: `theme.json` gives
// the Default themes theirs, and every theme file has its own), its lines and
// muted text, and the app's card (`card.tsx`), as Settings' groups have.
import { type JSX, reduceMotion, signal, Text, View } from "nsh";
import { Area, Bar, Cell, Grid, Line, Pie, Plot, PlotAxis, type PlotMotion } from "nsh/base/plot";
import { look } from "./brand";
import { Card, cardClass } from "./card";
import { Icon, type IconName } from "./icon";
import { popupSurface } from "./menu";
import { Press } from "./press";

const tabular = { fontVariant: ["tabular-nums" as const] };

/** How a series paints: as a plot's mark (`fill`) and as a view, a legend's dot (`bg`). */
export type Paint = { fill: string; bg: string };

/** The colors of a chart's series, in order: the theme's chart colors, written out whole. */
export const SERIES: Paint[] = [
  { fill: "fill-chart-1", bg: "bg-chart-1" },
  { fill: "fill-chart-2", bg: "bg-chart-2" },
  { fill: "fill-chart-3", bg: "bg-chart-3" },
  { fill: "fill-chart-4", bg: "bg-chart-4" },
  { fill: "fill-chart-5", bg: "bg-chart-5" },
];

/**
 * What is left of a whole: free space, and a bar's empty track. A faint wash
 * of the muted text rather than `muted`, which some themes make their card's
 * own color, so it shows on every card.
 */
export const REST: Paint = { fill: "fill-muted-foreground/15", bg: "bg-muted-foreground/15" };

/**
 * Each chart color as a wash, in `SERIES`' order: what is left of a whole the
 * color stands for, such as a drive's free space beside its files in the full
 * color. Lighter, so it reads as room rather than as something there.
 */
export const WASHES: Paint[] = [
  { fill: "fill-chart-1/30", bg: "bg-chart-1/30" },
  { fill: "fill-chart-2/30", bg: "bg-chart-2/30" },
  { fill: "fill-chart-3/30", bg: "bg-chart-3/30" },
  { fill: "fill-chart-4/30", bg: "bg-chart-4/30" },
  { fill: "fill-chart-5/30", bg: "bg-chart-5/30" },
];

/** Each chart color as a sparkline paints it, in `SERIES`' order: its line, and a faint wash under it. */
const TRENDS = [
  { line: "stroke-chart-1", under: "fill-chart-1/10" },
  { line: "stroke-chart-2", under: "fill-chart-2/10" },
  { line: "stroke-chart-3", under: "fill-chart-3/10" },
  { line: "stroke-chart-4", under: "fill-chart-4/10" },
  { line: "stroke-chart-5", under: "fill-chart-5/10" },
];

/** The plots' motion: drawing in when shown and moving to new values, none when the system asks for less. */
function motion(): PlotMotion | null {
  if (reduceMotion()) return null;
  return { appear: { duration: 500, easing: "easeOut" }, update: { duration: 300, easing: "easeOut" }, enter: { duration: 120 }, exit: { duration: 120 } };
}

/** The card a chart or a list sits in: a title, a line about it (`detail`), and a button at its end (`action`). */
export function ChartCard(props: { title: string; detail?: string; action?: JSX.Element; class?: string; children: JSX.Element }) {
  return (
    <Card class={`min-w-0 gap-4 p-4 ${props.class ?? ""}`}>
      <View class="flex-row items-start gap-3">
        <View class="min-w-0 flex-1 gap-0.5">
          <Text role="header" class="text-sm font-semibold text-card-foreground" style={{ numberOfLines: 1 }}>
            {props.title}
          </Text>
          <If when={props.detail}>
            <Text class="text-xs text-muted-foreground" style={{ numberOfLines: 2 }}>
              {props.detail}
            </Text>
          </If>
        </View>
        {props.action}
      </View>
      {props.children}
    </Card>
  );
}

/**
 * A number on the dashboard: its icon, in the theme's `chartInk`, on a chip of
 * one of the theme's first four chart colors (`chart`, 0 for `chart-1`), what
 * it counts (`label`), the number (`value`) and a line about it (`detail`). A
 * number about a series takes its color, so the tile and the charts agree.
 * With `trend`, a sparkline of it in the same color fills the rest of the
 * number's line. With `onPress` it opens the page it sums up, sinking while
 * held.
 */
export function StatTile(props: {
  icon: IconName;
  chart: number;
  label: string;
  value: string;
  detail?: string;
  trend?: number[];
  testID?: string;
  onPress?: () => void;
}) {
  const chip = () => SERIES[props.chart % 4].bg;
  const body = () => (
    <>
      <View class="flex-row items-center gap-3">
        <View class={`size-8 items-center justify-center rounded-lg ${chip()}`}>
          <Icon name={props.icon} size={16} color={look().chartInk} />
        </View>
        <Text class="min-w-0 flex-1 text-sm font-medium text-muted-foreground" style={{ numberOfLines: 1 }}>
          {props.label}
        </Text>
      </View>
      <View class="gap-0.5">
        <View class="flex-row items-end gap-4">
          <Text class="text-2xl font-bold text-card-foreground" style={{ letterSpacing: -0.4, numberOfLines: 1, ...tabular }}>
            {props.value}
          </Text>
          <If when={props.trend}>{(trend) => <Sparkline values={trend()} chart={props.chart} />}</If>
        </View>
        <Text class="text-xs text-muted-foreground" style={{ numberOfLines: 1, ...tabular }}>
          {props.detail ?? " "}
        </Text>
      </View>
    </>
  );
  // Every tile is the same two views, pressable or not: an unpadded one taking its share of the
  // row, and the card filling it. A padded view taking the share itself would start from its
  // padding where the others start from nothing, and come out wider. The shadow is the card's
  // (`Card`), on its rounded view, not the wrapper's, which would draw it square.
  return (
    <View class="min-w-44 flex-1">
      <If
        when={props.onPress}
        else={
          <Card class="grow gap-3 p-4" testID={props.testID}>
            {body()}
          </Card>
        }
      >
        {(press) => (
          <Press
            label={`${props.label}: ${props.value}`}
            testID={props.testID}
            class={`grow gap-3 p-4 ${cardClass()} hover:bg-accent ${look().rings.visible}`}
            style={look().surface}
            onPress={() => press()()}
          >
            {body()}
          </Press>
        )}
      </If>
    </View>
  );
}

/**
 * A small line of `values` in a chart color (`chart`, 0 for `chart-1`), the
 * oldest first, over a faint wash: how a number got where it is. It takes no
 * pointer or focus, so a press goes to the tile it sits in, and draws in when
 * the number of values changes (another period).
 */
export function Sparkline(props: { values: number[]; chart: number }) {
  const paint = () => TRENDS[props.chart % TRENDS.length];
  const rows = () => props.values.map((value) => ({ value }));
  // Room above the highest point and below zero, so the line's stroke isn't cut at the plot's edges.
  const domain = (): [number, number] => {
    const most = Math.max(1, ...props.values);
    return [-most * 0.08, most * 1.12];
  };
  return (
    <Plot
      data={rows()}
      y="value"
      yScale={{ type: "linear", domain: domain() }}
      interactive={false}
      class="mb-1 h-7 min-w-0 max-w-48 flex-1"
      motion={motion()}
      appearGeneration={props.values.length}
    >
      <Area y0={0} curve="linear" class={paint().under} />
      <Line curve="linear" class={`${paint().line} stroke-2`} />
    </Plot>
  );
}

/** A legend's line: the series' dot, its name and its value. */
export function LegendItem(props: { paint: Paint; label: string; value?: string }) {
  return (
    <View class="flex-row items-center gap-2">
      <View class={`size-2.5 rounded-sm ${props.paint.bg}`} />
      <Text class="min-w-0 flex-1 text-xs text-muted-foreground" style={{ numberOfLines: 1 }}>
        {props.label}
      </Text>
      <If when={props.value}>
        <Text class="text-xs font-medium text-card-foreground" style={tabular}>
          {props.value}
        </Text>
      </If>
    </View>
  );
}

/** One series of `StackedBars`: what it counts in a row, its name, and its paint. */
export type BarSeries<T> = { value: (row: T) => number; name: string; paint: Paint };

/**
 * Upright bars, one band per row, its series piled up in order, over the
 * theme's grid lines, with the rows' names under them (`tick`, on every
 * `tickEvery`th row counting back from the last, so the newest is always
 * named, or on the rows `ticks` lists) and whole numbers beside them. The
 * pointer, or the arrow keys once it has the focus, picks a row out and shows
 * `tooltip` of it.
 */
export function StackedBars<T>(props: {
  rows: T[];
  series: BarSeries<T>[];
  tick: (row: T) => string;
  tickEvery?: number;
  ticks?: number[];
  tooltip: (row: T) => JSX.Element;
  label: string;
  testID?: string;
}) {
  const hovered = signal<number | null>(null);
  const row = () => {
    const index = hovered();
    return index === null ? undefined : props.rows[index];
  };
  const surface = () => popupSurface("gap-1 rounded-md px-2.5 py-1.5");
  const tickValues = () => {
    if (props.ticks) return props.ticks;
    const every = Math.max(1, props.tickEvery ?? 1);
    const last = props.rows.length - 1;
    return props.rows.map((_, index) => index).filter((index) => (last - index) % every === 0);
  };
  return (
    <Plot
      data={props.rows}
      xScale={{ type: "band", paddingInner: 0.3, paddingOuter: 0.15 }}
      class={`mb-6 ml-8 h-44 rounded-sm ${look().rings.visible}`}
      testID={props.testID}
      motion={motion()}
      appearGeneration={props.rows.length}
      accessibilityLabel={props.label}
      accessibilityValue={{ text: row() ? `${props.tick(row() as T)}: ${props.series.map((each) => `${each.value(row() as T)} ${each.name}`).join(", ")}` : null }}
      crossLineClass="w-full rounded-sm bg-foreground/5"
      tooltipPlacement="pointer"
      tooltipOffset={10}
      tooltipContent={
        <If when={row()}>
          {(shown) => (
            <View class={surface().class} style={surface().style}>
              {props.tooltip(shown())}
            </View>
          )}
        </If>
      }
      onHoverChange={(event) => hovered.set(event.index)}
    >
      <Grid class="stroke-border" ticks={4} />
      <Each of={props.series} key={(each) => each.name}>
        {(each) => <Bar value={(data: T) => each().value(data)} stack="all" cornerRadii={2} class={each().paint.fill} />}
      </Each>
      <PlotAxis placement="left" ticks={4} format={(value) => (Number.isInteger(value) ? value.toLocaleString() : "")} labelClass="pr-2 text-xs text-muted-foreground" />
      <PlotAxis
        placement="bottom"
        tickValues={tickValues()}
        format={(index) => {
          const shown = props.rows[index];
          return shown === undefined ? "" : props.tick(shown);
        }}
        labelClass="pt-1.5 text-xs text-muted-foreground"
      />
    </Plot>
  );
}

/**
 * A grid of cells, `columns` across and `lines` down, each row of `rows` the
 * cell of its column (`column`) and line (`line`), named by `columnTick` and
 * `lineTick`, `columnTicks` the columns named (all without). Its color is how
 * much the row holds (`value`) along the theme's `heat` ramp of a chart color
 * (`chart`, 0 for `chart-1`): from a wash for the least above nothing to the
 * full color for the most; a cell holding nothing is a bar's empty track. The
 * pointer, or the arrow keys once it has the focus, outlines a cell and shows
 * `tooltip` of it.
 */
export function Heatmap<T>(props: {
  rows: T[];
  columns: string[];
  lines: string[];
  column: (row: T) => string;
  line: (row: T) => string;
  value: (row: T) => number;
  columnTick: (index: number) => string;
  lineTick: (index: number) => string;
  columnTicks?: number[];
  chart: number;
  tooltip: (row: T) => JSX.Element;
  describe: (row: T) => string;
  label: string;
  testID?: string;
}) {
  const hovered = signal<number | null>(null);
  const row = () => {
    const index = hovered();
    return index === null ? undefined : props.rows[index];
  };
  const surface = () => popupSurface("gap-1 rounded-md px-2.5 py-1.5");
  const heat = () => look().heat(props.chart);
  const most = () => Math.max(1, ...props.rows.map(props.value));
  // Where a cell's value falls along the ramp's three stops: nothing on the first, the empty
  // track; the least above nothing on the middle one, a wash, and the most on the last, so even
  // one event stands apart from none.
  const shade = (value: number) => (value <= 0 ? 0 : most() <= 1 ? 1 : 0.5 + (0.5 * (value - 1)) / (most() - 1));
  return (
    <Plot
      data={props.rows}
      xScale={{ type: "band", domain: props.columns, paddingInner: 0.12 }}
      yScale={{ type: "band", domain: props.lines, paddingInner: 0.16 }}
      colorScale={{ type: "sequential", domain: [0, 1], range: [heat().empty, heat().low, heat().high] }}
      class={`mb-6 ml-10 h-44 rounded-sm ${look().rings.visible}`}
      testID={props.testID}
      motion={motion()}
      appearGeneration={props.rows.length}
      accessibilityLabel={props.label}
      accessibilityValue={{ text: row() ? props.describe(row() as T) : null }}
      tooltipPlacement="pointer"
      tooltipOffset={10}
      tooltipContent={
        <If when={row()}>
          {(shown) => (
            <View class={surface().class} style={surface().style}>
              {props.tooltip(shown())}
            </View>
          )}
        </If>
      }
      onHoverChange={(event) => hovered.set(event.index)}
    >
      <Cell
        x={(data: T) => props.column(data)}
        y={(data: T) => props.line(data)}
        value={(data: T) => shade(props.value(data))}
        cornerRadii={2}
        hoverCellClass="rounded-sm outline-2 outline-foreground"
      />
      <PlotAxis placement="left" tickValues={props.lines.map((_, index) => index)} format={(index) => props.lineTick(index)} labelClass="pr-2 text-xs text-muted-foreground" />
      <PlotAxis
        placement="bottom"
        tickValues={props.columnTicks ?? props.columns.map((_, index) => index)}
        format={(index) => props.columnTick(index)}
        labelClass="pt-1.5 text-xs text-muted-foreground"
      />
    </Plot>
  );
}

/** One slice of a `Donut`: its name, its size and its paint. */
export type Slice = { name: string; value: number; paint: Paint };

/**
 * A ring of `slices`, in order clockwise from the top, with `center` in its
 * hole. The pointer over a slice lifts it, and `onHover` hears which, so the
 * center can say what it is.
 */
export function Donut(props: { slices: Slice[]; center: JSX.Element; label: string; onHover?: (slice: Slice | undefined) => void; size?: number }) {
  const size = () => props.size ?? 148;
  return (
    <View style={{ width: size(), height: size() }}>
      <Plot
        data={props.slices}
        class={`flex-1 rounded-full ${look().rings.visible}`}
        motion={motion()}
        appearGeneration={props.slices.length}
        accessibilityLabel={props.label}
        onHoverChange={(event) => props.onHover?.(event.index === null ? undefined : props.slices[event.index])}
      >
        <Pie value="value" innerRadius="68%" padAngle={0.02} cornerRadius={3} hoverLift={3} sliceClass={(slice: Slice) => slice.paint.fill} />
      </Plot>
      <View class="absolute inset-0 items-center justify-center" style={{ pointerEvents: "none" }}>
        {props.center}
      </View>
    </View>
  );
}

/**
 * A bar split in parts, in proportion to `max` (the longest bar's total, so
 * bars in a list compare), each part in its series' paint. Views, not a plot:
 * a list of these reads as one chart, each row with its own name.
 */
export function SplitBar(props: { parts: { value: number; paint: Paint }[]; max: number }) {
  const total = () => props.parts.reduce((sum, part) => sum + part.value, 0);
  const share = (value: number) => (props.max > 0 ? value / props.max : 0);
  return (
    <View class={`h-2 flex-row overflow-hidden rounded-full ${REST.bg}`}>
      <Each of={props.parts.map((part, index) => ({ ...part, index }))} key={(part) => String(part.index)}>
        {(part) => <View class={part().paint.bg} style={{ flexGrow: share(part().value), flexBasis: 0 }} />}
      </Each>
      <View style={{ flexGrow: Math.max(0, 1 - share(total())), flexBasis: 0 }} />
    </View>
  );
}
