// Header textures in low-alpha black and white. No shader: nsh's opens a GPU device of its own and
// redraws every frame.
import { type CanvasRenderingContext2D, type CanvasSize, colorScheme, signal } from "nsh";

export type TextureName = "none" | "haze" | "dots";

/** The textures, in the order Settings offers them. */
export const TEXTURES: { value: TextureName; title: string; label?: string; description: string }[] = [
  { value: "none", title: "None", label: "No texture", description: "Page headers in the theme's plain color." },
  { value: "haze", title: "Haze", description: "A soft light falling on page headers from the top right." },
  { value: "dots", title: "Dots", description: "A soft light on page headers, picking out a grid of fine dots where it falls." },
];

/** The texture the app shows. */
export const texture = signal<TextureName>("none");

/** `name` as a texture: the default for an unknown name. */
export function textureNamed(name: string): TextureName {
  return TEXTURES.find((each) => each.value === name)?.value ?? "none";
}

const black = (alpha: number) => `rgb(0 0 0 / ${alpha})`;
const white = (alpha: number) => `rgb(255 255 255 / ${alpha})`;

const rounded = (value: number, places: number) => Number(value.toFixed(places));

/** A pool of light: its center as fractions of the header, its reach as a fraction of its height. */
type Pool = { x: number; y: number; reach: number; dark: number; light: number };

const GLOW: Pool = { x: 0.82, y: 0.08, reach: 0.75, dark: 0.13, light: 0.09 };
const LOW: Pool = { x: 0.53, y: 1.25, reach: 0.7, dark: 0.07, light: 0.05 };

const BELL = [0, 0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2, 2.5];

function paint(pool: Pool, height: number, ink: (alpha: number) => string, dark: boolean): string {
  const peak = dark ? pool.dark : pool.light;
  const reach = pool.reach * height;
  const last = BELL.length - 1;
  const stops = BELL.map((at, index) => `${ink(index === last ? 0 : rounded(peak * Math.exp(-at * at), 4))} ${rounded(at * reach, 1)}px`);
  return `radial-gradient(circle at ${rounded(pool.x * 100, 2)}% ${rounded(pool.y * 100, 2)}%, ${stops.join(", ")})`;
}

const GAP = 14;
const DOT = 0.85;
const DOT_INK = { dark: 0.2, light: 0.16 };
const DOT_FADE: [number, number][] = [
  [0, 1],
  [0.52, 1],
  [0.75, 0.7],
  [1, 0.4],
  [1.25, 0.17],
  [1.52, 0],
];

/** Draws the dots the glow picks out on a header `size` big, as one path. */
export function drawDots(cx: CanvasRenderingContext2D, size: CanvasSize, ink: (alpha: number) => string, alpha: number) {
  const center = { x: GLOW.x * size.width, y: GLOW.y * size.height };
  const end = DOT_FADE[DOT_FADE.length - 1][0] * GLOW.reach * size.height;
  cx.beginPath();
  for (let y = GAP / 2; y < size.height; y += GAP) {
    for (let x = GAP / 2; x < size.width; x += GAP) {
      if ((x - center.x) ** 2 + (y - center.y) ** 2 >= end * end) continue;
      cx.moveTo(x + DOT, y);
      cx.arc(x, y, DOT, 0, Math.PI * 2);
    }
  }
  const fade = cx.createRadialGradient(center.x, center.y, 0, center.x, center.y, end);
  const last = DOT_FADE[DOT_FADE.length - 1][0];
  for (const [at, strength] of DOT_FADE) fade.addColorStop(at / last, ink(rounded(alpha * strength, 4)));
  cx.fillStyle = fade;
  cx.fill();
}

/** How a texture draws over a header: a `backgroundImage` paint, and what a `Canvas` over it draws. */
export type Finish = { light: string; dots?: (cx: CanvasRenderingContext2D, size: CanvasSize) => void };

/** How `name` draws over a header `height` points tall, or undefined for none. */
export function headerTexture(height: number, name: TextureName = texture()): Finish | undefined {
  if (name === "none") return undefined;
  const dark = colorScheme() === "dark";
  const ink = dark ? white : black;
  const light = `${paint(GLOW, height, ink, dark)}, ${paint(LOW, height, ink, dark)}`;
  if (name === "haze") return { light };
  const alpha = dark ? DOT_INK.dark : DOT_INK.light;
  return { light, dots: (cx, size) => drawDots(cx, size, ink, alpha) };
}
