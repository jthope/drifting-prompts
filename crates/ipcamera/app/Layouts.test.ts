import { expect, test } from "nsh/test";
import type { CellInfo, ViewInfo } from "ipcamera/native";
import type { MenuEntries } from "../../dp/ui";
import { cameraPageMenu, tileMenu } from "./views/actions";
import {
  addTile,
  cancelEditing,
  draft,
  draftCells,
  fitOf,
  fits,
  matchingLayout,
  moveTile,
  removeTile,
  resizeTile,
  roomFor,
  showingOf,
  startEditing,
  startFrom,
  streamsOf,
} from "./views/arrange";
import { grid, layouts, pageFitOf, setPageFit } from "./views/model";

const cell = (column: number, row: number, columnSpan: number, rowSpan: number): CellInfo => ({ column, row, columnSpan, rowSpan });
const layout = (key: string) => {
  const found = layouts.find((each) => each.key === key);
  if (!found) throw new Error(`no layout ${key}`);
  return found;
};

function viewAs(key: string): ViewInfo {
  const cells = layout(key).cells;
  return {
    id: "v",
    name: "Yard",
    layout: key,
    cells,
    fit: "original",
    tiles: cells.map((_, index) => (index === 0 ? "cam" : "")),
    streams: cells.map(() => ""),
    fits: cells.map(() => "original"),
    hiddenTiles: [],
    hiddenStreams: [],
    hiddenFits: [],
  };
}

const keyOf = (index: number) => draft()?.tiles[index].key ?? -1;

test("the grid is 12 by 12, and every layout fills it", () => {
  expect(grid.size).toBe(12);
  expect(grid.smallest).toBe(2);
  for (const each of layouts) {
    expect(each.cells.reduce((sum, one) => sum + one.columnSpan * one.rowSpan, 0)).toBe(144);
    expect(matchingLayout(each.cells)?.key).toBe(each.key);
  }
  expect(layout("4").cells[3]).toEqual(cell(6, 6, 6, 6));
});

test("a tile moves by whole cells, stops at the grid's edges, and never onto another", () => {
  startEditing(viewAs("4"));
  expect(moveTile(keyOf(0), 1, 0)).toBe(false);

  removeTile(keyOf(1));
  expect(draftCells().length).toBe(3);
  expect(moveTile(keyOf(0), 3, 0)).toBe(true);
  expect(draftCells()[0]).toEqual(cell(3, 0, 6, 6));
  expect(moveTile(keyOf(0), 99, -5)).toBe(true);
  expect(draftCells()[0]).toEqual(cell(6, 0, 6, 6));
  expect(moveTile(keyOf(0), 0, 3)).toBe(false);
  expect(draftCells()[0]).toEqual(cell(6, 0, 6, 6));
  cancelEditing();
  expect(draft()).toBe(null);
});

test("a tile resizes by its bottom right, no smaller than 2 × 2 and no further than the grid", () => {
  startEditing(viewAs("4"));
  removeTile(keyOf(1));
  const first = keyOf(0);
  expect(resizeTile(first, { right: -10, bottom: -10 })).toBe(true);
  expect(draftCells()[0]).toEqual(cell(0, 0, 2, 2));
  expect(resizeTile(first, { right: 99 })).toBe(true);
  expect(draftCells()[0]).toEqual(cell(0, 0, 12, 2));
  expect(resizeTile(first, { bottom: 6 })).toBe(false);
  expect(matchingLayout(draftCells())).toBe(undefined);
  cancelEditing();
});

test("a tile resizes by any edge or corner, the edges across from it staying put", () => {
  startEditing(viewAs("4"));
  removeTile(keyOf(1));
  const last = keyOf(2);
  expect(draftCells()[2]).toEqual(cell(6, 6, 6, 6));
  expect(resizeTile(last, { left: 10 })).toBe(true);
  expect(draftCells()[2]).toEqual(cell(10, 6, 2, 6));
  expect(resizeTile(last, { top: -99 })).toBe(true);
  expect(draftCells()[2]).toEqual(cell(10, 0, 2, 12));
  expect(resizeTile(last, { left: -4, top: 2 })).toBe(true);
  expect(draftCells()[2]).toEqual(cell(6, 2, 6, 10));
  expect(resizeTile(last, { left: -1 })).toBe(false);
  expect(draftCells()[2]).toEqual(cell(6, 2, 6, 10));
  cancelEditing();
});

test("a new tile goes in the first room from the top left, largest first, until there is none", () => {
  expect(roomFor([])).toEqual(cell(0, 0, 4, 4));
  startEditing(viewAs("4"));
  expect(addTile()).toBe(false);

  resizeTile(keyOf(0), { right: -2, bottom: -2 });
  expect(addTile()).toBe(true);
  expect(draftCells()[4]).toEqual(cell(4, 0, 2, 2));
  expect(draft()?.tiles[4].was).toBe(null);
  cancelEditing();
});

test("the last tile stays, and a layout lays the draft out tile by tile with their cameras", () => {
  startEditing(viewAs("1"));
  removeTile(keyOf(0));
  expect(draftCells().length).toBe(1);

  startFrom(layout("1+5"));
  expect(draftCells()).toEqual(layout("1+5").cells);
  expect(draft()?.tiles.map((tile) => tile.was)).toEqual([0, 1, 2, 3, 4, 5]);
  expect(fits(draft()?.tiles ?? [], cell(0, 0, 2, 2))).toBe(false);
  cancelEditing();
});

test("a frame shows the camera of the tile it was, kept past the cells too, and its streams each once", () => {
  const view = { ...viewAs("4"), tiles: ["cam", "", "cam", ""], streams: ["", "", "sub", ""], hiddenTiles: ["", "porch", "", "", "cam"], hiddenStreams: ["", "", "", "", "sub"] };
  expect(showingOf(view, 0)).toEqual({ camera: "cam", stream: "" });
  expect(showingOf(view, 1)).toBe(undefined);
  expect(showingOf(view, 5)).toEqual({ camera: "porch", stream: "" });
  expect(showingOf(view, 9)).toBe(undefined);
  expect(showingOf(view, null)).toBe(undefined);

  startEditing(view);
  expect(streamsOf(view, draft()?.tiles ?? [])).toEqual([
    { camera: "cam", stream: "" },
    { camera: "cam", stream: "sub" },
  ]);
  startFrom(layout("9"));
  expect(streamsOf(view, draft()?.tiles ?? [])).toEqual([
    { camera: "cam", stream: "" },
    { camera: "cam", stream: "sub" },
    { camera: "porch", stream: "" },
  ]);
  removeTile(keyOf(0));
  expect(streamsOf(view, draft()?.tiles ?? [])[0]).toEqual({ camera: "cam", stream: "sub" });
  cancelEditing();
});

test("a frame's picture keeps the fit of the tile it was, and a new one takes the view's", () => {
  const view = { ...viewAs("4"), fit: "stretch", fits: ["fill", "stretch", "16:9", "stretch"], hiddenTiles: ["cam"], hiddenStreams: [""], hiddenFits: ["1:1"] };
  expect(fitOf(view, 0)).toBe("fill");
  expect(fitOf(view, 2)).toBe("16:9");
  expect(fitOf(view, 4)).toBe("1:1");
  expect(fitOf(view, null)).toBe("stretch");
});

function menuText(entries: MenuEntries): string[] {
  return entries.flatMap((entry) => {
    if (!entry) return [];
    if ("type" in entry && entry.type === "separator") return ["-"];
    const label = "label" in entry ? `${entry.label}${"checked" in entry && entry.checked ? " ✓" : ""}` : "?";
    const submenu = "submenu" in entry && entry.submenu ? menuText(entry.submenu).map((line) => `  ${line}`) : [];
    return [label, ...submenu];
  });
}

test("a tile's menu picks its fit, those that force a shape apart, and a camera on its own has its own until the app closes", () => {
  const view = { ...viewAs("4"), fits: ["fill", "original", "original", "original"] };
  const menu = menuText(tileMenu(view, 0));
  const fit = menu.indexOf("Fit");
  expect(menu.slice(fit, fit + 10)).toEqual(["Fit", "  Original", "  Fill ✓", "  Stretch", "  -", "  1:1", "  4:3", "  3:4", "  16:9", "  9:16"]);
  expect(menu.indexOf("Stream") < fit).toBe(true);
  expect(menu[fit + 10]).toBe("Edit camera…");

  expect(pageFitOf("cam")).toBe("original");
  setPageFit("cam", "1:1");
  expect(pageFitOf("cam")).toBe("1:1");
  expect(menuText(cameraPageMenu("cam", "")).filter((line) => line.endsWith("✓"))).toContain("  1:1 ✓");
  setPageFit("cam", "original");
});
