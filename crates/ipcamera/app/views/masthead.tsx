import { type AnimatedValue, type JSX, Scroll, Text, View } from "nsh";
import { CollapsingHeader, collapsedness, Icon, type IconName, LiveDot } from "../../../dp/ui";
import { LibraryFilters } from "./filters";
import { countLabel, type Item, type Library } from "./library";

/** One of a header's stats: its icon, what it counts, and the figure with its units. */
export type MastheadStat = { icon: IconName; label: string; value: string };

const tabular = { fontVariant: ["tabular-nums" as const] };

/** A number with the marks a locale groups digits and writes decimals with: "1,234", "2.4", "1 234". */
const NUMBER = /(\d(?:[\d.,  ]*\d)?)/;

function figureParts(value: string) {
  return value
    .split(NUMBER)
    .filter((part) => part !== "")
    .map((text, index) => ({ text, index, unit: !/^\d/.test(text) }));
}

/** A header's stats beside its title; a window narrower than `lg` leaves them out. */
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

/** What is recording now, under the subtitle. */
export function Live(props: { text: string }) {
  return (
    <View class="flex-row items-center gap-1.5 pt-0.5">
      <LiveDot pulse />
      <Text class="text-xs font-medium text-card-foreground">{props.text}</Text>
    </View>
  );
}

/** A library page's header: its count, what the filters keep, its stats, and its filters and buttons. */
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
  /** Whether the Tag menu offers Taken by hand and From events. */
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
      <Scroll horizontal scrollbar={false} class="min-w-0 flex-1" contentClass="flex-row items-center pr-2">
        <LibraryFilters library={props.library} sources={props.sources} />
      </Scroll>
      <View class="flex-row items-center gap-2">
        <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1, opacity: collapsedness(props.scroll), ...tabular }}>
          {counted()}
        </Text>
        {props.actions}
      </View>
    </CollapsingHeader>
  );
}
