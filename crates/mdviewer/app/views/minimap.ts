import type { EditorDecoration } from "nsh/base";
import type { HeadingInfo, MarkInfo, MarkKind } from "mdviewer/native";
import { headingAt } from "./document";
import type { PageBlock } from "./pagemessages";

/** The marks' classes, in a table so nsh's class-name check, which doesn't know `syntax-heading`, leaves them alone. */
export const MARKS = { heading: "bg-syntax-heading", section: "bg-ring", line: "bg-muted-foreground/50" };

/** The lines section `index` spans; -1 is before the first heading. */
export function sectionLines(headings: HeadingInfo[], index: number, lineCount: number): { first: number; last: number } {
  const first = index < 0 ? 0 : (headings[index]?.line ?? 0);
  const next = headings[index + 1]?.line ?? lineCount;
  return { first, last: Math.max(first, next - 1) };
}

/** Raw's overview ruler marks: the headings, and the section of heading `current` (-1 for none). */
export function rulerMarks(headings: HeadingInfo[], current: number, lineCount: number): EditorDecoration[] {
  const marks: EditorDecoration[] = headings.map((heading) => ({
    range: { start: { line: heading.line, column: 0 }, end: { line: heading.line, column: 0 } },
    overviewRuler: { lane: "left", class: MARKS.heading },
  }));
  if (current >= 0 && current < headings.length) {
    const { first, last } = sectionLines(headings, current, lineCount);
    marks.push({ range: { start: { line: first, column: 0 }, end: { line: last, column: 0 } }, overviewRuler: { lane: "right", class: MARKS.section } });
  }
  return marks;
}

/** Where `offset` down a page `content` tall falls on a scroll bar `track` tall. */
export function onTrack(offset: number, content: number, track: number): number {
  if (content <= 0) return 0;
  return Math.min(track, Math.max(0, (offset / content) * track));
}

const RAIL_WIDTHS = [20, 14, 10, 7];

/** A line of the heading rail: its heading's index, how far down, and how long. */
export type RailLine = { index: number; top: number; width: number };

/** The heading rail's lines, placed by each heading's line's share of the file. */
export function railLines(headings: HeadingInfo[], lineCount: number, height: number, margin: number): RailLine[] {
  const top = Math.min(6, ...headings.map((heading) => heading.level));
  const room = Math.max(0, height - margin * 2);
  const last = Math.max(1, lineCount - 1);
  return headings.map((heading, index) => ({
    index,
    top: margin + Math.min(1, heading.line / last) * room,
    width: RAIL_WIDTHS[Math.min(RAIL_WIDTHS.length - 1, heading.level - top)],
  }));
}

/** What a bar of the map is, which picks its color. */
export type MapInk = "text" | "heading" | "code" | "link" | "marker" | "quote" | "punctuation" | "band" | "frame";

/** A bar of the map: `x` and `width` in `MAP_COLUMNS`, `y` and `height` in the picture's units. */
export type MapBar = { y: number; height: number; x: number; width: number; ink: MapInk };

/** A heading's name written over the map. */
export type MapLabel = { y: number; text: string };

/** What a map draws, in units of which the file is `extent` long. */
export type MapPicture = { extent: number; bars: MapBar[]; labels: MapLabel[] };

/** How many columns of text the map's width stands for. */
export const MAP_COLUMNS = 80;

const MOST_MAP_LINES = 4000;

const MARK_INKS: Partial<Record<MarkKind, MapInk>> = {
  heading: "heading",
  code: "code",
  link: "link",
  url: "link",
  marker: "marker",
  quote: "quote",
  punctuation: "punctuation",
};

function labeled(headings: HeadingInfo[]): HeadingInfo[] {
  const top = Math.min(6, ...headings.map((heading) => heading.level));
  return headings.filter((heading) => heading.level <= top + 1);
}

/** Raw's map of `text`, a unit a line. */
export function rawMap(text: string, marks: MarkInfo[], headings: HeadingInfo[]): MapPicture {
  const lines = text.split("\n");
  const step = Math.max(1, Math.ceil(lines.length / MOST_MAP_LINES));
  const drawn = (line: number) => line % step === 0;
  const bars: MapBar[] = [];
  const clip = (x: number, width: number) => Math.max(0, Math.min(width, MAP_COLUMNS - x));
  for (const mark of marks) {
    if (mark.kind !== "codeBlock" && mark.kind !== "frontMatter") continue;
    bars.push({ y: mark.startLine, height: mark.endLine - mark.startLine + 1, x: 0, width: MAP_COLUMNS, ink: "band" });
  }
  lines.forEach((line, index) => {
    if (!drawn(index)) return;
    for (const run of line.replace(/\r$/, "").matchAll(/\S+(?: \S+)*/g)) {
      const x = run.index ?? 0;
      if (x >= MAP_COLUMNS) break;
      bars.push({ y: index, height: step, x, width: clip(x, run[0].length), ink: "text" });
    }
  });
  for (const mark of marks) {
    const ink = MARK_INKS[mark.kind];
    if (!ink || mark.startLine !== mark.endLine || !drawn(mark.startLine) || mark.startColumn >= MAP_COLUMNS) continue;
    bars.push({ y: mark.startLine, height: step, x: mark.startColumn, width: clip(mark.startColumn, mark.endColumn - mark.startColumn), ink });
  }
  return { extent: lines.length, bars, labels: labeled(headings).map((heading) => ({ y: heading.line, text: heading.title })) };
}

// The page's line heights, in CSS pixels: its text's, and its code's.
const TEXT_LINE = 25.5;
const CODE_LINE = 20.8;

const INDENT = 4;

/** How far `block`'s own text runs: to the first block inside it, or its end. */
function ownHeight(blocks: PageBlock[], at: number): number {
  const block = blocks[at];
  const inner = blocks[at + 1];
  return inner && inner.depth > block.depth && inner.top < block.top + block.height ? Math.max(0, inner.top - block.top) : block.height;
}

function textRows(y: number, height: number, lineHeight: number, indent: number, ink: MapInk): MapBar[] {
  const count = Math.max(1, Math.round(height / lineHeight));
  return Array.from({ length: count }, (_, row) => ({
    y: y + row * lineHeight + lineHeight * 0.2,
    height: lineHeight * 0.55,
    x: indent,
    width: Math.max(1, (MAP_COLUMNS - indent) * (row === count - 1 && count > 1 ? 0.6 : 0.95)),
    ink,
  }));
}

/** Rendered's map from where the page drew its blocks, a unit a CSS pixel of a page `end` tall. */
export function pageMap(blocks: PageBlock[], headings: HeadingInfo[], end: number): MapPicture {
  const bars: MapBar[] = [];
  const titles = new Map(labeled(headings).map((heading) => [heading.line, heading.title]));
  const labels: MapLabel[] = [];
  blocks.forEach((block, at) => {
    const indent = Math.min(MAP_COLUMNS / 2, block.depth * INDENT);
    const own = ownHeight(blocks, at);
    switch (block.kind) {
      case "h1":
      case "h2":
      case "h3":
      case "h4":
      case "h5":
      case "h6": {
        const title = block.depth === 0 ? titles.get(block.line) : undefined;
        if (title !== undefined) labels.push({ y: block.top, text: title });
        const length = headings.find((heading) => heading.line === block.line)?.title.length ?? 20;
        bars.push({ y: block.top + block.height * 0.25, height: Math.max(4, block.height * 0.45), x: indent, width: Math.min(MAP_COLUMNS - indent, 8 + length * 1.4), ink: "heading" });
        return;
      }
      case "p":
      case "dl":
        bars.push(...textRows(block.top, own, TEXT_LINE, indent, "text"));
        return;
      case "li":
        bars.push({ y: block.top + TEXT_LINE * 0.2, height: TEXT_LINE * 0.55, x: indent, width: 2, ink: "marker" });
        if (own > TEXT_LINE / 2) bars.push(...textRows(block.top, own, TEXT_LINE, indent + INDENT, "text"));
        return;
      case "pre":
        bars.push({ y: block.top, height: block.height, x: indent, width: MAP_COLUMNS - indent, ink: "band" });
        bars.push(...textRows(block.top + 12, block.height - 24, CODE_LINE, indent + 3, "code"));
        return;
      case "blockquote":
        bars.push({ y: block.top, height: block.height, x: indent, width: 1.2, ink: "quote" });
        return;
      case "tr":
        bars.push({ y: block.top, height: 1, x: indent, width: MAP_COLUMNS - indent, ink: "frame" });
        bars.push({ y: block.top + block.height * 0.3, height: block.height * 0.4, x: indent + 2, width: (MAP_COLUMNS - indent) * 0.7, ink: blocks[at - 1]?.kind === "table" ? "heading" : "text" });
        return;
      case "hr":
        bars.push({ y: block.top + block.height / 2, height: 1, x: 0, width: MAP_COLUMNS, ink: "frame" });
        return;
      case "ul":
      case "ol":
      case "table":
        return;
      default:
        bars.push(own < block.height ? { y: block.top, height: block.height, x: indent, width: 1.2, ink: "marker" } : { y: block.top, height: block.height, x: indent, width: MAP_COLUMNS - indent, ink: "band" });
    }
  });
  return { extent: Math.max(end, 1), bars, labels };
}

/** The block of `lines` between blank lines around `line`, at most `most`, and its heading's index. */
export function blockAround(lines: string[], line: number, headings: HeadingInfo[], most = 12): { first: number; lines: string[]; heading: number } {
  const at = Math.min(Math.max(0, line), Math.max(0, lines.length - 1));
  let first = at;
  let last = at;
  while (first > 0 && lines[first - 1].trim() !== "" && at - first < most) first -= 1;
  while (last < lines.length - 1 && lines[last + 1].trim() !== "" && last - at < most) last += 1;
  if (last - first + 1 > most) {
    first = Math.max(first, at - Math.floor(most / 2));
    last = first + most - 1;
  }
  return { first, lines: lines.slice(first, last + 1), heading: headingAt(headings, at) };
}
