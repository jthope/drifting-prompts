// The app's textures: a finish drawn over the theme on page headers (Settings >
// Appearance > Texture, saved with the other preferences). A texture is light
// and shade, white in the dark and black in the light at low alphas, so it
// sits on any theme's colors without bringing a color of its own.
//
// They draw natively, with the rest of the header: the light is two radial
// gradients, a paint for `backgroundImage`, and the dots a canvas over it.
// Nothing moves, so neither draws again until the header changes size, and
// both are laid out with it. A shader would have to do without them in nsh
// 0.4.0: it opens a GPU device of its own (about 145 MB), redraws every display
// frame while it reads the time, and shows its last frame stretched while the
// window is resized, so the header shook (`crates/README.md`, "Textures").
import { type CanvasRenderingContext2D, type CanvasSize, colorScheme, signal } from "nsh";

export type TextureName = "none" | "haze" | "dots";

/** The textures, in the order Settings offers them. */
export const TEXTURES: { value: TextureName; title: string; label?: string; description: string }[] = [
  { value: "none", title: "None", label: "No texture", description: "Page headers in the theme's plain color." },
  { value: "haze", title: "Haze", description: "A soft light falling on page headers from the top right." },
  { value: "dots", title: "Dots", description: "A soft light on page headers, picking out a grid of fine dots where it falls." },
];

/** The texture the app shows. The app sets it from the saved preferences before its first frame. */
export const texture = signal<TextureName>("none");

/**
 * `name` as a texture: the default for a name this version doesn't know, as a
 * hand-edited `settings.json` can have.
 */
export function textureNamed(name: string): TextureName {
  return TEXTURES.find((each) => each.value === name)?.value ?? "none";
}

const black = (alpha: number) => `rgb(0 0 0 / ${alpha})`;
const white = (alpha: number) => `rgb(255 255 255 / ${alpha})`;

const rounded = (value: number, places: number) => Number(value.toFixed(places));

/**
 * A pool of light: its center, as fractions of the header's width and height;
 * how far it reaches, as a fraction of the height (where it is a third as
 * bright); and its alpha at the center, on a dark header and on a light one.
 * Sizes follow the height, so a pool stays round however wide the header is.
 */
type Pool = { x: number; y: number; reach: number; dark: number; light: number };

/** The bright pool near the top right, whose light picks out the dots. */
const GLOW: Pool = { x: 0.82, y: 0.08, reach: 0.75, dark: 0.13, light: 0.09 };
/** A fainter one centered below the header, lighting it along the bottom. */
const LOW: Pool = { x: 0.53, y: 1.25, reach: 0.7, dark: 0.07, light: 0.05 };

/** Where a pool's stops fall, in its reach: a bell curve, a third as bright at 1 and gone by 2.5. */
const BELL = [0, 0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2, 2.5];

/** `pool` on a header `height` points tall, in `ink`, as a radial gradient. */
function paint(pool: Pool, height: number, ink: (alpha: number) => string, dark: boolean): string {
  const peak = dark ? pool.dark : pool.light;
  const reach = pool.reach * height;
  const last = BELL.length - 1;
  const stops = BELL.map((at, index) => `${ink(index === last ? 0 : rounded(peak * Math.exp(-at * at), 4))} ${rounded(at * reach, 1)}px`);
  return `radial-gradient(circle at ${rounded(pool.x * 100, 2)}% ${rounded(pool.y * 100, 2)}%, ${stops.join(", ")})`;
}

/** One dot every 14 points each way, in the middle of its cell. */
const GAP = 14;
/** A dot's radius: a point and three quarters across. */
const DOT = 0.85;
/** A dot's alpha where the glow is brightest, on a dark header and on a light one. */
const DOT_INK = { dark: 0.2, light: 0.16 };
/**
 * How strongly the glow picks out the dots, from its center out, in its reach:
 * fully out to half of it, then fading faster than its light does, so the
 * grid shows only where the light is strong.
 */
const DOT_FADE: [number, number][] = [
  [0, 1],
  [0.52, 1],
  [0.75, 0.7],
  [1, 0.4],
  [1.25, 0.17],
  [1.52, 0],
];

/**
 * Draws the dots the glow picks out on a header `size` big: only the ones
 * within its reach, as one path, filled with the glow's falloff in `ink` at
 * `alpha`.
 */
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

/**
 * How a texture draws over a page's header: its light, a paint for
 * `backgroundImage`, and what a `Canvas` over the light draws, if anything.
 */
export type Finish = { light: string; dots?: (cx: CanvasRenderingContext2D, size: CanvasSize) => void };

/**
 * How `name` (the app's texture unless given) draws over a page's header
 * `height` points tall, in the appearance shown, or undefined for none.
 */
export function headerTexture(height: number, name: TextureName = texture()): Finish | undefined {
  if (name === "none") return undefined;
  const dark = colorScheme() === "dark";
  const ink = dark ? white : black;
  const light = `${paint(GLOW, height, ink, dark)}, ${paint(LOW, height, ink, dark)}`;
  if (name === "haze") return { light };
  const alpha = dark ? DOT_INK.dark : DOT_INK.light;
  return { light, dots: (cx, size) => drawDots(cx, size, ink, alpha) };
}
