// What the document views work out from a file (`views/document.ts`): the
// rendered view's sections, the raw view's folds, the heading a line is under,
// where a link goes and where a relative picture is.
import { expect, test } from "nsh/test";
import { TextView } from "nsh/base";
import type { HeadingInfo } from "mdviewer/native";
import { foldRanges, headingAnchors, headingAt, linkTarget, MARKDOWN, pictureSource, readingMinutes, splitSections, withPictures } from "./views/document";

const TEXT = "Intro.\n\n# One\n\nText.\n\n## Two\n\nMore.\n\n# Three\n\nLast[^1].\n\n[^1]: A note.\n";

const heading = (level: number, title: string, line: number): HeadingInfo => ({ level, title, line });

test("the page is cut at its top-level headings, the footnotes with the last section", () => {
  const sections = splitSections(TextView.parse(TEXT, MARKDOWN));
  expect(sections.length).toBe(4);
  expect(sections[0].blocks.map((block) => block.type)).toEqual(["paragraph"]);
  expect(sections[1].blocks[0].type).toBe("heading");
  expect(sections[3].footnotes?.length).toBe(1);
  expect(sections[1].footnotes).toBe(undefined);
});

test("a page with no text before its first heading starts with an empty section", () => {
  const sections = splitSections(TextView.parse("# Only\n\nText.\n", MARKDOWN));
  expect(sections.length).toBe(2);
  expect(sections[0].blocks.length).toBe(0);
});

test("front matter is not a heading of the page", () => {
  const document = TextView.parse("---\ntitle: A post\n---\n\n# A post\n", MARKDOWN);
  expect(splitSections(document).length).toBe(2);
  expect(headingAnchors(document).length).toBe(1);
});

test("a heading folds to the line before the next of its level, blank lines left out", () => {
  const lines = TEXT.split("\n");
  const headings = [heading(1, "One", 2), heading(2, "Two", 6), heading(1, "Three", 10)];
  expect(foldRanges(headings, lines)).toEqual([
    { startLine: 2, endLine: 8 },
    { startLine: 6, endLine: 8 },
    { startLine: 10, endLine: 14 },
  ]);
});

test("a heading with nothing under it doesn't fold", () => {
  expect(foldRanges([heading(1, "One", 0), heading(1, "Two", 2)], ["# One", "", "# Two"])).toEqual([]);
});

test("a line is under the last heading above it", () => {
  const headings = [heading(1, "One", 2), heading(2, "Two", 6)];
  expect(headingAt(headings, 0)).toBe(-1);
  expect(headingAt(headings, 2)).toBe(0);
  expect(headingAt(headings, 5)).toBe(0);
  expect(headingAt(headings, 40)).toBe(1);
});

test("a link goes to a heading, another file of the project, or outside the app", () => {
  expect(linkTarget("README.md", "#usage")).toEqual({ kind: "anchor", anchor: "usage" });
  expect(linkTarget("docs/setup.md", "guide.md#first-steps")).toEqual({ kind: "file", path: "docs/guide.md", anchor: "first-steps" });
  expect(linkTarget("docs/setup.md", "../README.md")).toEqual({ kind: "file", path: "README.md", anchor: "" });
  expect(linkTarget("README.md", "My%20Notes.md")).toEqual({ kind: "file", path: "My Notes.md", anchor: "" });
  expect(linkTarget("README.md", "https://example.com/")).toEqual({ kind: "outside", url: "https://example.com/" });
  expect(linkTarget("README.md", "mailto:someone@example.com")).toEqual({ kind: "outside", url: "mailto:someone@example.com" });
  expect(linkTarget("README.md", "../outside.md")).toEqual({ kind: "none" });
});

test("a relative picture is found beside its file, in the project", () => {
  expect(pictureSource("D:\\Notes", "docs/setup.md", "images/shot.png")).toBe("D:\\Notes\\docs\\images\\shot.png");
  expect(pictureSource("D:\\Notes\\", "docs/setup.md", "../logo.svg")).toBe("D:\\Notes\\logo.svg");
  expect(pictureSource("/home/me/notes", "README.md", "a%20b.png")).toBe("/home/me/notes/a b.png");
  expect(pictureSource("D:\\Notes", "README.md", "https://example.com/badge.svg")).toBe("https://example.com/badge.svg");
  expect(pictureSource("D:\\Notes", "README.md", "../../secret.png")).toBe("../../secret.png");
});

test("pictures are found wherever the page has them", () => {
  const document = TextView.parse("![a](a.png)\n\n- ![b](b.png)\n\n> [![c](c.png)](https://example.com)\n", MARKDOWN);
  const found: string[] = [];
  withPictures(document, (src) => {
    found.push(src);
    return src;
  });
  expect(found.sort()).toEqual(["a.png", "b.png", "c.png"]);
});

test("reading takes a minute for every two hundred words, and a minute at least", () => {
  expect(readingMinutes(0)).toBe(0);
  expect(readingMinutes(12)).toBe(1);
  expect(readingMinutes(1000)).toBe(5);
});
