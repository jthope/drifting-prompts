// How Split's two sides line up (`views/scrollsync.ts`): the places they share,
// and where a scroll on one side falls on the other.
import { expect, test } from "nsh/test";
import { anchorsOf, follow } from "./views/scrollsync";

test("the sides share the top, each heading both have, and the ends", () => {
  expect(anchorsOf([100, 300], [150, 500], 800, 1200)).toEqual({ raw: [0, 100, 300, 800], page: [0, 150, 500, 1200] });
});

test("a heading only one side has, out of order, or past where a side scrolls to is no shared place", () => {
  // Folded away in Raw, then out of order on the page, then past the page's end.
  expect(anchorsOf([undefined, 200, 300, 700], [100, 400, 350, 1300], 800, 1200)).toEqual({ raw: [0, 200, 800], page: [0, 400, 1200] });
});

test("a scroll on one side falls in proportion between the places around it on the other", () => {
  const raw = [0, 100, 300, 800];
  const page = [0, 150, 500, 1200];
  expect(follow(0, raw, page)).toBe(0);
  expect(follow(100, raw, page)).toBe(150);
  expect(follow(200, raw, page)).toBe(325);
  expect(follow(800, raw, page)).toBe(1200);
  // And back, the same places the other way.
  expect(follow(325, page, raw)).toBe(200);
  // Past either end, it stays at the end.
  expect(follow(900, raw, page)).toBe(1200);
  expect(follow(-10, raw, page)).toBe(0);
});
