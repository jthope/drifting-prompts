// A camera's live picture, as the stage's tiles and the layout editor's frames
// show it.
import { createAnimatedValue, type NativeComponentRef, nativeComponent, signal, WebView } from "nsh";
import { Cameras } from "ipcamera/native";

// Drawn by `src/video.rs`, which registers this name. Where nothing does (the
// web, headless tests) it is an empty view. The wheel over it zooms into the
// picture there, and it says how far in with `zoom`; `resetZoom` shows the
// whole picture again. `panX` and `panY` are every drag's moves added up,
// which move a zoomed picture.
const LiveVideo = nativeComponent<{ camera: string; stream: string; fit: string; panX: number; panY: number }, { zoom: { scale: number } }>("ipcamera.video", {
  props: ["camera", "stream", "fit", "panX", "panY"],
  events: ["zoom"],
});

/**
 * How far a picture is zoomed in, as its picture says (`scale`, 1 for the
 * whole picture), the way back to the whole picture, and the drags that move a
 * zoomed one (`pan`: animated values the drag drives natively, which the
 * picture reads). The wheel zooms where the app draws the picture itself, the
 * desktop; on the web the scale stays 1.
 */
export type PictureZoom = ReturnType<typeof pictureZoom>;

/** A picture's zoom, starting at the whole picture. */
export function pictureZoom() {
  const scale = signal(1);
  const pan = { x: createAnimatedValue(0), y: createAnimatedValue(0) };
  let video: NativeComponentRef | undefined;
  return {
    scale,
    pan,
    attach: (node: NativeComponentRef) => {
      video = node;
    },
    reset: () => video?.command("resetZoom"),
    /** Keeps where a drag left the picture: the next drag adds to it, so the values the picture reads never jump back. */
    dropped: () => {
      pan.x.extractOffset();
      pan.y.extractOffset();
    },
  };
}

/**
 * A camera's live picture, on `stream` (`""` for the camera's own). Where the
 * app doesn't draw it itself, it is a page that plays it (`Cameras.liveUrl`):
 * on the web, the server's player. That page takes no presses, so the tile
 * under it keeps its menu. The desktop draws the picture with Direct2D, which
 * the wheel zooms into and a drag moves while zoomed (`zoom`). Both are drawn
 * as part of the view they are in, so a transform that moves the view moves
 * the picture with it, in the same frame.
 */
export function Picture(props: { camera: string; stream: string; fit: string; zoom: PictureZoom }) {
  const url = () => Cameras.liveUrl(props.camera, props.stream);
  const zoomed = () => props.zoom.scale() > 1;
  return (
    <If
      when={url()}
      else={
        <LiveVideo
          ref={props.zoom.attach}
          camera={props.camera}
          stream={props.stream}
          fit={props.fit}
          panX={props.zoom.pan.x}
          panY={props.zoom.pan.y}
          // Only a zoomed picture is dragged, so a drag elsewhere stays a press on the tile.
          pan={zoomed() ? props.zoom.pan : undefined}
          onPanEnd={props.zoom.dropped}
          onZoom={(zoom) => props.zoom.scale.set(zoom.scale)}
          style={{ flex: 1, cursor: zoomed() ? "grab" : undefined }}
        />
      }
    >
      <WebView src={url()} scrollable={false} style={{ flex: 1, pointerEvents: "none" }} />
    </If>
  );
}
