import { Canvas, type CanvasRenderingContext2D, type CanvasSize, Overlay, type Point, signal, Text, token, View } from "nsh";
import { TextView } from "nsh/base";
import { look, popupSurface } from "../../../dp/ui";
import { MARKDOWN } from "./document";
import { MAP_COLUMNS, type MapInk, type MapPicture } from "./minimap";
import { PAGE } from "./syntax";

/** The map's width, in points. */
export const MAP_WIDTH = 84;

const PAD = 4;

const PREVIEW_AFTER = 350;

const PREVIEW_WIDTH = 360;

/** What the map's preview shows of a place: its heading crumb and its lines, rendered when `page`. */
export type MapPreview = { crumb: string; text: string; page: boolean };

function inkOf(ink: MapInk): { color: string; alpha: number } {
  switch (ink) {
    case "text":
      return { color: token("muted-foreground"), alpha: 0.55 };
    case "heading":
      return { color: token("syntax-heading"), alpha: 1 };
    case "code":
      return { color: token("syntax-code"), alpha: 0.9 };
    case "link":
      return { color: token("syntax-link"), alpha: 0.9 };
    case "marker":
      return { color: token("syntax-marker"), alpha: 0.9 };
    case "quote":
      return { color: token("syntax-quote"), alpha: 0.8 };
    case "punctuation":
      return { color: token("syntax-punctuation"), alpha: 0.6 };
    case "band":
      return { color: token("muted"), alpha: 0.9 };
    case "frame":
      return { color: token("border"), alpha: 1 };
  }
}

const INKS: MapInk[] = ["band", "frame", "text", "punctuation", "quote", "marker", "link", "code", "heading"];

const THINNEST = 0.6;

function drawPicture(cx: CanvasRenderingContext2D, size: CanvasSize, picture: MapPicture, scale: number) {
  const across = (size.width - PAD * 2) / MAP_COLUMNS;
  for (const ink of INKS) {
    const { color, alpha } = inkOf(ink);
    cx.fillStyle = color;
    cx.globalAlpha = alpha;
    for (const bar of picture.bars) {
      if (bar.ink !== ink) continue;
      cx.fillRect(PAD + bar.x * across, bar.y * scale, Math.max(THINNEST, bar.width * across), Math.max(THINNEST, bar.height * scale));
    }
  }
  cx.globalAlpha = 1;
  cx.font = "700 9px sans-serif";
  cx.textBaseline = "top";
  let last = Number.NEGATIVE_INFINITY;
  for (const label of picture.labels) {
    const y = label.y * scale;
    if (y - last < 12) continue;
    last = y;
    const width = Math.min(size.width - PAD * 2, cx.measureText(label.text).width + 4);
    cx.fillStyle = token("background");
    cx.globalAlpha = 0.85;
    cx.fillRect(PAD - 1, y, width, 11);
    cx.globalAlpha = 1;
    cx.fillStyle = token("syntax-heading");
    cx.fillText(label.text, PAD + 1, y + 1, size.width - PAD * 2 - 2);
  }
}

function drawOver(cx: CanvasRenderingContext2D, size: CanvasSize, matches: number[], view: { top: number; height: number } | null, scale: number) {
  cx.fillStyle = look().found.mark;
  for (const at of matches) cx.fillRect(0, at * scale - 1, size.width, 2);
  if (!view) return;
  const top = view.top * scale;
  const height = Math.max(8, view.height * scale);
  cx.fillStyle = token("foreground");
  cx.globalAlpha = 0.08;
  cx.fillRect(0, top, size.width, height);
  cx.globalAlpha = 0.3;
  cx.fillRect(0, top, size.width, 1);
  cx.fillRect(0, top + height - 1, size.width, 1);
  cx.globalAlpha = 1;
}

/** The map of the whole file beside a view; `view` and `matches` are in the picture's units. */
export function FileMap(props: {
  label: string;
  testID?: string;
  picture: MapPicture;
  scale: (height: number) => number;
  view: { top: number; height: number } | null;
  matches: number[];
  onGo: (unit: number) => void;
  preview: (unit: number) => MapPreview | null;
}) {
  const height = signal(0);
  const shown = signal<{ unit: number; anchor: Point } | null>(null);
  let resting: ReturnType<typeof setTimeout> | undefined;
  const unitAt = (y: number) => {
    const scale = props.scale(height());
    return scale > 0 ? Math.min(props.picture.extent, Math.max(0, y / scale)) : 0;
  };
  const hide = () => {
    clearTimeout(resting);
    shown.set(null);
  };
  const preview = () => {
    const at = shown();
    return at ? props.preview(at.unit) : null;
  };
  return (
    <View
      testID={props.testID}
      accessibilityLabel={props.label}
      class="h-full border-l border-border"
      style={{ width: MAP_WIDTH }}
      onLayout={(event) => height.set(event.bounds.size.height)}
      onPressDown={(event) => {
        hide();
        props.onGo(unitAt(event.position.y));
      }}
      onMouseMove={(event) => {
        if (event.pressedButton) {
          props.onGo(unitAt(event.position.y));
          return;
        }
        const place = { unit: unitAt(event.position.y), anchor: { x: event.pagePosition.x - event.position.x, y: event.pagePosition.y } };
        clearTimeout(resting);
        if (shown()) shown.set(place);
        else resting = setTimeout(() => shown.set(place), PREVIEW_AFTER);
      }}
      onMouseLeave={hide}
    >
      {/* Two canvases, so the picture redraws only when the file or the map's size changes. */}
      <Canvas class="absolute inset-0" draw={(cx, size) => drawPicture(cx, size, props.picture, props.scale(size.height))} />
      <Canvas class="absolute inset-0" draw={(cx, size) => drawOver(cx, size, props.matches, props.view, props.scale(size.height))} />
      <If when={shown() && preview()}>
        {(card) => (
          <Overlay anchor={shown()?.anchor ?? null} placement="left" offset={6} margin={12}>
            <View testID="map-preview" class={`gap-1.5 px-3 py-2 ${popupSurface().class}`} style={{ ...popupSurface().style, width: PREVIEW_WIDTH }}>
              <Text class="text-xs text-muted-foreground" style={{ numberOfLines: 1 }}>
                {card().crumb}
              </Text>
              <If when={card().page} else={<Text class="font-mono text-xs text-foreground">{card().text}</Text>}>
                <TextView document={TextView.parse(card().text, MARKDOWN)} classes={PAGE} />
              </If>
            </View>
          </Overlay>
        )}
      </If>
    </View>
  );
}
