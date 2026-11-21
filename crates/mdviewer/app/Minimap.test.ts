import { expect, test } from "nsh/test";
import type { HeadingInfo, MarkInfo } from "mdviewer/native";
import { blockAround, MAP_COLUMNS, MARKS, onTrack, pageMap, railLines, rawMap, rulerMarks, sectionLines } from "./views/minimap";
import type { PageBlock } from "./views/pagemessages";

const heading = (level: number, title: string, line: number): HeadingInfo => ({ level, title, line });

const HEADINGS = [heading(1, "One", 0), heading(2, "Two", 10), heading(3, "Three", 20), heading(1, "Four", 30)];

test("a section runs from its heading to the line before the next one", () => {
  expect(sectionLines(HEADINGS, 1, 41)).toEqual({ first: 10, last: 19 });
  expect(sectionLines(HEADINGS, 3, 41)).toEqual({ first: 30, last: 40 });
  expect(sectionLines([heading(1, "Late", 5)], -1, 41)).toEqual({ first: 0, last: 4 });
});

test("Raw's ruler has a dash for each heading on its left, and the reader's section on its right", () => {
  const marks = rulerMarks(HEADINGS, 1, 41);
  expect(marks.filter((mark) => mark.overviewRuler?.lane === "left").map((mark) => mark.range.start.line)).toEqual([0, 10, 20, 30]);
  const section = marks.find((mark) => mark.overviewRuler?.lane === "right");
  expect(section?.range).toEqual({ start: { line: 10, column: 0 }, end: { line: 19, column: 0 } });
  expect(section?.overviewRuler?.class).toBe(MARKS.section);
  // Above the first heading, no section is marked.
  expect(rulerMarks(HEADINGS, -1, 41).length).toBe(4);
});

test("a place on the page falls on its scroll bar in proportion, never past its ends", () => {
  expect(onTrack(500, 2000, 400)).toBe(100);
  expect(onTrack(3000, 2000, 400)).toBe(400);
  expect(onTrack(10, 0, 400)).toBe(0);
});

test("Raw's map is each line's runs of words, its marks in their colors, a band behind code, and the top headings' names", () => {
  const text = "# Title\n\nSome  words   apart\n```\ncode\n```\n### Deep";
  const marks: MarkInfo[] = [
    { kind: "heading", startLine: 0, startColumn: 2, endLine: 0, endColumn: 7 },
    { kind: "codeBlock", startLine: 3, startColumn: 0, endLine: 5, endColumn: 3 },
  ];
  const map = rawMap(text, marks, [heading(1, "Title", 0), heading(3, "Deep", 6)]);
  expect(map.extent).toBe(7);
  // Two spaces or more part the runs of a line; one doesn't.
  expect(map.bars.filter((bar) => bar.y === 2 && bar.ink === "text").map((bar) => [bar.x, bar.width])).toEqual([
    [0, 4],
    [6, 5],
    [14, 5],
  ]);
  expect(map.bars.find((bar) => bar.ink === "heading")).toEqual({ y: 0, height: 1, x: 2, width: 5, ink: "heading" });
  expect(map.bars.find((bar) => bar.ink === "band")).toEqual({ y: 3, height: 3, x: 0, width: MAP_COLUMNS, ink: "band" });
  // The level-3 heading is two below the top one: too deep to be named.
  expect(map.labels).toEqual([{ y: 0, text: "Title" }]);
});

test("the page's map draws each block where the page drew it, and names the top headings", () => {
  const block = (kind: string, line: number, top: number, height: number, depth = 0, endLine = line): PageBlock => ({ line, endLine, top, height, kind, depth });
  const blocks = [block("p", 0, 24, 51), block("h1", 2, 100, 40), block("ul", 4, 150, 60, 0, 5), block("li", 4, 150, 26, 1), block("li", 5, 180, 26, 1), block("pre", 7, 220, 70, 0, 10)];
  const map = pageMap(blocks, [heading(1, "One", 2)], 400);
  expect(map.extent).toBe(400);
  expect(map.labels).toEqual([{ y: 100, text: "One" }]);
  // Two lines of text in the paragraph, a marker for each item, a band behind the code.
  expect(map.bars.filter((bar) => bar.ink === "text" && bar.y < 100).length).toBe(2);
  expect(map.bars.filter((bar) => bar.ink === "marker").map((bar) => Math.round(bar.y))).toEqual([155, 185]);
  expect(map.bars.find((bar) => bar.ink === "band")).toEqual({ y: 220, height: 70, x: 0, width: MAP_COLUMNS, ink: "band" });
});

test("the preview shows the block of lines around a line, under its heading", () => {
  const lines = ["# One", "", "a", "b", "c", "", "d"];
  expect(blockAround(lines, 3, [heading(1, "One", 0)])).toEqual({ first: 2, lines: ["a", "b", "c"], heading: 0 });
  const long = Array.from({ length: 40 }, (_, at) => `line ${at}`);
  const block = blockAround(long, 20, [], 6);
  expect(block.lines.length).toBe(6);
  expect(block.first).toBe(17);
  expect(block.heading).toBe(-1);
});

test("the rail places each heading by its line's share of the file, longer the higher its level", () => {
  // 200 points of room between the margins; the last line, 40, at the bottom.
  const lines = railLines(HEADINGS, 41, 232, 16);
  expect(lines.map((line) => line.top)).toEqual([16, 66, 116, 166]);
  expect(lines.map((line) => line.width)).toEqual([20, 14, 10, 20]);
  expect(lines.map((line) => line.index)).toEqual([0, 1, 2, 3]);
});
