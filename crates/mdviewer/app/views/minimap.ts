import type { EditorDecoration, TextViewBlock, TextViewDocument, TextViewInline } from "nsh/base";
import type { HeadingInfo, MarkInfo, MarkKind } from "mdviewer/native";
import { headingAt } from "./document";

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

function plainText(inlines: TextViewInline[]): string {
  return inlines
    .map((each) => {
      switch (each.type) {
        case "text":
        case "keyboard":
          return each.text;
        case "code":
          return each.code;
        case "math":
          return each.tex;
        case "image":
          return each.alt ?? "";
        case "custom":
          return each.text ?? "";
        case "break":
        case "footnoteReference":
          return " ";
        default:
          return plainText(each.content);
      }
    })
    .join("");
}

const textLines = (characters: number) => Math.max(1, Math.ceil(characters / 90));

/** About how tall `block` is on the page, in lines of text. */
export function blockWeight(block: TextViewBlock): number {
  switch (block.type) {
    case "heading":
      return block.level <= 2 ? 2.6 : 2;
    case "paragraph":
      return 0.8 + textLines(plainText(block.content).length);
    case "codeBlock":
      return 1.6 + block.code.split("\n").length * 0.9;
    case "list":
      return 0.6 + block.items.reduce((sum, item) => sum + item.children.reduce((inner, child) => inner + blockWeight(child) - 0.8, 0) + 0.2, 0);
    case "blockquote":
      return 1 + block.children.reduce((sum, child) => sum + blockWeight(child), 0);
    case "details":
      return 1.4 + (block.open ? block.children.reduce((sum, child) => sum + blockWeight(child), 0) : 0);
    case "table":
      return 1 + ((block.rows?.length ?? 0) + 1) * 1.3;
    case "rule":
      return 1.2;
    default:
      return 2;
  }
}

function blockBars(block: TextViewBlock, y: number, height: number, weight: number): MapBar[] {
  const lineHeight = height / Math.max(1, weight);
  const textRows = (top: number, count: number, indent: number, ink: MapInk = "text"): MapBar[] =>
    Array.from({ length: count }, (_, row) => ({
      y: top + row * lineHeight,
      height: lineHeight * 0.55,
      x: indent,
      width: (MAP_COLUMNS - indent) * (row === count - 1 && count > 1 ? 0.6 : 0.95),
      ink,
    }));
  switch (block.type) {
    case "heading": {
      const length = plainText(block.content).length;
      return [{ y: y + height * 0.3, height: Math.max(lineHeight * 0.9, height * 0.3), x: 0, width: Math.min(MAP_COLUMNS, 8 + length * 1.4), ink: "heading" }];
    }
    case "paragraph":
      return textRows(y, Math.max(1, Math.round(weight - 0.8)), 0);
    case "codeBlock": {
      const code = block.code.split("\n");
      return [
        { y, height: height * 0.92, x: 0, width: MAP_COLUMNS, ink: "band" },
        ...code.map(
          (line, row): MapBar => ({
            y: y + lineHeight * (0.8 + row * 0.9),
            height: lineHeight * 0.5,
            x: 3 + Math.min(20, line.length - line.trimStart().length),
            width: Math.min(MAP_COLUMNS - 6, line.trim().length),
            ink: "code",
          }),
        ),
      ];
    }
    case "list": {
      let top = y;
      return block.items.flatMap((item) => {
        const rows = Math.max(1, Math.round(item.children.reduce((sum, child) => sum + blockWeight(child) - 0.8, 0)));
        const marker: MapBar = { y: top, height: lineHeight * 0.55, x: 1, width: 2, ink: "marker" };
        const bars = [marker, ...textRows(top, rows, 5)];
        top += (rows + 0.2) * lineHeight;
        return bars;
      });
    }
    case "blockquote":
      return [{ y, height: height * 0.92, x: 0, width: 1.2, ink: "quote" }, ...textRows(y + lineHeight * 0.4, Math.max(1, Math.round(weight - 1)), 4, "quote")];
    case "table": {
      const rows = (block.rows?.length ?? 0) + 1;
      const rules = Array.from({ length: rows + 1 }, (_, row): MapBar => ({ y: y + row * 1.3 * lineHeight, height: lineHeight * 0.15, x: 0, width: MAP_COLUMNS, ink: "frame" }));
      const cells = Array.from({ length: rows }, (_, row): MapBar => ({ y: y + (row + 0.45) * 1.3 * lineHeight, height: lineHeight * 0.5, x: 2, width: MAP_COLUMNS * 0.7, ink: row === 0 ? "heading" : "text" }));
      return [...rules, ...cells];
    }
    case "rule":
      return [{ y: y + height / 2, height: lineHeight * 0.2, x: 0, width: MAP_COLUMNS, ink: "frame" }];
    default:
      return [{ y, height: height * 0.9, x: 0, width: MAP_COLUMNS, ink: "band" }];
  }
}

/** Rendered's map of the page, a unit a point; a section not measured yet is left out. */
export function pageMap(sections: TextViewDocument[], tops: (number | undefined)[], end: number): MapPicture {
  const bars: MapBar[] = [];
  const labels: MapLabel[] = [];
  const levels = sections.flatMap((section) => section.blocks.flatMap((block) => (block.type === "heading" ? [block.level] : [])));
  const top = Math.min(6, ...levels);
  sections.forEach((section, index) => {
    const start = tops[index];
    if (start === undefined) return;
    const stop = tops.slice(index + 1).find((each) => each !== undefined) ?? end;
    const weights = section.blocks.map(blockWeight);
    const total = weights.reduce((sum, weight) => sum + weight, 0);
    if (total <= 0 || stop <= start) return;
    let y = start;
    section.blocks.forEach((block, at) => {
      const height = ((stop - start) * weights[at]) / total;
      bars.push(...blockBars(block, y, height, weights[at]));
      if (block.type === "heading" && block.level <= top + 1) labels.push({ y, text: plainText(block.content) });
      y += height;
    });
  });
  return { extent: Math.max(end, 1), bars, labels };
}

/** The line of a file about at `offset` down its page; the inverse of `pageOffset`. */
export function lineAtPage(offset: number, headings: HeadingInfo[], lineCount: number, tops: (number | undefined)[], end: number): number {
  let section = 0;
  tops.forEach((top, index) => {
    if (top !== undefined && top <= offset) section = index;
  });
  const top = tops[section] ?? 0;
  const bottom = tops.slice(section + 1).find((each) => each !== undefined) ?? end;
  const first = section === 0 ? 0 : (headings[section - 1]?.line ?? 0);
  const last = section < headings.length ? headings[section].line : lineCount;
  const share = bottom > top ? Math.min(1, Math.max(0, (offset - top) / (bottom - top))) : 0;
  return Math.min(Math.max(0, lineCount - 1), Math.floor(first + (last - first) * share));
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
