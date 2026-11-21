import { computed, signal, Text, View } from "nsh";
import { type MediaUsage, Storage } from "ipcamera/native";
import { LiveDot, look, Meter, Press, StatusItem } from "../../../dp/ui";
import { events, kept } from "./eventlog";
import { countLabel, sizeLabel } from "./library";
import { show, states } from "./model";
import { showSettings } from "./preferences";

/** At this share of the media limit the gauge turns to the theme's warning color. */
export const NEAR_LIMIT = 0.9;

/** The share of `usage.limit` the snapshots and videos take, 0 to 1; 0 with no limit. */
export function shareOfLimit(usage: MediaUsage): number {
  if (usage.limit <= 0) return 0;
  return Math.min(1, (usage.videos + usage.snapshots) / usage.limit);
}

const tabular = { fontVariant: ["tabular-nums" as const] };

function Sizes(props: { usage: MediaUsage }) {
  return (
    <>
      <StatusItem icon="film" tint="chart-1" text={sizeLabel(props.usage.videos)} label={`Videos take ${sizeLabel(props.usage.videos)}`} onPress={() => show({ kind: "videos" })} />
      <StatusItem icon="images" tint="chart-2" text={sizeLabel(props.usage.snapshots)} label={`Snapshots take ${sizeLabel(props.usage.snapshots)}`} onPress={() => show({ kind: "snapshots" })} />
      <If when={props.usage.drive !== ""}>
        <StatusItem
          icon="hard-drive"
          tint="chart-4"
          text={`${sizeLabel(props.usage.free)} free`}
          label={`${sizeLabel(props.usage.free)} free on ${props.usage.drive}`}
          onPress={() => showSettings("general")}
        />
      </If>
    </>
  );
}

function Gauge(props: { usage: MediaUsage }) {
  const used = () => props.usage.videos + props.usage.snapshots;
  const text = () => `Media ${sizeLabel(used())} of ${sizeLabel(props.usage.limit)}`;
  return (
    <Press label={`${text()}. Show the limit`} scale={1} class={`h-6 flex-row items-center gap-2 rounded-md px-1.5 ${look().buttons.ghost.frame}`} onPress={() => showSettings("automation")}>
      <Text class={`text-xs ${look().statusBar.text}`} style={tabular}>
        {text()}
      </Text>
      <Meter value={shareOfLimit(props.usage)} warn={shareOfLimit(props.usage) >= NEAR_LIMIT} />
      <If when={props.usage.makeRoom}>
        <Text class={`text-xs ${look().statusBar.text}`}>Make room on</Text>
      </If>
    </Press>
  );
}

/** The status bar, along the window's bottom. */
export function StatusBar() {
  const usage = signal<MediaUsage | null>(null);
  Storage.onUsage((next) => usage.set(next));

  const live = () => states().filter((camera) => camera.connected && camera.tone === "ok").length;
  const recording = () => states().filter((camera) => camera.recording).length;
  const today = computed(() => {
    const start = new Date(new Date().setHours(0, 0, 0, 0)).getTime();
    return events().filter((event) => event.startedAt >= start).length;
  });

  return (
    <View class={`min-h-8 flex-row items-center gap-1 px-2 ${look().statusBar.frame}`}>
      <StatusItem
        icon="cctv"
        tint="chart-3"
        text={`${countLabel(live())} of ${countLabel(states().length)} live`}
        label={`${countLabel(live())} of ${countLabel(states().length)} cameras live. Show the cameras`}
        onPress={() => show({ kind: "cameras" })}
      />
      <If when={recording() > 0}>
        <StatusItem mark={<LiveDot />} text={`${countLabel(recording())} recording`} label={`${countLabel(recording())} recording. Show the videos`} onPress={() => show({ kind: "videos" })} />
      </If>
      <If when={kept}>
        <StatusItem
          icon="bell"
          tint="warning"
          text={today() === 1 ? "1 event today" : `${countLabel(today())} events today`}
          label={`${countLabel(today())} events today. Show the events`}
          onPress={() => show({ kind: "events" })}
        />
      </If>
      <View class="flex-1" />
      <If when={usage()}>
        {(shown) => (
          <If when={shown().limit > 0} else={<Sizes usage={shown()} />}>
            <Gauge usage={shown()} />
          </If>
        )}
      </If>
    </View>
  );
}
