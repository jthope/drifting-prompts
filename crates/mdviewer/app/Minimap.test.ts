// Where the reader's marks of a whole file go (`views/minimap.ts`): the
// sections, Raw's ruler, a place on the page's scroll bar, the heading rail,
// and the maps of Raw and of the page, with the line under a place on the page
// and the block of lines its preview shows.
import { expect, test } from "nsh/test";
import { TextView } from "nsh/base";
import type { HeadingInfo, MarkInfo } from "mdviewer/native";
import { MARKDOWN, splitSections } from "./views/document";
import { blockAround, blockWeight, lineAtPage, MAP_COLUMNS, MARKS, onTrack, pageMap, railLines, rawMap, rulerMarks, sectionLines } from "./views/minimap";

const heading = (level: number, title: string, line: number): HeadingInfo => ({ level, title, line });

/** A file of 41 lines: two top headings, and two under the first. */
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

test("the page's map shares each section's measured height among its blocks by their estimated heights", () => {
  const sections = splitSections(TextView.parse("Intro.\n\n# One\n\nA paragraph.\n\n```\na\nb\n```\n", MARKDOWN));
  // Section 0 (the intro) from 0 to 100, section 1 from 100 to the page's end, 400.
  const map = pageMap(sections, [0, 100], 400);
  expect(map.extent).toBe(400);
  expect(map.labels).toEqual([{ y: 100, text: "One" }]);
  const one = sections[1].blocks.map(blockWeight);
  const total = one.reduce((sum, weight) => sum + weight, 0);
  // The code block, last, starts where the heading's and the paragraph's shares end.
  const band = map.bars.find((bar) => bar.ink === "band");
  expect(Math.abs((band?.y ?? 0) - (100 + (300 * (one[0] + one[1])) / total)) < 0.001).toBe(true);
  // A section not measured yet draws nothing.
  expect(pageMap(sections, [0], 400).labels).toEqual([]);
});

test("the line under a place on the page is as far into its section's lines as the place is into the section", () => {
  const headings = [heading(1, "One", 10), heading(1, "Two", 20)];
  const tops = [0, 100, 300];
  expect(lineAtPage(200, headings, 30, tops, 400)).toBe(15);
  expect(lineAtPage(350, headings, 30, tops, 400)).toBe(25);
  expect(lineAtPage(50, headings, 30, tops, 400)).toBe(5);
  expect(lineAtPage(1000, headings, 30, tops, 400)).toBe(29);
});

test("the preview shows the block of lines around a line, under its heading", () => {
  const lines = ["# One", "", "a", "b", "c", "", "d"];
  expect(blockAround(lines, 3, [heading(1, "One", 0)])).toEqual({ first: 2, lines: ["a", "b", "c"], heading: 0 });
  // A long block shows the lines around the line, at most as many as asked.
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
