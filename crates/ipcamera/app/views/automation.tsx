import { signal, View } from "nsh";
import { Automation, type AutomationSettings, type EventCaptureInfo, type EventVideoSettings } from "ipcamera/native";
import { Appear, CheckBox, CheckField, InlineSelect, SettingRow, SettingsGroup } from "../../../dp/ui";
import { failed } from "./actions";
import { alertsOn, setAlerts } from "./eventlog";
import {
  AFTER_SECONDS,
  APP_STREAMS,
  BEFORE_SECONDS,
  beforeLabel,
  KIND_COLUMNS,
  LONGEST_MINUTES,
  longestLabel,
  numberItems,
  secondsLabel,
  withKind,
} from "./eventvideo";
import { sizeLabel } from "./library";

/** What an event can save, as the Save menu offers it. */
export const CAPTURES: { value: EventCaptureInfo; title: string }[] = [
  { value: "picture", title: "A picture" },
  { value: "video", title: "A video" },
  { value: "pictureAndVideo", title: "Picture and video" },
  { value: "nothing", title: "Nothing" },
];

const WINDOWS = [0, 1, 2, 5, 10, 15, 30];

/** A window as its menu says it: `5 minutes`, `Don't group` for 0. */
export function windowLabel(minutes: number): string {
  if (minutes <= 0) return "Don't group";
  return minutes === 1 ? "1 minute" : `${minutes} minutes`;
}

const MEDIA_LIMITS = [0, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1024, 2048].map((gigabytes) => gigabytes * 1024);

const VIDEO_LIMITS = [0, 100, 250, 500, 1024, 2048, 4096];

/** A limit in megabytes as a menu says it: `250 MB`, `5 GB`, `1 TB`; `No limit` for 0. */
export function limitLabel(megabytes: number): string {
  if (megabytes <= 0) return "No limit";
  const units: [number, string][] = [
    [1024 * 1024, "TB"],
    [1024, "GB"],
  ];
  for (const [size, unit] of units) {
    if (megabytes >= size) {
      const count = megabytes / size;
      return `${Number.isInteger(count) ? count : count.toFixed(1)} ${unit}`;
    }
  }
  return `${megabytes} MB`;
}

/** The choices of a limit's menu, with `current` among them when a hand-edited file holds another. */
export function limitItems(choices: number[], current: number) {
  return numberItems(choices, current, limitLabel);
}

/** The choices of the Group within menu, with `current` among them when a hand-edited file holds another. */
export function windowItems(current: number) {
  return numberItems(WINDOWS, current, windowLabel);
}

const savesVideo = (capture: EventCaptureInfo) => capture === "video" || capture === "pictureAndVideo";

function EventVideoGroup(props: { video: EventVideoSettings; onChange: (video: EventVideoSettings) => void }) {
  const change = (next: Partial<EventVideoSettings>) => props.onChange({ ...props.video, ...next });
  return (
    <SettingsGroup title="Event videos" description="How each event's videos are made, for every camera that has no settings of its own (a camera's Events part).">
      <SettingRow
        first
        icon="clock"
        label="Keep recording"
        detail="After the camera last saw something. What it sees again sooner continues the same video rather than starting another, and counts as the same sighting."
        control={
          <InlineSelect
            label="Keep recording for"
            class="w-32"
            items={numberItems(AFTER_SECONDS, props.video.afterSeconds, secondsLabel)}
            value={String(props.video.afterSeconds)}
            onChange={(value) => change({ afterSeconds: Number(value) })}
          />
        }
      />
      <SettingRow
        icon="undo-2"
        label="Start before"
        detail="Each video starts this long before the camera saw something, held in memory from a stream that stays connected, decoding nothing. Off, a camera that isn't connected connects as it sees something, and a video's first seconds can be missed."
        control={
          <InlineSelect
            label="Start each video before"
            class="w-32"
            items={numberItems(BEFORE_SECONDS, props.video.beforeSeconds, beforeLabel)}
            value={String(props.video.beforeSeconds)}
            onChange={(value) => change({ beforeSeconds: Number(value) })}
          />
        }
      />
      <SettingRow
        icon="film"
        label="Longest video"
        detail="A video that runs longer goes on in a new file, from the same moment, with nothing lost between them."
        control={
          <InlineSelect
            label="Longest video"
            class="w-32"
            items={numberItems(LONGEST_MINUTES, props.video.longestMinutes, longestLabel)}
            value={String(props.video.longestMinutes)}
            onChange={(value) => change({ longestMinutes: Number(value) })}
          />
        }
      />
      <SettingRow
        icon="move"
        label="Motion"
        detail="For a camera whose AI tells people, vehicles and animals apart: motion keeps a video going between its detections, for up to two minutes after the last. Motion alone never starts one."
        control={<CheckBox label="Motion keeps a video going" checked={props.video.motionContinues} onChange={(on) => change({ motionContinues: on })} />}
      />
      <SettingRow
        icon="video"
        label="Stream"
        detail="Which of each camera's streams the videos record. Its own is the one its Stream setting names."
        control={
          <InlineSelect label="Stream to record" class="w-40" items={APP_STREAMS} value={props.video.stream} onChange={(value) => change({ stream: value })} />
        }
      />
      <SettingRow
        icon="tag"
        label="Save videos of"
        detail="Every event is logged, whatever the camera saw; only these save a video. A camera that knows only motion saves one for motion."
      >
        <KindChecks video={props.video} onChange={props.onChange} />
      </SettingRow>
    </SettingsGroup>
  );
}

/** The kinds that save a video, each a box, in the columns of `KIND_COLUMNS`. */
export function KindChecks(props: { video: EventVideoSettings; onChange: (video: EventVideoSettings) => void }) {
  return (
    <View class="flex-row flex-wrap gap-x-4 gap-y-2.5">
      <Each of={KIND_COLUMNS} key={(column) => column.map((kind) => kind.key).join()}>
        {(column) => (
          <View class="w-36 gap-2.5">
            <Each of={column()} key={(kind) => kind.key}>
              {(kind) => (
                <CheckField label={kind().title} checked={props.video.kinds.includes(kind().key)} onChange={(on) => props.onChange(withKind(props.video, kind().key, on))} />
              )}
            </Each>
          </View>
        )}
      </Each>
    </View>
  );
}

/** The Automation section. */
export function AutomationSection() {
  const settings = signal(Automation.settings());
  const held = signal<number | null>(null);
  Automation.mediaSize()
    .then((bytes) => held.set(bytes))
    .catch(() => held.set(null));

  const change = (next: Partial<AutomationSettings>) => {
    const saved = { ...settings(), ...next };
    settings.set(saved);
    Automation.save(saved).catch(failed);
  };
  const holding = () => {
    const bytes = held();
    return bytes === null ? "" : ` They hold ${sizeLabel(bytes)} now.`;
  };

  return (
    <Appear>
      <View class="gap-6">
        <SettingsGroup title="When a camera sees something" description="What the app does by itself as an event starts.">
          <SettingRow
            first
            icon="bell"
            label="Events"
            detail="Watch every camera the app can, on screen or not, and log what each sees on the Events page. Off, a camera is asked what it sees only while it streams, for its badge."
            control={<CheckBox label="Log events" checked={settings().logEvents} onChange={(on) => change({ logEvents: on })} />}
          />
          <If when={settings().logEvents}>
            <SettingRow
              icon="monitor"
              label="Alerts"
              detail="A message at the bottom right says what the camera sees, with a button that shows the camera, once an event."
              control={<CheckBox label="Show alerts" checked={alertsOn()} onChange={setAlerts} />}
            />
            <SettingRow
              icon="camera"
              label="Save"
              detail="What each event saves into Snapshots and Videos, tagged with what the camera saw. A video records while the camera sees something, as Event videos below says. A camera you disconnected is left alone."
              control={
                <InlineSelect
                  label="What each event saves"
                  class="w-40"
                  items={CAPTURES}
                  value={settings().eventCapture}
                  onChange={(value) => change({ eventCapture: value as EventCaptureInfo })}
                />
              }
            />
            <SettingRow
              icon="clock"
              label="Group within"
              detail="What a camera sees again this soon after it last saw something joins the same event, so a car that stops and leaves is one event, with one picture and one alert."
              control={
                <InlineSelect
                  label="Group events within"
                  class="w-32"
                  items={windowItems(settings().eventWindowMinutes)}
                  value={String(settings().eventWindowMinutes)}
                  onChange={(value) => change({ eventWindowMinutes: Number(value) })}
                />
              }
            />
          </If>
        </SettingsGroup>

        <If when={settings().logEvents && savesVideo(settings().eventCapture)}>
          <EventVideoGroup video={settings().eventVideo} onChange={(eventVideo) => change({ eventVideo })} />
        </If>

        <SettingsGroup title="Storage limits" description={`For the snapshots and videos together, in the folders General names.${holding()}`}>
          <SettingRow
            first
            icon="hard-drive"
            label="All media"
            detail={
              settings().overwriteOldest && settings().mediaLimitMb > 0
                ? "At this size the app deletes the oldest snapshots and videos to make room for new ones."
                : "At this size the app takes no more snapshots or videos, and stops what it records."
            }
            control={
              <InlineSelect
                label="Limit for all media"
                class="w-28"
                items={limitItems(MEDIA_LIMITS, settings().mediaLimitMb)}
                value={String(settings().mediaLimitMb)}
                onChange={(value) => change({ mediaLimitMb: Number(value) })}
              />
            }
          />
          <If when={settings().mediaLimitMb > 0}>
            <SettingRow
              icon="trash-2"
              label="Make room"
              detail="At the limit, delete the oldest snapshots and videos for good, so new ones can be saved. A video still recording is never deleted."
              control={<CheckBox label="Delete the oldest to make room" checked={settings().overwriteOldest} onChange={(on) => change({ overwriteOldest: on })} />}
            />
          </If>
          <SettingRow
            icon="film"
            label="Video file size"
            detail="A recording goes on in a new file when its file reaches this size."
            control={
              <InlineSelect
                label="Largest video file"
                class="w-28"
                items={limitItems(VIDEO_LIMITS, settings().videoLimitMb)}
                value={String(settings().videoLimitMb)}
                onChange={(value) => change({ videoLimitMb: Number(value) })}
              />
            }
          />
        </SettingsGroup>
      </View>
    </Appear>
  );
}
