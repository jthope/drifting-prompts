import { Canvas, type CanvasRenderingContext2D, Path2D, token } from "nsh";
import { lucide, type LucideElement, type LucideName } from "./lucide";

export type IconName = LucideName;

/** The theme colors an icon can be drawn in. */
export type IconTint =
  | "foreground"
  | "muted-foreground"
  | "primary-foreground"
  | "secondary-foreground"
  | "accent-foreground"
  | "popover-foreground"
  | "sidebar-foreground"
  | "success"
  | "warning"
  | "info"
  | "destructive"
  | "chart-1"
  | "chart-2"
  | "chart-3"
  | "chart-4"
  | "chart-5"
  | "icon";

/** Lucide's icons are stroked, never filled. */
function draw(cx: CanvasRenderingContext2D, element: LucideElement) {
  switch (element.tag) {
    case "path":
      cx.stroke(new Path2D(element.d));
      return;
    case "circle":
      cx.beginPath();
      cx.arc(element.cx, element.cy, element.r, 0, Math.PI * 2);
      break;
    case "rect":
      cx.beginPath();
      cx.roundRect(element.x, element.y, element.width, element.height, element.rx ?? 0);
      break;
    case "line":
      cx.beginPath();
      cx.moveTo(element.x1, element.y1);
      cx.lineTo(element.x2, element.y2);
      break;
  }
  cx.stroke();
}

/** A Lucide icon; `token` is read in `draw` to keep step with the appearance, and `color` replaces the tint. */
export function Icon(props: { name: IconName; size?: number; tint?: IconTint; color?: string }) {
  const size = () => props.size ?? 16;
  return (
    <Canvas
      style={{ pointerEvents: "none", width: size(), height: size() }}
      draw={(cx, { width }) => {
        cx.scale(width / 24, width / 24);
        cx.strokeStyle = props.color ?? token(props.tint ?? "foreground");
        cx.lineWidth = 2;
        cx.lineCap = "round";
        cx.lineJoin = "round";
        for (const element of lucide[props.name] as LucideElement[]) draw(cx, element);
      }}
    />
  );
}
