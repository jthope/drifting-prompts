import type { HeadingInfo, MatchInfo } from "mdviewer/native";

/** A block of the page: the lines it comes from (from 0) and where it is, in CSS pixels from the page's top. */
export type PageBlock = { line: number; endLine: number; top: number; height: number; kind: string; depth: number };

/** A key the page passes on: `code` names the key whatever the layout, `key` what it types. */
export type PageKey = { key: string; code: string; control: boolean; shift: boolean; alt: boolean };

/** What the page's search highlights: the query as a JavaScript pattern, the line each of the app's matches starts on,
 * the one the reader is on (-1 for none), and a count that goes up whenever the page should scroll to it. */
export type PageFind = { source: string; flags: string; wholeWord: boolean; lines: number[]; current: number; reveal: number; above: number };

/** What the page tells the app. */
export type FromPage =
  | { type: "ready" }
  | { type: "layout"; height: number; blocks: PageBlock[] }
  | { type: "scroll"; y: number; viewport: number; content: number }
  | { type: "link"; url: string; control: boolean }
  | ({ type: "key" } & PageKey)
  | { type: "menu"; x: number; y: number; link: string; selection: string }
  | { type: "copy"; text: string }
  | { type: "wantPicture"; id: string; src: string }
  | { type: "found"; count: number; places: number[] };

/** What the app tells the page. */
export type ToPage =
  | { type: "show"; html: string; top: number | null }
  | { type: "theme"; vars: Record<string, string>; dark: boolean }
  | { type: "scrollTo"; y: number; animated: boolean }
  | { type: "goToLine"; line: number; above: number; animated: boolean }
  | ({ type: "find" } & PageFind)
  | { type: "picture"; id: string; data?: string; problem?: string }
  | { type: "selectAll" };

type Fields = Record<string, unknown>;

const num = (value: unknown) => (typeof value === "number" && Number.isFinite(value) ? value : 0);
const str = (value: unknown) => (typeof value === "string" ? value : "");
const bool = (value: unknown) => value === true;

function block(value: unknown): PageBlock | null {
  if (typeof value !== "object" || value === null) return null;
  const each = value as Fields;
  return { line: num(each.line), endLine: num(each.endLine), top: num(each.top), height: num(each.height), kind: str(each.kind), depth: num(each.depth) };
}

/** A message from the page, checked; `null` when it isn't one. Anything on the page could have sent it. */
export function fromPage(data: string): FromPage | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(data);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const message = parsed as Fields;
  switch (message.type) {
    case "ready":
      return { type: "ready" };
    case "layout":
      return {
        type: "layout",
        height: num(message.height),
        blocks: Array.isArray(message.blocks) ? message.blocks.flatMap((each) => block(each) ?? []) : [],
      };
    case "scroll":
      return { type: "scroll", y: num(message.y), viewport: num(message.viewport), content: num(message.content) };
    case "link":
      return { type: "link", url: str(message.url), control: bool(message.control) };
    case "key":
      return { type: "key", key: str(message.key), code: str(message.code), control: bool(message.control), shift: bool(message.shift), alt: bool(message.alt) };
    case "menu":
      return { type: "menu", x: num(message.x), y: num(message.y), link: str(message.link), selection: str(message.selection) };
    case "copy":
      return { type: "copy", text: str(message.text) };
    case "wantPicture":
      return { type: "wantPicture", id: str(message.id), src: str(message.src) };
    case "found": {
      const places = Array.isArray(message.places) ? message.places.filter((each): each is number => typeof each === "number" && Number.isFinite(each)) : [];
      return { type: "found", count: num(message.count), places };
    }
    default:
      return null;
  }
}

/** Where each of `headings` starts on the page: the top-level heading block on its line; `undefined` while it isn't laid out. */
export function headingTops(headings: HeadingInfo[], blocks: PageBlock[]): (number | undefined)[] {
  const tops = new Map<number, number>();
  for (const each of blocks) {
    if (each.depth === 0 && /^h[1-6]$/.test(each.kind) && !tops.has(each.line)) tops.set(each.line, each.top);
  }
  return headings.map((heading) => tops.get(heading.line));
}

/** The innermost of `blocks` that `holds`, the later on a tie, since the page lists a block before what's inside it. */
function innermost(blocks: PageBlock[], holds: (block: PageBlock) => boolean): PageBlock | undefined {
  let found: PageBlock | undefined;
  for (const each of blocks) {
    if (holds(each) && (!found || each.depth >= found.depth)) found = each;
  }
  return found;
}

/** About which line (from 0) is at `y` on the page: as far into the innermost block there as `y` is into the block; the last block above it between blocks. */
export function lineAtPlace(blocks: PageBlock[], y: number): number {
  const block = innermost(blocks, (each) => each.top <= y && y < each.top + each.height);
  if (!block) {
    const above = blocks.filter((each) => each.top <= y);
    return above.length === 0 ? 0 : Math.max(...above.map((each) => each.endLine));
  }
  const lines = block.endLine - block.line + 1;
  const share = block.height > 0 ? (y - block.top) / block.height : 0;
  return Math.min(block.endLine, block.line + Math.floor(share * lines));
}

/** The heading the reader is under at scroll offset `y`: the last whose top is above `y + slack`; -1 above the first. */
export function readingAt(tops: (number | undefined)[], y: number, slack: number): number {
  let found = -1;
  tops.forEach((top, index) => {
    if (top !== undefined && top <= y + slack) found = index;
  });
  return found;
}

const MODIFIER_ORDER = ["ctrl", "alt", "shift"];

const MODIFIER_NAMES: Record<string, string> = { mod: "ctrl", cmdorctrl: "ctrl", control: "ctrl", ctrl: "ctrl", alt: "alt", option: "alt", opt: "alt", shift: "shift" };

const KEY_NAMES: Record<string, string> = { "=": "plus", "-": "minus", esc: "escape", return: "enter" };

const CODE_KEYS: Record<string, string> = {
  Equal: "plus",
  Minus: "minus",
  Space: "space",
  Enter: "enter",
  NumpadEnter: "enter",
  Escape: "escape",
  Tab: "tab",
  Backspace: "backspace",
  Delete: "delete",
  Insert: "insert",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  Home: "home",
  End: "end",
  PageUp: "pageup",
  PageDown: "pagedown",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  BracketLeft: "[",
  BracketRight: "]",
  Backquote: "`",
};

/** A keystroke as `nsh/keymap` writes it, in one spelling: lowercase, `mod` as `ctrl`, the modifiers in one order. */
export function normalized(keystroke: string): string {
  const parts = keystroke.toLowerCase().split("+");
  const key = parts.pop() ?? "";
  const modifiers = new Set(parts.map((part) => MODIFIER_NAMES[part] ?? part));
  return [...MODIFIER_ORDER.filter((each) => modifiers.has(each)), KEY_NAMES[key] ?? key].join("+");
}


/** The keystroke of a key the page passed on, as `normalized` spells it. */
export function keystrokeOf(key: PageKey): string {
  const code = key.code;
  const name = /^Key[A-Z]$/.test(code)
    ? code.slice(3).toLowerCase()
    : /^Digit\d$/.test(code)
      ? code.slice(5)
      : /^F\d{1,2}$/.test(code)
        ? code.toLowerCase()
        : (CODE_KEYS[code] ?? key.key.toLowerCase());
  return [key.control && "ctrl", key.alt && "alt", key.shift && "shift", name].filter(Boolean).join("+");
}

/** A line of the file (from 0) and where the page drew the block that starts on it. */
export type SyncPlace = { line: number; top: number };

/** Where Split lines its sides up: the first line of each block the page drew, one block a line, in order; every
 * heading, and the other blocks spread evenly when there are more than `most` in all. */
export function syncPlaces(blocks: PageBlock[], most: number): SyncPlace[] {
  const firsts = new Map<number, PageBlock>();
  for (const each of blocks) if (!firsts.has(each.line)) firsts.set(each.line, each);
  const all = [...firsts.values()];
  const headings = all.filter((each) => /^h[1-6]$/.test(each.kind));
  const rest = all.filter((each) => !/^h[1-6]$/.test(each.kind));
  const room = Math.max(0, most - headings.length);
  const kept = rest.length <= room ? rest : Array.from({ length: room }, (_, at) => rest[Math.floor((at * rest.length) / room)]);
  return [...headings, ...kept].sort((one, other) => one.line - other.line).map((each) => ({ line: each.line, top: each.top }));
}

/** What the page's search highlights, for `pattern` (`pagePattern`'s) and the app's `matches` in the file. */
export function pageFind(pattern: RegExp | null, wholeWord: boolean, matches: MatchInfo[], current: number, reveal: number, above: number): PageFind {
  return {
    source: pattern?.source ?? "",
    flags: pattern?.flags ?? "",
    wholeWord,
    lines: matches.map((each) => each.startLine),
    current,
    reveal,
    above,
  };
}
