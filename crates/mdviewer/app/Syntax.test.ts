// How the raw view colors a file's marks (`views/syntax.ts`): each mark a
// decoration over its range, in the theme's syntax colors, and a code block a
// band behind its lines.
import { expect, test } from "nsh/test";
import { decorationsOf, PAGE } from "./views/syntax";

test("a file's marks become the raw view's decorations, in the theme's syntax colors", () => {
  const [heading, band] = decorationsOf([
    { kind: "heading", startLine: 0, startColumn: 2, endLine: 0, endColumn: 7 },
    { kind: "codeBlock", startLine: 2, startColumn: 0, endLine: 4, endColumn: 3 },
  ]);
  expect(heading.range).toEqual({ start: { line: 0, column: 2 }, end: { line: 0, column: 7 } });
  expect(heading.class).toContain("text-syntax-heading");
  expect(heading.lineClass).toBe(null);
  // A code block's band is behind its whole lines, its text left to nsh's own colors.
  expect(band.class).toBe(null);
  expect(band.lineClass).toContain("bg-");
});

test("a link on the page is in the theme's link color", () => {
  expect(PAGE.link).toContain("text-syntax-link");
});
