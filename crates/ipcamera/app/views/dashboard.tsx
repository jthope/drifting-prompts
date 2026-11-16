// The dashboard: the cameras and what they have saved and seen, at a glance.
// Under the header the other pages have, with the period it looks back over in
// its bar, a row of numbers (each opening the page it sums up, the disk's
// Settings), then the cards
// on a grid of twelve columns as wide as the window: how full the drives are
// beside what was saved day by day, when the cameras saw something by weekday
// and hour beside the hours of the day added up, then the latest events beside
// the space each camera's files take. A narrower window takes two columns, then
// one. The cameras themselves are the Cameras page's, which the first number
// opens. What it shows is in `overview.ts`; the charts and cards are
// `ui/chart.tsx`'s.
import { createAnimatedValue, effect, Scroll, signal, Text, View } from "nsh";
import { Snapshots, Videos } from "ipcamera/native";
import {
  Button,
  ChartCard,
  CollapsingHeader,
  Donut,
  Heatmap,
  LegendItem,
  LiveDot,
  look,
  PageTabs,
  Press,
  SERIES,
  type Slice,
  SplitBar,
  StackedBars,
  StatTile,
  StatusDot,
  WASHES,
} from "../../../dp/ui";
import { current, type EventItem, events, eventsLabel, kept, viewing, whatLabel } from "./eventlog";
import { EventPicture } from "./events";
import { cameraName, countLabel, sizeLabel } from "./library";
import { CHART, recording, show } from "./model";
import {
  agoLabel,
  cameraSpace,
  type Day,
  type Drive,
  days,
  drives,
  type HourCell,
  type HourTotal,
  hourCells,
  hourTotals,
  now,
  PERIODS,
  period,
  periodDays,
  ready,
  reloadSpace,
  spaceProblem,
  summary,
} from "./overview";
import { showSettings } from "./preferences";
import { reload as reloadVideos, totalLabel } from "./recordings";
import { reload as reloadSnapshots } from "./snapshots";

/** How often the dashboard reads the videos again while a camera records, so their length, size and the free space keep up. */
const WHILE_RECORDING_EVERY = 5000;

// Each thing in its chart color (`CHART`). The drive is the disk tile's color: its other files in
// the full color, and its free space a wash of it.
const VIDEOS = SERIES[CHART.videos];
const SNAPSHOTS = SERIES[CHART.snapshots];
const OTHER = SERIES[CHART.disk];
const FREE = WASHES[CHART.disk];
// The events take the cameras' color, as the events page's badge does.
const EVENTS = SERIES[CHART.cameras];

const tabular = { fontVariant: ["tabular-nums" as const] };
const shortDay = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });
const longDay = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric" });
const shortWeekday = new Intl.DateTimeFormat(undefined, { weekday: "short" });
const longWeekday = new Intl.DateTimeFormat(undefined, { weekday: "long" });
const hourOnly = new Intl.DateTimeFormat(undefined, { hour: "numeric" });

/** How many of the latest events the dashboard lists. */
const LATEST = 5;

/** The heatmap's lines and columns, as its cells name them: the weekdays, Sunday first, and the hours, midnight first. */
const WEEKDAYS = Array.from({ length: 7 }, (_, weekday) => String(weekday));
const HOURS = Array.from({ length: 24 }, (_, hour) => String(hour));
/** The hours named under the hours' charts: midnight, 6 AM, noon and 6 PM. */
const HOUR_TICKS = [0, 6, 12, 18];

/** A day that is weekday `weekday` (0 for Sunday): 1 January 2023 was a Sunday. */
const onWeekday = (weekday: number, hour = 0) => new Date(2023, 0, 1 + weekday, hour);
/** "6 PM". */
const hourName = (hour: number) => hourOnly.format(onWeekday(0, hour));
/** "6 PM – 7 PM". */
const hourSpan = (hour: number) => `${hourName(hour)} – ${hourName((hour + 1) % 24)}`;

/** "1 camera", "3 cameras". */
const counted = (count: number, one: string, many: string) => `${countLabel(count)} ${count === 1 ? one : many}`;

/** A line in a card saying there is nothing to show yet. */
function Nothing(props: { text: string }) {
  return (
    <View class="min-h-24 items-center justify-center">
      <Text class="text-center text-sm text-muted-foreground">{props.text}</Text>
    </View>
  );
}

/** The numbers across the top, each opening the page it sums up; what is on disk opens Settings over the dashboard, where the folders are. */
function Numbers() {
  const sums = summary;
  const inPeriod = () => days().reduce((total, day) => total + day.snapshots, 0);
  const free = () => drives().map((drive) => `${sizeLabel(drive.free)} free on ${drive.drive}`).join(", ");
  return (
    <View class="grid grid-cols-2 gap-4 xl:grid-cols-4">
      <StatTile
        icon="video"
        chart={CHART.cameras}
        label="Cameras"
        value={countLabel(sums().cameras)}
        detail={`${countLabel(sums().connected)} connected  ·  ${countLabel(sums().recording)} recording`}
        testID="tile:cameras"
        onPress={() => show({ kind: "cameras" })}
      />
      <StatTile
        icon="film"
        chart={CHART.videos}
        label="Videos"
        value={countLabel(sums().videos)}
        detail={sums().videos > 0 ? `${totalLabel(sums().seconds)} recorded  ·  ${sizeLabel(sums().videoBytes)}` : "None recorded yet"}
        trend={days().map((day) => day.videos)}
        testID="tile:videos"
        onPress={() => show({ kind: "videos" })}
      />
      <StatTile
        icon="images"
        chart={CHART.snapshots}
        label="Snapshots"
        value={countLabel(sums().snapshots)}
        detail={`${countLabel(inPeriod())} in the last ${periodDays()} days  ·  ${sizeLabel(sums().snapshotBytes)}`}
        trend={days().map((day) => day.snapshots)}
        testID="tile:snapshots"
        onPress={() => show({ kind: "snapshots" })}
      />
      <StatTile
        icon="hard-drive"
        chart={CHART.disk}
        label="On disk"
        value={sizeLabel(sums().videoBytes + sums().snapshotBytes)}
        detail={free() || (spaceProblem() ? "Free space unknown" : " ")}
        testID="tile:disk"
        onPress={() => showSettings("general")}
      />
    </View>
  );
}

/** What a day's bar says when the pointer or the keys pick it out. */
function DayTip(props: { day: Day }) {
  return (
    <View class="min-w-40 gap-1">
      <Text class="text-xs font-semibold text-popover-foreground">{longDay.format(new Date(props.day.start))}</Text>
      <LegendItem paint={VIDEOS} label="Videos" value={countLabel(props.day.videos)} />
      <LegendItem paint={SNAPSHOTS} label="Snapshots" value={countLabel(props.day.snapshots)} />
      <If when={props.day.seconds > 0}>
        <Text class="text-xs text-muted-foreground" style={tabular}>{`${totalLabel(props.day.seconds)} recorded`}</Text>
      </If>
    </View>
  );
}

/** The videos and snapshots saved each day of the period, piled up. */
function Activity() {
  const videos = () => days().reduce((total, day) => total + day.videos, 0);
  const shots = () => days().reduce((total, day) => total + day.snapshots, 0);
  const detail = () =>
    videos() + shots() === 0
      ? `Nothing in the last ${periodDays()} days`
      : `${counted(videos(), "video", "videos")} and ${counted(shots(), "snapshot", "snapshots")} in the last ${periodDays()} days`;
  return (
    <ChartCard title="Saved per day" detail={detail()} class="grow">
      <If when={videos() + shots() > 0} else={<Nothing text="Videos and snapshots show here, day by day, as cameras save them." />}>
        <StackedBars
          rows={days()}
          series={[
            { value: (day: Day) => day.videos, name: "videos", paint: VIDEOS },
            { value: (day: Day) => day.snapshots, name: "snapshots", paint: SNAPSHOTS },
          ]}
          tick={(day) => shortDay.format(new Date(day.start))}
          tickEvery={Math.ceil(days().length / 7)}
          tooltip={(day) => <DayTip day={day} />}
          label={`Videos and snapshots saved each day, the last ${periodDays()} days`}
          testID="activity"
        />
      </If>
      <View class="flex-row gap-5">
        <View class="w-24">
          <LegendItem paint={VIDEOS} label="Videos" />
        </View>
        <View class="w-24">
          <LegendItem paint={SNAPSHOTS} label="Snapshots" />
        </View>
      </View>
    </ChartCard>
  );
}

/** One drive: a ring of what fills it, the free space in its hole (or the part the pointer is over), and the parts to its right. */
function DriveRing(props: { drive: Drive }) {
  const hovered = signal<Slice | undefined>(undefined);
  const other = () => Math.max(0, props.drive.total - props.drive.free - props.drive.videos - props.drive.snapshots);
  const slices = (): Slice[] =>
    [
      { name: "Videos", value: props.drive.videos, paint: VIDEOS },
      { name: "Snapshots", value: props.drive.snapshots, paint: SNAPSHOTS },
      { name: "Other files", value: other(), paint: OTHER },
      { name: "Free", value: props.drive.free, paint: FREE },
    ].filter((slice) => slice.value > 0);
  const shown = () => hovered() ?? { name: "Free", value: props.drive.free, paint: FREE };
  return (
    <View class="flex-row items-center gap-6">
      <Donut
        slices={slices()}
        size={168}
        label={`${props.drive.drive}: ${sizeLabel(props.drive.free)} free of ${sizeLabel(props.drive.total)}`}
        onHover={(slice) => hovered.set(slice)}
        center={
          <View class="items-center gap-0.5">
            <Text class="text-lg font-bold text-card-foreground" style={tabular}>
              {sizeLabel(shown().value)}
            </Text>
            <Text class="text-xs text-muted-foreground">{shown().name === "Free" ? `free on ${props.drive.drive}` : shown().name}</Text>
          </View>
        }
      />
      <View class="min-w-0 max-w-64 flex-1 gap-2">
        <LegendItem paint={VIDEOS} label="Videos" value={sizeLabel(props.drive.videos)} />
        <LegendItem paint={SNAPSHOTS} label="Snapshots" value={sizeLabel(props.drive.snapshots)} />
        <LegendItem paint={OTHER} label="Other files" value={sizeLabel(other())} />
        <LegendItem paint={FREE} label="Free" value={sizeLabel(props.drive.free)} />
        <Text class="pt-1 text-xs text-muted-foreground" style={tabular}>{`${sizeLabel(props.drive.total)} in all`}</Text>
      </View>
    </View>
  );
}

/** How full the drives under the videos and snapshots folders are. */
function Storage() {
  const detail = () =>
    drives().length > 1 ? "The drives the videos and the snapshots are saved on." : "The drive the videos and snapshots are saved on.";
  return (
    <ChartCard title="Storage" detail={detail()} class="grow">
      <If when={spaceProblem()}>{(problem) => <Nothing text={problem()} />}</If>
      {/* In the middle of what the card has left, as tall as the chart beside it makes the row. */}
      <View class="grow justify-center gap-6">
        <Each of={drives()} key={(drive) => drive.drive}>
          {(drive) => <DriveRing drive={drive()} />}
        </Each>
      </View>
    </ChartCard>
  );
}

/** The space each camera's files take, largest first, each bar against the largest. */
function SpaceByCamera() {
  const largest = () => cameraSpace()[0]?.total ?? 0;
  return (
    <ChartCard title="Space by camera" detail="What each camera's videos and snapshots take on disk." class="grow">
      <If when={cameraSpace().length > 0} else={<Nothing text="No videos or snapshots yet." />}>
        <View class="gap-3">
          <Each of={cameraSpace()} key={(row) => row.key}>
            {(row) => (
              <View class="gap-1.5">
                <View class="flex-row items-center gap-2">
                  <Text class="min-w-0 flex-1 text-sm text-card-foreground" style={{ numberOfLines: 1 }}>
                    {row().name}
                  </Text>
                  <Text class="text-xs text-muted-foreground" style={tabular}>
                    {sizeLabel(row().total)}
                  </Text>
                </View>
                <SplitBar
                  parts={[
                    { value: row().videos, paint: VIDEOS },
                    { value: row().snapshots, paint: SNAPSHOTS },
                  ]}
                  max={largest()}
                />
              </View>
            )}
          </Each>
        </View>
      </If>
    </ChartCard>
  );
}

/** What an hour's cell or bar says when the pointer or the keys pick it out. */
function HourTip(props: { title: string; events: number }) {
  return (
    <View class="min-w-36 gap-1">
      <Text class="text-xs font-semibold text-popover-foreground">{props.title}</Text>
      <LegendItem paint={EVENTS} label="Events" value={countLabel(props.events)} />
    </View>
  );
}

/** "Tuesday, 6 PM – 7 PM". */
const cellTitle = (cell: HourCell) => `${longWeekday.format(onWeekday(cell.weekday))}, ${hourSpan(cell.hour)}`;

/** When the cameras saw something over the period: a cell for each hour of each weekday, the more events the fuller its color. */
function EventHours() {
  const total = () => hourCells().reduce((sum, cell) => sum + cell.events, 0);
  const detail = () =>
    total() === 0 ? `No events in the last ${periodDays()} days` : `${eventsLabel(total())} by weekday and hour, the last ${periodDays()} days`;
  return (
    <ChartCard title="Events by hour" detail={detail()} class="grow">
      <If when={total() > 0} else={<Nothing text="Events show here by weekday and hour as the cameras see them." />}>
        <Heatmap
          rows={hourCells()}
          columns={HOURS}
          lines={WEEKDAYS}
          column={(cell) => String(cell.hour)}
          line={(cell) => String(cell.weekday)}
          value={(cell) => cell.events}
          columnTick={hourName}
          lineTick={(weekday) => shortWeekday.format(onWeekday(weekday))}
          columnTicks={HOUR_TICKS}
          chart={CHART.cameras}
          tooltip={(cell) => <HourTip title={cellTitle(cell)} events={cell.events} />}
          describe={(cell) => `${cellTitle(cell)}: ${eventsLabel(cell.events)}`}
          label={`Events by weekday and hour, the last ${periodDays()} days`}
          testID="event-hours"
        />
      </If>
    </ChartCard>
  );
}

/** The hours of the day over the period, whatever the weekday: the heatmap's columns added up, as bars. */
function BusiestHours() {
  const total = () => hourTotals().reduce((sum, hour) => sum + hour.events, 0);
  const busiest = () => hourTotals().reduce((most, hour) => (hour.events > most.events ? hour : most));
  const detail = () => (total() === 0 ? `No events in the last ${periodDays()} days` : `Busiest from ${hourSpan(busiest().hour)}`);
  return (
    <ChartCard title="Busiest hours" detail={detail()} class="grow">
      <If when={total() > 0} else={<Nothing text="The hours the cameras see most show here." />}>
        <StackedBars
          rows={hourTotals()}
          series={[{ value: (hour: HourTotal) => hour.events, name: "events", paint: EVENTS }]}
          tick={(hour) => hourName(hour.hour)}
          ticks={HOUR_TICKS}
          tooltip={(hour) => <HourTip title={hourSpan(hour.hour)} events={hour.events} />}
          label={`Events in each hour of the day, the last ${periodDays()} days`}
          testID="busiest-hours"
        />
      </If>
    </ChartCard>
  );
}

/**
 * One of the latest events: its picture, what the camera saw, whose camera and
 * how long ago. A press shows it on the events page, in the preview, with its
 * picture open full size where it saved one.
 */
function LatestEvent(props: { event: EventItem }) {
  const event = () => props.event;
  const ago = () => agoLabel(event().startedAt, now());
  const open = () => {
    current.set(event().name);
    if (event().picture !== "") viewing.set(event().name);
    show({ kind: "events" });
  };
  return (
    <Press
      label={`${whatLabel(event())} at ${cameraName(event().camera)}, ${ago()}. Open it`}
      testID={`latest:${event().id}`}
      scale={0.99}
      class={`flex-row items-center gap-3 rounded-md px-2 py-1.5 ${look().listRow.hover} ${look().rings.visible}`}
      onPress={open}
    >
      <View class="overflow-hidden rounded border border-border bg-black" style={{ width: 56, aspectRatio: 16 / 9 }}>
        <EventPicture event={event()} small />
      </View>
      <View class="min-w-0 flex-1 flex-row items-center gap-2">
        <Text class="min-w-0 text-sm font-medium text-card-foreground" style={{ numberOfLines: 1, flexShrink: 1 }}>
          {whatLabel(event())}
        </Text>
        <If when={event().ongoing}>
          <View class="flex-row items-center gap-1">
            <StatusDot tone="warn" />
            <Text class="text-xs font-medium text-warning">Now</Text>
          </View>
        </If>
      </View>
      <Text class="min-w-0 flex-1 text-sm text-muted-foreground" style={{ numberOfLines: 1 }}>
        {cameraName(event().camera)}
      </Text>
      <Text class="w-28 text-right text-xs text-muted-foreground" style={{ numberOfLines: 1, ...tabular }}>
        {ago()}
      </Text>
    </Press>
  );
}

/** The newest events, whatever the period, and a button to all of them. */
function LatestEvents() {
  const latest = () => events().slice(0, LATEST);
  const today = () => {
    const midnight = new Date(now()).setHours(0, 0, 0, 0);
    return events().filter((event) => event.startedAt >= midnight).length;
  };
  return (
    <ChartCard
      title="Latest events"
      detail={`${eventsLabel(today())} today`}
      action={<Button label="Show all" kind="ghost" onPress={() => show({ kind: "events" })} />}
      class="grow"
    >
      <If when={latest().length > 0} else={<Nothing text="Events show here as the cameras see them." />}>
        <View class="gap-0.5">
          <Each of={latest()} key={(event) => event.id}>
            {(event) => <LatestEvent event={event()} />}
          </Each>
        </View>
      </If>
    </ChartCard>
  );
}

/** The dashboard page, in place of the stage and the PTZ panel. */
export function DashboardPage() {
  const scroll = createAnimatedValue(0);
  // Reads each folder now, then again whenever a file comes or goes, by this app or anything else,
  // and the drives with them.
  Snapshots.onChange(() => {
    reloadSnapshots();
    reloadSpace();
  });
  Videos.onChange(() => {
    reloadVideos();
    reloadSpace();
  });
  // A file being written doesn't change the folder's listing, so while a camera records the videos
  // and the drives are read again on a timer.
  effect(() => {
    if (!recording()) return;
    const timer = setInterval(() => {
      reloadVideos();
      reloadSpace();
    }, WHILE_RECORDING_EVERY);
    return () => clearInterval(timer);
  });
  now.set(Date.now());
  effect(() => {
    const timer = setInterval(() => now.set(Date.now()), 60_000);
    return () => clearInterval(timer);
  });

  const live = () => summary().recording;
  return (
    <Scroll testID="dashboard" scrollValue={scroll} class="flex-1 bg-background">
      <CollapsingHeader
        scroll={scroll}
        icon="layout-dashboard"
        chart={CHART.cameras}
        title="Dashboard"
        subtitle="Your cameras and what they have saved, at a glance."
        details={
          <If when={live() > 0}>
            <View class="flex-row items-center gap-1.5 pt-0.5">
              <LiveDot pulse />
              <Text class="text-xs font-medium text-card-foreground">{`${counted(live(), "camera", "cameras")} recording now`}</Text>
            </View>
          </If>
        }
      >
        <PageTabs items={PERIODS} value={period()} onChange={(next) => period.set(next)} />
      </CollapsingHeader>
      <If when={ready()}>
        <View class="w-full gap-4 px-6 pb-12 pt-6">
          <Numbers />
          {/* As wide as the window, as the header is: twelve columns on a wide window, the charts of
              time taking the most of them, two on a narrower one and one on a narrow one. Each card is
              as tall as its row. Where the app keeps no event log (the web), the space by camera
              takes the events' place. */}
          <View class="grid grid-cols-1 gap-4 lg:grid-cols-2 xl:grid-cols-12">
            <View class="min-w-0 xl:col-span-4">
              <Storage />
            </View>
            <View class="min-w-0 xl:col-span-8">
              <Activity />
            </View>
            <If when={kept}>
              <>
                <View class="min-w-0 xl:col-span-8">
                  <EventHours />
                </View>
                <View class="min-w-0 xl:col-span-4">
                  <BusiestHours />
                </View>
                <View class="min-w-0 xl:col-span-7">
                  <LatestEvents />
                </View>
              </>
            </If>
            <View class={kept ? "min-w-0 xl:col-span-5" : "min-w-0 lg:col-span-2 xl:col-span-12"}>
              <SpaceByCamera />
            </View>
          </View>
        </View>
      </If>
    </Scroll>
  );
}
