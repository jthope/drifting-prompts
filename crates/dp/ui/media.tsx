// What pictures and recordings share: the tile a gallery shows each in, at
// its size or as a row of a list, with the ring that selects it; the small
// labels over a picture's corner; and the red mark of a recording, which stays
// red whatever the theme.
import { type JSX, Text, View } from "nsh";
import { Toggle, ToggleGroup, Toolbar, ToolbarSeparator } from "nsh/base";
import { look, OVERLAY } from "./brand";
import { OnPill } from "./button";
import { CheckBox } from "./field";
import { Icon, type IconName } from "./icon";
import { Press } from "./press";

type Point = { x: number; y: number };

/** Around each tile's picture and caption: the hover fill's room. */
export const TILE_PAD = 6;

const tabular = { fontVariant: ["tabular-nums" as const] };

/** A recording's red dot. `pulse` makes it beat, for a line that says something records now; `label` names it for a screen reader. */
export function LiveDot(props: { pulse?: boolean; label?: string }) {
  return <View accessibilityLabel={props.label} class={`size-2 rounded-full bg-destructive ${props.pulse ? "animate-pulse" : ""}`} />;
}

/** Says a camera is recording: the red dot and REC, and how long it has run when `length` says. White, for over a picture. */
export function RecMark(props: { length?: string }) {
  return (
    <View accessibilityLabel={props.length ? `Recording, ${props.length}` : "Recording"} class="flex-row items-center gap-1">
      <LiveDot />
      <Text class="text-xs font-semibold text-white" style={tabular}>
        {props.length ? `REC ${props.length}` : "REC"}
      </Text>
    </View>
  );
}

/**
 * Covers a picture that isn't live: a dark layer over all of it, so a last
 * picture still drawn under it can't pass for live, and in its middle an icon,
 * a `title`, `text` saying what happened, a quieter `detail` and the way
 * forward (`action`). `compact` is a view's tile, smaller than a camera's own
 * page. A press on it, its button's aside, is the view's it is in, as one on
 * the picture would be.
 */
export function PictureNotice(props: { icon: IconName; title?: string; text: string; detail?: string; action?: JSX.Element; compact?: boolean }) {
  const lines = { numberOfLines: 3 };
  return (
    <View class={`absolute inset-0 items-center justify-center ${OVERLAY.dim} ${props.compact ? "gap-1.5 p-2" : "gap-2 p-6"}`}>
      <Icon name={props.icon} size={props.compact ? 18 : 28} color={OVERLAY.ink} />
      <View class="max-w-96 items-center gap-0.5">
        <If when={props.title}>
          <Text class={`text-center font-semibold ${OVERLAY.notice.title} ${props.compact ? "text-xs" : "text-sm"}`} style={{ numberOfLines: 1 }}>
            {props.title}
          </Text>
        </If>
        <Text class={`text-center ${OVERLAY.notice.text} ${props.compact ? "text-xs" : "text-sm"}`} style={lines}>
          {props.text}
        </Text>
        <If when={props.detail}>
          <Text class={`text-center text-xs ${OVERLAY.notice.detail}`} style={{ numberOfLines: 1, ...tabular }}>
            {props.detail}
          </Text>
        </If>
      </View>
      {props.action}
    </View>
  );
}

/** A small dark label over a picture, which presses pass through. `class` places it (`absolute bottom-1.5 right-1.5`). */
export function OverlayChip(props: { class?: string; children: JSX.Element }) {
  return (
    <View class={`flex-row items-center gap-1.5 px-1.5 py-0.5 ${OVERLAY.chip} ${props.class ?? ""}`} style={{ pointerEvents: "none" }}>
      {props.children}
    </View>
  );
}

/**
 * Buttons over a picture (`kind="overlay"`), on the same dark chip as a tile's
 * name, so they read on a bright picture as on a dark one. `class` places it.
 */
export function OverlayBar(props: { class?: string; children: JSX.Element }) {
  return <View class={`flex-row items-center ${OVERLAY.chip} ${props.class ?? ""}`}>{props.children}</View>;
}

/**
 * A picture's toolbar: buttons over it (`kind="overlay"`), the stream it is on
 * (a `Button` of that kind), a switch where one fits (`OverlaySegments`) and
 * lines between them (`OverlaySeparator`), on a dark
 * pill. Its buttons are pills too, one even margin inside it (`OnPill`), so the
 * one at either end follows its curve. It is one stop of the keyboard focus,
 * whose arrow keys move between its items, and a screen reader hears it as one
 * toolbar called `label`. `class` places it.
 */
export function OverlayToolbar(props: { label: string; class?: string; children: JSX.Element }) {
  return (
    <Toolbar accessibilityLabel={props.label} class={`flex-row items-center gap-0.5 p-0.5 ${OVERLAY.toolbar} ${props.class ?? ""}`}>
      <OnPill.Provider value={true}>{props.children}</OnPill.Provider>
    </Toolbar>
  );
}

/** A line between an `OverlayToolbar`'s groups. */
export function OverlaySeparator() {
  return <ToolbarSeparator class={`mx-1 h-4 w-px ${OVERLAY.separator}`} />;
}

/**
 * One choice of a few on a picture's toolbar, as `Segmented` is on a page:
 * toggles in a group, the chosen one solid white with dark ink. `value` is the
 * chosen one, or `""` while none is. An item's `label` is what a screen reader
 * says for it ("Main stream" for "Main").
 */
export function OverlaySegments<T extends string>(props: { items: { value: T; title: string; label: string }[]; value: T | ""; onChange: (value: T) => void }) {
  const on = (value: T) => props.value === value;
  return (
    <ToggleGroup class={`flex-row p-0.5 ${OVERLAY.segments.track}`}>
      <Each of={props.items} key={(item) => item.value}>
        {(item) => (
          <Toggle
            pressed={on(item().value)}
            accessibilityLabel={item().label}
            class={`h-6 justify-center px-2.5 ${OVERLAY.segments.segment} ${on(item().value) ? OVERLAY.segments.on : ""} ${look().rings.visible}`}
            onChange={() => props.onChange(item().value)}
          >
            <Text class={`text-xs ${on(item().value) ? OVERLAY.segments.onText : OVERLAY.segments.offText}`}>{item().title}</Text>
          </Toggle>
        )}
      </Each>
    </ToggleGroup>
  );
}

/** How a gallery shows its files: pictures in three sizes, the small ones without their captions, or a list of rows. */
export type TileSize = "large" | "medium" | "small" | "list";

/** The keys held at a press. */
type Keys = { shift: boolean; control: boolean; platform: boolean };

/** How wide a list row's picture is, in points. */
const LIST_PICTURE = 80;

/**
 * The ring over a picture's corner that puts it in the selection, or takes it
 * out; with Shift, every file from the one last picked. It shows while the
 * pointer is over its tile (`group/tile`), and always while `shown` (anything
 * on the page selected). Hidden, it still takes a press. It lies over the
 * tile's press rather than in it, so pressing it leaves the tile closed.
 */
function PictureCheck(props: { label: string; testID: string; on: boolean; shown: boolean; onPress: (range: boolean) => void }) {
  return (
    <View
      role="checkbox"
      accessibilityLabel={`Select ${props.label}`}
      accessibilityState={{ checked: props.on }}
      testID={props.testID}
      class={`absolute size-7 items-center justify-center ${props.shown ? "" : "opacity-0 group-hover/tile:opacity-100"}`}
      style={{ top: TILE_PAD + 2, left: TILE_PAD + 2 }}
      onPress={(event) => props.onPress(event.modifiers.shift)}
    >
      <View class={`size-5 items-center justify-center rounded-full ${props.on ? "border-2 border-primary bg-primary" : OVERLAY.check}`}>
        <If when={props.on}>
          <Icon name="check" size={12} tint="primary-foreground" />
        </If>
      </View>
    </View>
  );
}

/**
 * A file in a gallery: its picture over its title and a line under it
 * (`detail`), in a `size` the page picks. `picture` fills a 16:9 frame, on
 * black for a video and the muted color for a snapshot (`frame`). A small one
 * leaves the words out; in a list it is a row, with its picture small at the
 * start and `extra` at its end, a checkbox before it.
 *
 * A press goes to `onPress` with the keys held, which opens it or picks it
 * (`Library.press`); its ring or checkbox goes to `onCheck`, true for a range.
 * `selected` fills it as a selected row of a table is, and `selecting` (anything
 * on the page selected) shows every ring. The right button opens its menu.
 */
export function MediaTile(props: {
  label: string;
  testID: string;
  picture: JSX.Element;
  title: string;
  detail: string;
  extra?: string;
  frame?: "black" | "muted";
  size?: TileSize;
  selected?: boolean;
  selecting?: boolean;
  onPress: (keys: Keys) => void;
  onCheck?: (range: boolean) => void;
  onContextMenu: (at: Point) => void;
}) {
  const list = () => props.size === "list";
  const fill = () => (props.selected ? look().listRow.selected : look().listRow.hover);
  const titleText = () => (props.selected ? (look().listRow.selectedText ?? "text-foreground") : "text-foreground");
  const frame = () => `overflow-hidden rounded-md border border-border ${props.frame === "black" ? "bg-black" : "bg-muted"}`;
  const words = () => (
    <>
      <Text class={`text-sm font-medium ${titleText()}`} style={{ numberOfLines: 1 }}>
        {props.title}
      </Text>
      <Text class="text-xs text-muted-foreground" style={{ numberOfLines: 1, ...tabular }}>
        {props.detail}
      </Text>
    </>
  );
  const press = (event: { modifiers: Keys }) => props.onPress(event.modifiers);
  const menu = (event: { pagePosition: Point }) => props.onContextMenu(event.pagePosition);

  return (
    <View class={`group/tile min-w-0 flex-1 rounded-lg ${fill()}`}>
      <If
        when={list()}
        else={
          <Press label={props.label} testID={props.testID} selected={props.selected} class={`gap-2 rounded-lg ${look().rings.visible}`} onPress={press} onContextMenu={menu} padding={TILE_PAD}>
            <View class={frame()} style={{ aspectRatio: 16 / 9 }}>
              {props.picture}
            </View>
            <If when={props.size !== "small"}>
              <View class="gap-0.5 px-1 pb-0.5">{words()}</View>
            </If>
          </Press>
        }
      >
        <View class="flex-row items-center">
          <If when={props.onCheck}>
            {(check) => (
              <View class="items-center" style={{ width: 36 }}>
                <CheckBox label={`Select ${props.label}`} checked={props.selected === true} onChange={() => check()(false)} />
              </View>
            )}
          </If>
          <Press
            label={props.label}
            testID={props.testID}
            selected={props.selected}
            class={`min-w-0 flex-1 flex-row items-center gap-3 rounded-lg ${look().rings.visible}`}
            onPress={press}
            onContextMenu={menu}
            padding={TILE_PAD}
            scale={0.99}
          >
            <View class={frame()} style={{ width: LIST_PICTURE, aspectRatio: 16 / 9 }}>
              {props.picture}
            </View>
            <View class="min-w-0 flex-1 gap-0.5">{words()}</View>
            <If when={props.extra}>
              {(extra) => (
                <Text class="pr-2 text-xs text-muted-foreground" style={{ numberOfLines: 1, ...tabular }}>
                  {extra()}
                </Text>
              )}
            </If>
          </Press>
        </View>
      </If>
      <If when={list() ? undefined : props.onCheck}>
        {(check) => (
          <PictureCheck
            label={props.label}
            testID={`check:${props.testID}`}
            on={props.selected === true}
            shown={props.selecting === true || props.selected === true}
            onPress={(range) => check()(range)}
          />
        )}
      </If>
    </View>
  );
}
