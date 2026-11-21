import { expect, test } from "nsh/test";
import type { HeadingInfo } from "mdviewer/native";
import { foldRanges, headingAt, linkTarget, readingMinutes } from "./views/document";

const TEXT = "Intro.\n\n# One\n\nText.\n\n## Two\n\nMore.\n\n# Three\n\nLast[^1].\n\n[^1]: A note.\n";

const heading = (level: number, title: string, line: number): HeadingInfo => ({ level, title, line });

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

test("reading takes a minute for every two hundred words, and a minute at least", () => {
  expect(readingMinutes(0)).toBe(0);
  expect(readingMinutes(12)).toBe(1);
  expect(readingMinutes(1000)).toBe(5);
});
