// The map beside a view of the file: the whole file drawn small on nsh's
// `Canvas` (its picture, `MapPicture` in `minimap.ts`), the headings' names
// written over it, a band across it for each match of the search, and a lighter
// band for the part in view. A press on the map takes the view there, and a drag
// keeps it there as the pointer goes. Where the pointer rests, a card shows that
// part of the file larger, beside the map: the heading it is under and its
// lines, as written beside Raw and as the page shows them beside Rendered.
//
// The picture and what moves over it are two canvases, one over the other: the
// picture draws again only when the file or the map's size changes, the band in
// view as often as the view scrolls.
import { Canvas, type CanvasRenderingContext2D, type CanvasSize, Overlay, type Point, signal, Text, token, View } from "nsh";
import { TextView } from "nsh/base";
import { look, popupSurface } from "../../../dp/ui";
import { MARKDOWN } from "./document";
import { MAP_COLUMNS, type MapInk, type MapPicture } from "./minimap";
import { PAGE } from "./syntax";

/** The map's width, in points. */
export const MAP_WIDTH = 84;

/** The room at the map's sides, in points. */
const PAD = 4;

/** How long the pointer rests on the map before the preview shows, in milliseconds. */
const PREVIEW_AFTER = 350;

/** The preview's width, in points. */
const PREVIEW_WIDTH = 360;

/** What the map's preview shows of a place: the heading it is under and where (`crumb`), and its lines, as the page shows them with `page`, else as written. */
export type MapPreview = { crumb: string; text: string; page: boolean };

/** Each ink's theme color, and how strongly it shows. */
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

/** The order the inks draw in: the bands under everything. */
const INKS: MapInk[] = ["band", "frame", "text", "punctuation", "quote", "marker", "link", "code", "heading"];

/** The thinnest a bar is drawn, in points, however small its share of the file. */
const THINNEST = 0.6;

/** Draws `picture` at `scale` points a unit on a canvas `size` big: its bars, then its headings' names, kept apart. */
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

/** Draws what moves over the picture: a band across for each match (`matches`), and the part in view (`view`), at `scale` points a unit. */
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

/**
 * The map: `picture`, at `scale(height)` points a unit on a map `height` tall;
 * `view`, the part in view, and `matches`, where the search's are, in the
 * picture's units. A press or a drag takes the view to a unit (`onGo`), and the
 * pointer resting shows `preview(unit)`.
 */
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
  /** Where the preview shows: the place in the picture's units, and the map's edge at the pointer, in the window. */
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
        // Once it shows, the preview follows the pointer; before, it waits for the pointer to rest.
        if (shown()) shown.set(place);
        else resting = setTimeout(() => shown.set(place), PREVIEW_AFTER);
      }}
      onMouseLeave={hide}
    >
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
