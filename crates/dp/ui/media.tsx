import { type JSX, Text, View } from "nsh";
import { Toggle, ToggleGroup, Toolbar, ToolbarSeparator } from "nsh/base";
import { look, OVERLAY } from "./brand";
import { OnPill } from "./button";
import { CheckBox } from "./field";
import { Icon, type IconName } from "./icon";
import { Press } from "./press";

type Point = { x: number; y: number };

/** Room around each tile's picture and caption, for the hover fill. */
export const TILE_PAD = 6;

const tabular = { fontVariant: ["tabular-nums" as const] };

/** A recording's red dot; `pulse` makes it beat. */
export function LiveDot(props: { pulse?: boolean; label?: string }) {
  return <View accessibilityLabel={props.label} class={`size-2 rounded-full bg-destructive ${props.pulse ? "animate-pulse" : ""}`} />;
}

/** The red dot and REC, with how long it has run; white, for over a picture. */
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

/** Covers a picture that isn't live, dark so its last picture can't pass for live, and says what happened. */
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

/** A small dark label over a picture, which presses pass through. */
export function OverlayChip(props: { class?: string; children: JSX.Element }) {
  return (
    <View class={`flex-row items-center gap-1.5 px-1.5 py-0.5 ${OVERLAY.chip} ${props.class ?? ""}`} style={{ pointerEvents: "none" }}>
      {props.children}
    </View>
  );
}

/** Buttons over a picture on a dark chip, so they read on any picture. */
export function OverlayBar(props: { class?: string; children: JSX.Element }) {
  return <View class={`flex-row items-center ${OVERLAY.chip} ${props.class ?? ""}`}>{props.children}</View>;
}

/** A picture's toolbar on a dark pill: one focus stop, its items reached by the arrow keys. */
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

/** One choice of a few on a picture's toolbar; `value` is `""` while none is chosen. */
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

type Keys = { shift: boolean; control: boolean; platform: boolean };

const LIST_PICTURE = 80;

/** Lies over the tile's press, not in it, so pressing it leaves the tile closed. */
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

/** A file in a gallery, as a picture over its caption or a list row; `onCheck` hears its ring or checkbox. */
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
