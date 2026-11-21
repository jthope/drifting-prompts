import { type Accessor, computed, Image, signal, Text, Video, View, type ViewStyle } from "nsh";
import { Automation, Preferences, Videos } from "ipcamera/native";
import { Button, Card, ColumnTitle, EmptyState, Icon, IconButton, MediaTile, MenuOpener, OverlayChip, Press, RecMark, rowText, StatusDot } from "../../../dp/ui";
import { popup } from "./actions";
import {
  clearEvents,
  clearFilters,
  current,
  deleteEvent,
  deleteEvents,
  eventLibrary,
  eventMenu,
  type EventItem,
  events,
  eventsLabel,
  eventsScroll,
  filtered,
  kept,
  lastedLabel,
  loaded,
  ordered,
  previewed,
  showCamera,
  startedInFull,
  startedLabel,
  stepPreview,
  tileMenu,
  totals,
  viewing,
  whatLabel,
} from "./eventlog";
import { LibraryGrid, type LibraryTable, NothingMatches, type TableAside } from "./grid";
import { cameraName, countLabel } from "./library";
import { eventLinks, eventVideos, openEvent, showEventsSnapshot, showEventsVideos } from "./linked";
import { pickOf } from "./listing";
import { LibraryHeader } from "./masthead";
import { CHART, cameraOf } from "./model";
import { lengthLabel, reload as reloadVideos } from "./recordings";
import { PictureViewer } from "./viewer";

const tabular = { fontVariant: ["tabular-nums" as const] };

const eventLabel = (event: EventItem) => `${whatLabel(event)} at ${cameraName(event.camera)}, ${startedInFull(event)}`;

/** `Seen 6 times`: how often the camera saw an event within its window; `""` for one pass. */
export const sightingsLabel = (event: EventItem) => (event.sightings > 1 ? `Seen ${countLabel(event.sightings)} times` : "");

/** An event's picture: its snapshot's thumbnail, never the full snapshot, else its first video's. */
export function EventPicture(props: { event: EventItem; small?: boolean }) {
  const still = () => (props.event.thumbnail !== "" ? props.event.thumbnail : (eventVideos(props.event)[0]?.thumbnail ?? ""));
  return (
    <>
      <If when={still() !== ""}>
        <Image src={still()} fadeDuration={150} style={{ width: "100%", height: "100%", objectFit: "cover" }} />
      </If>
      <If when={props.event.ongoing && !props.small}>
        <OverlayChip class="absolute bottom-1.5 right-1.5">
          <Text class="text-xs font-medium text-white">Now</Text>
        </OverlayChip>
      </If>
    </>
  );
}

function Tile(props: { event: EventItem }) {
  const name = () => props.event.name;
  return (
    <MediaTile
      label={eventLabel(props.event)}
      testID={`event:${props.event.id}`}
      frame="black"
      picture={<EventPicture event={props.event} />}
      title={whatLabel(props.event)}
      detail={[cameraName(props.event.camera), startedLabel(props.event), sightingsLabel(props.event)].filter((part) => part !== "").join("  ·  ")}
      size={eventLibrary.size()}
      selected={eventLibrary.isSelected(name())}
      selecting={eventLibrary.selecting()}
      onPress={(keys) => eventLibrary.press(props.event, keys, () => openEvent(props.event))}
      onCheck={(range) => eventLibrary.pick(name(), range ? "range" : "toggle")}
      onContextMenu={(at) => popup(tileMenu(props.event, eventLinks(props.event)), at)}
    />
  );
}

const COLUMN: Record<"time" | "what" | "camera" | "saved" | "actions", ViewStyle> = {
  time: { width: 92 },
  what: { flex: 1, minWidth: 120 },
  camera: { width: 150 },
  saved: { width: 72 },
  actions: { width: 44 },
};

const TABLE_WIDTH = 92 + 200 + 150 + 72 + 44;
const TABLE_LEAST = 92 + 120 + 150 + 72 + 44;

function EventTitles() {
  const newest = () => eventLibrary.order() === "newest";
  return (
    <>
      <View class="min-w-0 flex-1 flex-row items-center">
        <ColumnTitle title="Time" sort={newest() ? "descending" : "ascending"} onSort={() => eventLibrary.order.set(newest() ? "oldest" : "newest")} style={COLUMN.time} />
        <ColumnTitle title="Event" style={COLUMN.what} />
        <ColumnTitle title="Camera" style={COLUMN.camera} />
        <ColumnTitle title="Saved" style={COLUMN.saved} />
      </View>
      <View style={COLUMN.actions} />
    </>
  );
}

function Saved(props: { event: EventItem }) {
  return (
    <View class="flex-row items-center gap-2">
      <If when={props.event.picture !== ""}>
        <Icon name="images" size={14} tint="muted-foreground" />
      </If>
      <If when={props.event.videos.length > 0}>
        <View class="flex-row items-center gap-1">
          <Icon name="film" size={14} tint="muted-foreground" />
          <If when={props.event.videos.length > 1}>
            <Text class="text-xs text-muted-foreground" style={tabular}>
              {countLabel(props.event.videos.length)}
            </Text>
          </If>
        </View>
      </If>
    </View>
  );
}

function EventCells(props: { event: EventItem; beside: boolean }) {
  const event = () => props.event;
  const on = () => eventLibrary.isSelected(event().name);
  const menu = (press: { pagePosition: { x: number; y: number } }) => popup(tileMenu(event(), eventLinks(event())), press.pagePosition);
  return (
    <>
      <View
        role="button"
        accessibilityLabel={eventLabel(event())}
        accessibilityState={{ selected: on() }}
        testID={`event:${event().id}`}
        class="h-full min-w-0 flex-1 flex-row items-center"
        onPress={(press) => {
          if (press.clickCount === 2) return openEvent(event());
          const how = pickOf(press.modifiers);
          if (props.beside && how === "only") current.set(event().name);
          else eventLibrary.pick(event().name, how);
        }}
        onContextMenu={menu}
      >
        <View class="px-2" style={COLUMN.time}>
          <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1, ...tabular }}>
            {startedLabel(event())}
          </Text>
        </View>
        <View class="flex-row items-center gap-2 px-2" style={COLUMN.what}>
          <Text class={`min-w-0 text-sm font-medium ${rowText(on())}`} style={{ numberOfLines: 1, flexShrink: 1 }}>
            {whatLabel(event())}
          </Text>
          <If when={sightingsLabel(event()) !== ""}>
            <Text class="text-xs text-muted-foreground" style={{ numberOfLines: 1, ...tabular }}>
              {sightingsLabel(event())}
            </Text>
          </If>
          <If when={event().ongoing}>
            <View class="flex-row items-center gap-1">
              <StatusDot tone="warn" />
              <Text class="text-xs font-medium text-warning">Now</Text>
            </View>
          </If>
        </View>
        <View class="px-2" style={COLUMN.camera}>
          <Text class={`text-sm ${rowText(on())}`} style={{ numberOfLines: 1 }}>
            {cameraName(event().camera)}
          </Text>
        </View>
        <View class="px-2" style={COLUMN.saved}>
          <Saved event={event()} />
        </View>
      </View>

      <View class="flex-row items-center justify-end pr-1" style={COLUMN.actions}>
        <MenuOpener>{(openMenu) => <IconButton icon="ellipsis" size="small" label={`${whatLabel(event())} options`} onPress={() => openMenu(tileMenu(event(), eventLinks(event())))} />}</MenuOpener>
      </View>
    </>
  );
}

const TABLE: LibraryTable<EventItem> = {
  titles: () => <EventTitles />,
  cells: (event, beside) => <EventCells event={event()} beside={beside()} />,
  label: eventLabel,
  width: TABLE_WIDTH,
  least: TABLE_LEAST,
  current: (event) => previewed()?.name === event.name,
  step: stepPreview,
};

const SHOT = { width: 96, height: 54 };
const PAD = 16;
const GAP = 12;
const PREVIEW_TEXT = 68;
const PREVIEW_BUTTONS = 45;
const LEAST_PICTURE = 120;
const BESIDE_TEXT = 300;

/** The preview's pick for an event: its picture (-1) or one of its videos, by place. */
const shot = signal<{ event: string; index: number } | null>(null);

type Shot = { index: number; thumbnail: string; label: string; length: string; recording: boolean };

function shotsOf(event: EventItem): Shot[] {
  const picture: Shot[] = event.picture === "" ? [] : [{ index: -1, thumbnail: event.thumbnail, label: "Show its picture", length: "", recording: false }];
  const clips = eventVideos(event);
  return [
    ...picture,
    ...clips.map((video, at) => ({
      index: at,
      thumbnail: video.thumbnail,
      label: `Play video ${countLabel(at + 1)} of ${countLabel(clips.length)}, ${lengthLabel(video.seconds)}`,
      length: lengthLabel(video.seconds),
      recording: video.recording,
    })),
  ];
}

function shownOf(event: EventItem): number | null {
  const picked = shot();
  const count = eventVideos(event).length;
  if (picked !== null && picked.event === event.name && (picked.index === -1 ? event.picture !== "" : picked.index < count)) return picked.index;
  if (event.picture !== "") return -1;
  return count > 0 ? 0 : null;
}

/** A video plays only once picked from the strip (`play`), so stepping through the rows plays nothing. */
function PreviewPicture(props: { event: EventItem; shown: number | null; play: boolean; height: number; width?: number }) {
  const video = () => (props.shown !== null && props.shown >= 0 ? eventVideos(props.event)[props.shown] : undefined);
  const nothing = () => props.event.picture === "" && props.event.videos.length === 0;
  return (
    <View class={`overflow-hidden rounded-md ${nothing() ? "border border-border" : "bg-black"}`} style={{ height: props.height, width: props.width }}>
      <If when={props.shown === -1}>
        <Press label={`Open the picture of ${eventLabel(props.event)} full size`} testID="preview:picture" scale={1} class="flex-1" onPress={() => viewing.set(props.event.name)}>
          <Image src={props.event.picture} fadeDuration={150} accessibilityLabel={eventLabel(props.event)} style={{ flex: 1, objectFit: "contain" }} />
        </Press>
      </If>
      <If when={video()}>
        {(clip) => (
          <If
            when={!clip().recording}
            else={
              <View class="flex-1">
                <If when={clip().thumbnail !== ""}>
                  <Image src={clip().thumbnail} fadeDuration={150} style={{ flex: 1, objectFit: "contain" }} />
                </If>
                <OverlayChip class="absolute bottom-2 left-2">
                  <RecMark length={lengthLabel(clip().seconds)} />
                </OverlayChip>
              </View>
            }
          >
            <Video src={clip().path} controls paused={!props.play} accessibilityLabel={`${eventLabel(props.event)}, ${lengthLabel(clip().seconds)}`} style={{ flex: 1 }} />
          </If>
        )}
      </If>
      <If when={nothing()}>
        <View class="flex-1 items-center justify-center gap-1 px-6">
          <Text class="text-center text-sm text-muted-foreground">This event saved no picture or video.</Text>
          <Text class="text-center text-xs text-muted-foreground">Settings → Automation chooses what events save.</Text>
        </View>
      </If>
    </View>
  );
}

function PreviewStrip(props: { event: EventItem; shots: Shot[]; shown: number | null }) {
  return (
    <View class="flex-row flex-wrap gap-2">
      <Each of={props.shots} key={(each) => String(each.index)}>
        {(each) => (
          <Press
            label={each().label}
            testID={`preview:shot:${each().index}`}
            selected={props.shown === each().index}
            class={`overflow-hidden rounded border-2 bg-black ${props.shown === each().index ? "border-ring" : "border-transparent"}`}
            style={SHOT}
            onPress={() => shot.set({ event: props.event.name, index: each().index })}
          >
            <If when={each().thumbnail !== ""}>
              <Image src={each().thumbnail} fadeDuration={150} style={{ width: "100%", height: "100%", objectFit: "cover" }} />
            </If>
            <If when={each().length !== ""}>
              <OverlayChip class="absolute bottom-1 right-1">
                <If
                  when={each().recording}
                  else={
                    <Text class="text-xs font-medium text-white" style={tabular}>
                      {each().length}
                    </Text>
                  }
                >
                  <RecMark />
                </If>
              </OverlayChip>
            </If>
          </Press>
        )}
      </Each>
    </View>
  );
}

function Preview(props: { event: EventItem; room: { width: number; height: number } }) {
  const event = () => props.event;
  const shots = computed(() => shotsOf(event()));
  const shown = () => shownOf(event());
  const picked = () => {
    const chosen = shot();
    return chosen !== null && chosen.event === event().name && chosen.index === shown();
  };
  const strip = () => shots().length > 1;
  const layout = () => {
    const across = props.room.width - 2 * PAD - 2;
    const tall = props.room.height - 2 * PAD - 2;
    const below = PREVIEW_TEXT + PREVIEW_BUTTONS + 2 * GAP + (strip() ? SHOT.height + GAP : 0);
    const stacked = Math.max(LEAST_PICTURE, Math.min((across * 9) / 16, tall - below));
    const besideHeight = Math.min(tall, ((across - GAP - BESIDE_TEXT) * 9) / 16);
    return besideHeight > stacked
      ? { beside: true, width: (besideHeight * 16) / 9, height: besideHeight }
      : { beside: false, width: undefined, height: stacked };
  };
  const detail = () => [cameraName(event().camera), startedInFull(event()), event().ongoing ? lastedLabel(event()) : `Lasted ${lastedLabel(event())}`, sightingsLabel(event())].filter((part) => part !== "").join("  ·  ");

  const about = () => (
    <View class="gap-1">
      <View class="flex-row items-center gap-2">
        <Text class="min-w-0 text-base font-semibold text-card-foreground" style={{ numberOfLines: 1, flexShrink: 1 }}>
          {whatLabel(event())}
        </Text>
        <If when={event().ongoing}>
          <View class="flex-row items-center gap-1">
            <StatusDot tone="warn" />
            <Text class="text-xs font-medium text-warning">Now</Text>
          </View>
        </If>
      </View>
      <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 2, ...tabular }}>
        {detail()}
      </Text>
    </View>
  );
  const saved = () => (
    <If when={strip()}>
      <PreviewStrip event={event()} shots={shots()} shown={shown()} />
    </If>
  );
  const actions = () => (
    <View class="flex-row flex-wrap items-center gap-1 border-t border-border pt-3">
      <Button icon="cctv" label="Show camera" disabled={!cameraOf(event().camera)} onPress={() => showCamera(event())} />
      <If when={event().snapshot !== ""}>
        <Button kind="ghost" icon="images" label="Show in Snapshots" onPress={() => showEventsSnapshot(event())} />
      </If>
      <If when={event().videos.length > 0}>
        <Button kind="ghost" icon="film" label="Show in Videos" onPress={() => showEventsVideos(event())} />
      </If>
      <View class="flex-1" />
      <IconButton icon="trash-2" label="Delete event" disabled={event().ongoing} onPress={() => deleteEvent(event())} />
    </View>
  );

  return (
    <Card class={layout().beside ? "flex-row gap-3" : "gap-3"} style={{ padding: PAD }} testID="preview">
      <PreviewPicture event={event()} shown={shown()} play={picked()} height={layout().height} width={layout().width} />
      <If
        when={layout().beside}
        else={
          <>
            {about()}
            {saved()}
            {actions()}
          </>
        }
      >
        <View class="min-w-0 flex-1 gap-3">
          {about()}
          {saved()}
          <View class="flex-1" />
          {actions()}
        </View>
      </If>
    </Card>
  );
}

const tableWidth = signal(Preferences.eventsTableWidth());

const ASIDE: TableAside = {
  least: 420,
  view: (room: Accessor<{ width: number; height: number }>) => <If when={previewed()}>{(event) => <Preview event={event()} room={room()} />}</If>,
  tableWidth,
  onTableWidth: (width) => {
    tableWidth.set(width);
    void Preferences.saveEventsTableWidth(width);
  },
};

const pictured = computed(() => ordered().filter((event) => event.picture !== ""));

function Viewer() {
  return (
    <PictureViewer
      items={pictured()}
      open={viewing()}
      onOpen={(name) => viewing.set(name)}
      noun="picture"
      src={(event) => event.picture}
      icon="images"
      title={(event) => whatLabel(event)}
      detail={(event) => `${cameraName(event.camera)}  ·  ${startedInFull(event)}`}
      menu={(event) => eventMenu(event, eventLinks(event))}
      remove={deleteEvent}
      canDelete={(event) => !event.ongoing}
      actions={(event) => <IconButton icon="cctv" label="Show camera" disabled={!cameraOf(event.camera)} onPress={() => showCamera(event)} />}
    />
  );
}

function EventsHeader() {
  const live = () => (totals().ongoing > 0 ? `${countLabel(totals().ongoing)} going on now` : undefined);

  return (
    <LibraryHeader
      scroll={eventsScroll}
      icon="bell"
      chart={CHART.cameras}
      title="Events"
      library={eventLibrary}
      count={totals().count}
      total={events().length}
      one="event"
      many="events"
      live={live()}
      stats={[
        { icon: "video", label: totals().cameras === 1 ? "camera" : "cameras", value: countLabel(totals().cameras) },
        { icon: "calendar", label: "today", value: countLabel(totals().today) },
      ]}
      actions={
        <>
          <If when={filtered()}>
            <Button label="Clear filters" onPress={clearFilters} />
          </If>
          <IconButton icon="trash-2" label="Delete every event" onPress={clearEvents} />
        </>
      }
    />
  );
}

function NoEvents() {
  const text = () => {
    if (!kept) return "The desktop app logs what the cameras see. The server doesn't keep an event log yet.";
    if (!Automation.settings().logEvents) return "The app logs no events: Settings → Automation turns them on.";
    return "When a camera sees something, it shows here with what it saved, as Settings → Automation asks: what its AI tells apart, such as a person or a vehicle, or motion for a camera that knows only motion. Every camera the app can watch is watched, on screen or not: a Reolink, a Dahua with its HTTP API on, or a camera with ONVIF events on.";
  };
  return <EmptyState icon="bell" title={kept ? "No events yet" : "No events here"} text={text()} />;
}

/** The events page, in place of the stage and the PTZ panel. */
export function EventsPage() {
  eventsScroll.set(0);
  // The preview plays each event's videos, so this page follows the videos folder too.
  Videos.onChange(() => reloadVideos());

  return (
    <View class="flex-1 bg-background">
      <If when={loaded() || !kept}>
        <If when={events().length > 0} else={<NoEvents />}>
          <LibraryGrid
            testID="events"
            scroll={eventsScroll}
            library={eventLibrary}
            header={<EventsHeader />}
            empty={<NothingMatches what="events" onClear={clearFilters} />}
            tile={(event) => <Tile event={event()} />}
            table={TABLE}
            aside={ASIDE}
            count={eventsLabel}
            onDelete={deleteEvents}
            busy={() => viewing() !== null}
          />
        </If>
      </If>
      <Viewer />
    </View>
  );
}
