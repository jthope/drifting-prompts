// The header of the snapshots and videos pages: the dashboard's collapsing
// header (`ui/collapsing.tsx`), at its height, with the page's count and what
// the filters keep under the title, its stats in a column at the masthead's
// end, and its filters and buttons in the bar that stays as the pictures
// scroll under it. The Cameras page's header shows its stats and what records
// with the same `Stats` and `Live`.
import { type AnimatedValue, type JSX, Scroll, Text, View } from "nsh";
import { CollapsingHeader, collapsedness, Icon, type IconName, LiveDot } from "../../../dp/ui";
import { LibraryFilters } from "./filters";
import { countLabel, type Item, type Library } from "./library";

/** One of a header's stats: what it counts (`label`), the figure (`value`, with its units), and an icon for it. */
export type MastheadStat = { icon: IconName; label: string; value: string };

const tabular = { fontVariant: ["tabular-nums" as const] };

/** A number in a figure, with the marks a locale groups digits and writes decimals with: "1,234", "2.4", "1 234". */
const NUMBER = /(\d(?:[\d.,  ]*\d)?)/;

/** `value` in its parts, in order: each number, and each unit between them ("1 h 5 min" is 1, h, 5, min). */
function figureParts(value: string) {
  return value
    .split(NUMBER)
    .filter((part) => part !== "")
    .map((text, index) => ({ text, index, unit: !/^\d/.test(text) }));
}

/**
 * The stats beside the title, as the dashboard's tiles say a number: what each
 * counts on top, small, in capitals, after its icon; the figure under it in the
 * tiles' type, its units smaller and muted, so the numbers are what the eye
 * compares; a line between each. A window narrower than `lg` leaves them out
 * rather than squeeze the title; the subtitle still says how many show.
 */
export function Stats(props: { stats: MastheadStat[] }) {
  const marked = () => props.stats.map((stat, index) => ({ ...stat, index }));
  return (
    <View class="flex-row items-center gap-6 max-lg:hidden">
      <Each of={marked()} key={(stat) => String(stat.index)}>
        {(stat) => (
          <View class={`gap-1 ${stat().index > 0 ? "border-l border-border pl-6" : ""}`}>
            <View class="flex-row items-center gap-1.5">
              <Icon name={stat().icon} size={12} tint="muted-foreground" />
              <Text class="text-xs font-medium uppercase tracking-wider text-muted-foreground" style={{ numberOfLines: 1 }}>
                {stat().label}
              </Text>
            </View>
            <Text class="text-2xl font-bold text-card-foreground" style={{ letterSpacing: -0.4, numberOfLines: 1, ...tabular }}>
              <Each of={figureParts(stat().value)} key={(part) => String(part.index)}>
                {(part) => (
                  <Text class={part().unit ? "text-sm font-medium text-muted-foreground" : ""} style={part().unit ? { letterSpacing: 0 } : {}}>
                    {part().text}
                  </Text>
                )}
              </Each>
            </Text>
          </View>
        )}
      </Each>
    </View>
  );
}

/** What is recording now, under the subtitle, as the dashboard says it. */
export function Live(props: { text: string }) {
  return (
    <View class="flex-row items-center gap-1.5 pt-0.5">
      <LiveDot pulse />
      <Text class="text-xs font-medium text-card-foreground">{props.text}</Text>
    </View>
  );
}

/**
 * A library page's header. `chart` is the badge's color (`CHART`), `count` of
 * `total` show (`one` and `many` name them), `library` holds the filters, `stats` stand beside the title, `live`
 * says what is recording now, under the count, and `actions` are the page's buttons, at the bar's end.
 */
export function LibraryHeader<T extends Item>(props: {
  scroll: AnimatedValue;
  icon: IconName;
  chart: number;
  title: string;
  library: Library<T>;
  count: number;
  total: number;
  one: string;
  many: string;
  stats: MastheadStat[];
  live?: string;
  actions?: JSX.Element;
  /** Whether its Tag menu offers Taken by hand and From events: on the pages of files. */
  sources?: boolean;
}) {
  const counted = () =>
    `${countLabel(props.count)}${props.library.filtered() ? ` of ${countLabel(props.total)}` : ""} ${props.count === 1 ? props.one : props.many}`;
  const subtitle = () => [counted(), props.library.summary()].filter((part) => part !== "").join("  ·  ");

  return (
    <CollapsingHeader
      scroll={props.scroll}
      icon={props.icon}
      chart={props.chart}
      title={props.title}
      subtitle={subtitle()}
      details={<If when={props.live}>{(live) => <Live text={live()} />}</If>}
      aside={<Stats stats={props.stats} />}
    >
      {/* The filters scroll sideways in a window too narrow for them, rather than pushing the buttons out. */}
      <Scroll horizontal scrollbar={false} class="min-w-0 flex-1" contentClass="flex-row items-center pr-2">
        <LibraryFilters library={props.library} sources={props.sources} />
      </Scroll>
      <View class="flex-row items-center gap-2">
        {/* The count the masthead said, once the masthead has gone. */}
        <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1, opacity: collapsedness(props.scroll), ...tabular }}>
          {counted()}
        </Text>
        {props.actions}
      </View>
    </CollapsingHeader>
  );
}
