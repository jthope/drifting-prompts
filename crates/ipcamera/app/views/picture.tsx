import { createAnimatedValue, type NativeComponentRef, nativeComponent, signal, WebView } from "nsh";
import { Cameras } from "ipcamera/native";

// Registered by `src/video.rs`; where nothing registers it (the web, headless tests) it is an empty view.
const LiveVideo = nativeComponent<
  { camera: string; stream: string; fit: string; panX: number; panY: number },
  { zoom: { scale: number }; shape: { width: number; height: number } }
>("ipcamera.video", {
  props: ["camera", "stream", "fit", "panX", "panY"],
  events: ["zoom", "shape"],
});

/** A width and a height: a place's in points, or a picture's shape. */
export type Size = { width: number; height: number };

/** How far a picture is zoomed in, the way back to the whole picture, and the drags that move a zoomed one. */
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
    /** Keeps where a drag left the picture, so the next drag adds to it rather than jumping back. */
    dropped: () => {
      pan.x.extractOffset();
      pan.y.extractOffset();
    },
  };
}

/** A camera's live picture on `stream`: drawn natively on the desktop, else the page `Cameras.liveUrl` gives, which takes no presses. */
export function Picture(props: { camera: string; stream: string; fit: string; zoom: PictureZoom; onShape?: (shape: Size | null) => void }) {
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
          onShape={(shape) => props.onShape?.(shape.width > 0 && shape.height > 0 ? { width: shape.width, height: shape.height } : null)}
          style={{ flex: 1, cursor: zoomed() ? "grab" : undefined }}
        />
      }
    >
      <WebView src={url()} scrollable={false} style={{ flex: 1, pointerEvents: "none" }} />
    </If>
  );
}
