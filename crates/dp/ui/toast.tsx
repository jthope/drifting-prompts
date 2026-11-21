import { Overlay, signal, Text } from "nsh";
import { Toast, type ToastEntry, ToastManager, ToastStack } from "nsh/base";
import { Button } from "./button";
import { popupSurface } from "./menu";
import { StatusDot, type StatusTone } from "./status";

/** In milliseconds. */
const DURATION = 4000;
const LIMIT = 3;
/** How often the lifecycle moves on, in milliseconds. */
const TICK = 50;

const manager = new ToastManager();
const shown = signal<ToastEntry[]>([]);
const expanded = signal(false);
/** A button on a message: what it says, and what it does. */
export type NoticeAction = { label: string; onPress: () => void };

const notices = signal<Record<string, { text: string; tone: StatusTone; action?: NoticeAction }>>({});

let ticking: ReturnType<typeof setInterval> | undefined;
let count = 0;

function tick() {
  const advanced = manager.advance(expanded());
  if (advanced.removed.length > 0) {
    const left = { ...notices() };
    for (const gone of advanced.removed) delete left[gone.id];
    notices.set(left);
  }
  if (advanced.changed) shown.set(manager.visible(LIMIT));
  if (manager.isEmpty() && ticking !== undefined) {
    clearInterval(ticking);
    ticking = undefined;
  }
}

/** Shows a message for a few seconds, with `action`'s button when it has one. */
export function notify(text: string, tone: StatusTone = "normal", action?: NoticeAction) {
  const id = `notice-${++count}`;
  notices.set({ ...notices(), [id]: { text, tone, action } });
  manager.push(id, id, { timeout: DURATION });
  shown.set(manager.visible(LIMIT));
  ticking ??= setInterval(tick, TICK);
}

function Notice(props: { entry: ToastEntry }) {
  const notice = () => notices()[props.entry.id] ?? { text: "", tone: "normal" as StatusTone };
  const surface = () => popupSurface("flex-row items-center gap-2.5 px-3 py-2.5 transition-all duration-200 ease-out");
  return (
    <Toast
      transitionStatus={props.entry.status}
      accessibilityLabel={notice().text}
      onDismissRequest={() => manager.dismiss(props.entry.id)}
      class={surface().class}
      style={surface().style}
      startingClass="translate-y-4 opacity-0"
      endingClass="scale-95 opacity-0"
      swipingClass="transition-none"
    >
      <StatusDot tone={notice().tone} />
      <Text class="flex-1 text-sm text-popover-foreground">{notice().text}</Text>
      <If when={notice().action}>
        {(action) => (
          <Button
            label={action().label}
            onPress={() => {
              manager.dismiss(props.entry.id);
              action().onPress();
            }}
          />
        )}
      </If>
    </Toast>
  );
}

/** Where the messages show: the bottom right of the window. */
export function Toaster() {
  return (
    <Overlay style={{ pointerEvents: "box-none", left: 0, right: 0, bottom: 0, padding: 16, alignItems: "flex-end" }}>
      <ToastStack placement="bottomRight" style={{ width: 340, maxWidth: "100%" }} onExpandedChange={(event) => expanded.set(event.expanded)}>
        <Each of={shown()} key={(entry: ToastEntry) => entry.id}>
          {(entry) => <Notice entry={entry()} />}
        </Each>
      </ToastStack>
    </Overlay>
  );
}
