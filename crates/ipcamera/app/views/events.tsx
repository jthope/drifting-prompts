// The events page: what the cameras saw, each with what it saved, laid out like
// the snapshots (the collapsing header over the grid of `grid.tsx`) and
// narrowed, sorted, grouped and selected the same way. It opens as a log, the
// grid's table at the List size: a row each, under a heading for each day that
// folds its rows away, saying when, what, whose camera and what it saved, and
// ending at its columns' width. Beside it, where the page has room, the
// preview shows one event at a time: its picture, or its videos in place,
// which play when asked to, with the ways to its camera and its files; the
// line between the two drags the table wider or narrower. A press of a row shows it
// there, and Up and Down step through the rows; Control and Shift select, as
// on the Cameras page, and a double press opens what it saved full size. Where
// the page is too narrow for the preview, a plain press selects, as Explorer's does.
// The View menu's other sizes show the events as pictures, as the snapshots
// page does. The log itself is followed by `eventlog.ts`, whatever the page;
// the way from an event to its files, and back, is `linked.ts`.
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

/** What a screen reader calls an event: what, where and when. */
const eventLabel = (event: EventItem) => `${whatLabel(event)} at ${cameraName(event.camera)}, ${startedInFull(event)}`;

/** `Seen 6 times`: how often the camera saw an event within its window; `""` for one pass. */
export const sightingsLabel = (event: EventItem) => (event.sightings > 1 ? `Seen ${countLabel(event.sightings)} times` : "");

/**
 * An event's picture, filling its frame: its snapshot's thumbnail, never the
 * snapshot itself, which decodes to 8 MB at 1080p; else its first video's;
 * else nothing, the frame alone. Over a tile's, `Now` in its corner while the
 * camera sees it now. A row's is too small for the corner (`small`): its row
 * says Now instead. The dashboard's latest events show it too.
 */
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

/**
 * One event at a picture size: its picture over what the camera saw, whose
 * camera and when. A press opens what it saved, or picks it while others are
 * selected.
 */
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

/**
 * Each column's width in a row of the table, the same in the titles and the
 * rows. What the camera saw takes what is left, down to its narrowest, so the
 * table fits a narrow page; at most the table is `TABLE_WIDTH` wide, unless it
 * is dragged wider beside the preview.
 */
const COLUMN: Record<"time" | "what" | "camera" | "saved" | "actions", ViewStyle> = {
  time: { width: 92 },
  what: { flex: 1, minWidth: 120 },
  camera: { width: 150 },
  saved: { width: 72 },
  actions: { width: 44 },
};

/** The table's cells at their widest: what the camera saw gets 200 points of it. */
const TABLE_WIDTH = 92 + 200 + 150 + 72 + 44;
/** The table's cells at their narrowest, what the camera saw at its least: the table is dragged no narrower. */
const TABLE_LEAST = 92 + 120 + 150 + 72 + 44;

/** The table's column titles. Time sorts the events newest or oldest first, as the header's order does. */
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

/** What an event saved, as marks at its row's end: a picture, and its videos with how many past one; nothing for what it didn't save, or what was deleted since. */
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

/**
 * An event's cells in the table: when, what the camera saw, with how many
 * times it saw it and Now while it sees it now, whose camera, and what it
 * saved, all one press: beside the preview (`beside`) a plain press shows it
 * there, elsewhere it selects as Explorer does, and Control and Shift select
 * either way; a double press opens what it saved full size. Then its menu,
 * which a right press anywhere on the row opens too.
 */
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

/** The page's table, which the grid shows at the List size, its rows stepping the preview. */
const TABLE: LibraryTable<EventItem> = {
  titles: () => <EventTitles />,
  cells: (event, beside) => <EventCells event={event()} beside={beside()} />,
  label: eventLabel,
  width: TABLE_WIDTH,
  least: TABLE_LEAST,
  current: (event) => previewed()?.name === event.name,
  step: stepPreview,
};

/** The preview's thumbnails of what an event saved, in points, at 16:9. */
const SHOT = { width: 96, height: 54 };
/** The preview's padding and the gaps between its parts. */
const PAD = 16;
const GAP = 12;
/** Under the preview's picture: what the camera saw and its line under it (two lines at most), and the buttons over their rule. */
const PREVIEW_TEXT = 68;
const PREVIEW_BUTTONS = 45;
/** The least height the preview's picture gets, however short the page. */
const LEAST_PICTURE = 120;

/** What the preview's picture area shows, for the event it was picked on: its picture (-1) or one of its videos, by place. */
const shot = signal<{ event: string; index: number } | null>(null);

/** One of an event's things saved, as the preview's strip shows it: its picture (`index` -1) or a video. */
type Shot = { index: number; thumbnail: string; label: string; length: string; recording: boolean };

/** What `event` saved, in the strip's order: its picture, then its videos as the folder lists them. */
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

/** Which of `event`'s things the preview shows: the one picked, while it still has it, else its picture, else its first video; `null` with neither. */
function shownOf(event: EventItem): number | null {
  const picked = shot();
  const count = eventVideos(event).length;
  if (picked !== null && picked.event === event.name && (picked.index === -1 ? event.picture !== "" : picked.index < count)) return picked.index;
  if (event.picture !== "") return -1;
  return count > 0 ? 0 : null;
}

/**
 * The preview's picture area: the event's picture, which a press opens full
 * size; or one of its videos in place with the platform's controls, at its
 * first picture until its play button is pressed, or playing once picked from
 * the strip (`play`), so opening the page or stepping through the rows plays
 * nothing by itself (one still being written shows its first picture until it
 * stops); or, for one that saved nothing, a line saying so.
 */
function PreviewPicture(props: { event: EventItem; shown: number | null; play: boolean; height: number }) {
  const video = () => (props.shown !== null && props.shown >= 0 ? eventVideos(props.event)[props.shown] : undefined);
  const nothing = () => props.event.picture === "" && props.event.videos.length === 0;
  return (
    <View class={`overflow-hidden rounded-md ${nothing() ? "border border-border" : "bg-black"}`} style={{ height: props.height }}>
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

/** The strip of what an event saved, under its picture, when it saved more than one thing: a press shows that one. */
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

/**
 * The preview beside the table, in `room`: the event's picture or the video
 * picked under it, as large as fits under the header at the top of the page;
 * what the camera saw, whose camera, when, for how long and how many times;
 * the strip of what it saved, when that is more than one thing; and the ways
 * to its camera and to its files on their pages, and its Delete.
 */
function Preview(props: { event: EventItem; room: { width: number; height: number } }) {
  const event = () => props.event;
  const shots = computed(() => shotsOf(event()));
  const shown = () => shownOf(event());
  /** Whether what shows was picked from the strip, which plays a video; what shows by itself waits for its play button. */
  const picked = () => {
    const chosen = shot();
    return chosen !== null && chosen.event === event().name && chosen.index === shown();
  };
  const strip = () => shots().length > 1;
  /** As wide as the card, at 16:9, unless that is taller than the page leaves it. */
  const pictureHeight = () => {
    const across = props.room.width - 2 * PAD - 2;
    const below = PREVIEW_TEXT + PREVIEW_BUTTONS + 2 * GAP + (strip() ? SHOT.height + GAP : 0);
    return Math.max(LEAST_PICTURE, Math.min((across * 9) / 16, props.room.height - 2 * PAD - 2 - below));
  };
  const detail = () => [cameraName(event().camera), startedInFull(event()), event().ongoing ? lastedLabel(event()) : `Lasted ${lastedLabel(event())}`, sightingsLabel(event())].filter((part) => part !== "").join("  ·  ");

  return (
    <Card class="gap-3" style={{ padding: PAD }} testID="preview">
      <PreviewPicture event={event()} shown={shown()} play={picked()} height={pictureHeight()} />

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

      <If when={strip()}>
        <PreviewStrip event={event()} shots={shots()} shown={shown()} />
      </If>

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
    </Card>
  );
}

/** How wide the table is beside the preview, as it was last dragged to, in `settings.json`; `null` until it is, for its columns' own width. */
const tableWidth = signal(Preferences.eventsTableWidth());

/** The preview beside the table, where the page leaves it at least this wide, and the table's width beside it, saved as each drag ends. */
const ASIDE: TableAside = {
  least: 420,
  view: (room: Accessor<{ width: number; height: number }>) => <If when={previewed()}>{(event) => <Preview event={event()} room={room()} />}</If>,
  tableWidth,
  onTableWidth: (width) => {
    tableWidth.set(width);
    void Preferences.saveEventsTableWidth(width);
  },
};

/** The events that saved a picture, in the page's order: those the viewer steps through. */
const pictured = computed(() => ordered().filter((event) => event.picture !== ""));

/** An event's picture at full size: what the camera saw, whose camera and when, and the button that shows that camera. */
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

/** The header: how many show and what the filters keep, the stats and what goes on now, and the filters and buttons in its bar. */
function EventsHeader() {
  const live = () => (totals().ongoing > 0 ? `${countLabel(totals().ongoing)} going on now` : undefined);

  return (
    <LibraryHeader
      scroll={eventsScroll}
      icon="bell"
      // The cameras' color, as the Groups page's: what the cameras saw.
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

/** Before the first event: what makes one, and which cameras are watched; or that nothing is logged. */
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
  // The videos folder, read now and whenever a file comes or goes, for what each event recorded: the preview plays them.
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
