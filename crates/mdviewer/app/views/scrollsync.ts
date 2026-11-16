// Split's two sides scrolling together, while the header's toggle says so
// (`scrollTogether` in `model.ts`).
//
// The sides can't share a share of their length: a code block, a table or a
// picture is taller on one side than on the other. They share places instead:
// the top, each top-level heading, and the end. A heading's place in Raw is
// where its line is in the editor (`rangeBounds`, its scrolling taken away);
// on the page it is where its section starts (the rendered view's `tops`).
// Between two places, a side follows the other in proportion (`follow`), so the
// two meet at every heading and drift apart at most within a section.
//
// Whichever side is scrolled leads, and the other follows it, ignoring its own
// scrolling for a moment (`FOLLOWING`), so the two never chase each other. The
// page says when it scrolls; nsh 0.5.1's editor doesn't, so Raw's place is read
// every `RAW_CHECK_EVERY` while it can be scrolled: while the pointer is over it
// or it has the keyboard. The places are measured again when a scroll starts
// after a pause (a fold in Raw moves them), and when either side changes size.
import type { ScrollRef } from "nsh";
import type { EditorRef } from "nsh/base";

/** How long a side ignores its own scrolling after following the other's, in milliseconds: its scroll events come a frame or two late. */
const FOLLOWING = 200;

/**
 * How often Raw's place is read while it can be scrolled, in milliseconds. A
 * workaround: nsh 0.5.1's `Editor` reports no scrolling; an `onScroll` of its
 * own in a later release replaces this.
 */
export const RAW_CHECK_EVERY = 33;

/** How long scrolling may pause before the next scroll measures the places again, in milliseconds. */
const PAUSE = 400;

/** The places both sides share, in order on each: scroll offsets, the top first and the end last. */
export interface Anchors {
  raw: number[];
  page: number[];
}

/**
 * The places both sides share: the top, each heading found on both (`raw` and
 * `page`, by heading, `undefined` where a side has none, as a heading folded
 * away in Raw), and the ends (`rawEnd`, `pageEnd`, how far each scrolls). A
 * heading out of order with the one before it on either side, or past where
 * either side scrolls to, is left out: it can't be a place both reach.
 */
export function anchorsOf(raw: (number | undefined)[], page: (number | undefined)[], rawEnd: number, pageEnd: number): Anchors {
  const anchors: Anchors = { raw: [0], page: [0] };
  raw.forEach((rawAt, index) => {
    const pageAt = page[index];
    if (rawAt === undefined || pageAt === undefined) return;
    const last = anchors.raw.length - 1;
    if (rawAt > anchors.raw[last] && pageAt > anchors.page[last] && rawAt < rawEnd && pageAt < pageEnd) {
      anchors.raw.push(rawAt);
      anchors.page.push(pageAt);
    }
  });
  const last = anchors.raw.length - 1;
  anchors.raw.push(Math.max(rawEnd, anchors.raw[last]));
  anchors.page.push(Math.max(pageEnd, anchors.page[last]));
  return anchors;
}

/** Where scroll offset `at` on one side falls on the other: `from` and `to` are the same places on each, in order; between two, in proportion. */
export function follow(at: number, from: number[], to: number[]): number {
  if (from.length < 2) return to[0] ?? 0;
  let next = 1;
  while (next < from.length - 1 && at > from[next]) next++;
  const [start, end] = [from[next - 1], from[next]];
  const share = end > start ? Math.min(1, Math.max(0, (at - start) / (end - start))) : 0;
  return to[next - 1] + (to[next] - to[next - 1]) * share;
}

/** What the rendered view tells a sync: where its headings' sections start, and how far it scrolls. */
export interface PageSide {
  scroll: ScrollRef;
  /** Where each of the file's top-level headings starts on the page, as a scroll offset, by heading. */
  headingTops: () => (number | undefined)[];
}

/** What the raw view tells a sync: its editor, the headings' lines and the file's last line. */
export interface RawSide {
  editor: EditorRef;
  /** The line each top-level heading is on, from 0. */
  headingLines: () => number[];
  /** The file's last line, from 0. */
  lastLine: () => number;
}

/** The two sides of Split, kept together while `on` says so. */
export interface ScrollSync {
  /** The raw view, once its editor is there. */
  raw(side: RawSide | undefined): void;
  /** The page, once its scroll view is there. */
  page(side: PageSide | undefined): void;
  /** Whether Raw can be scrolled now: the pointer over it, or the keyboard in it. While it can, its place is read. */
  rawActive(active: boolean): void;
  /** The raw view's height, as it lays out. */
  rawHeight(height: number): void;
  /** The page scrolled to `offset`, its content `content` tall in a view `height` tall. */
  pageScrolled(offset: number, content: number, height: number): void;
  /** Something changed that moves the places: the file, a side's width. */
  remeasure(): void;
  /** Leave both sides alone for `ms`: the outline is taking them both somewhere. */
  pause(ms: number): void;
  /** Bring the page to where Raw is: the toggle was turned on. */
  align(): void;
  /** Stop reading Raw's place. */
  stop(): void;
}

/** A sync of Split's two sides, while `on()`. */
export function createScrollSync(on: () => boolean): ScrollSync {
  let raw: RawSide | undefined;
  let page: PageSide | undefined;
  let rawViewport = 0;
  let pageEnd = 0;
  let anchors: Anchors | null = null;
  let measuring: Promise<Anchors | null> | null = null;
  let rawAt = 0;
  let rawQuietUntil = 0;
  let pageQuietUntil = 0;
  let pausedUntil = 0;
  let lastSync = 0;
  let timer: ReturnType<typeof setInterval> | undefined;

  /** The places both sides share now, measured in Raw's editor. */
  const measure = async (): Promise<Anchors | null> => {
    const side = raw;
    if (!side || !page) return null;
    const scrolled = (await side.editor.scrollOffset()).y;
    const lineTop = async (line: number) => {
      const box = (await side.editor.rangeBounds({ start: { line, column: 0 }, end: { line, column: 0 } }))[0]?.bounds;
      return box ? { top: box.origin.y + scrolled, bottom: box.origin.y + box.size.height + scrolled } : undefined;
    };
    const tops = await Promise.all(side.headingLines().map(async (line) => (await lineTop(line))?.top));
    const last = await lineTop(side.lastLine());
    const rawEnd = Math.max(0, (last?.bottom ?? 0) - rawViewport);
    return anchorsOf(tops, page.headingTops(), rawEnd, pageEnd);
  };

  /** The places, measured again when scrolling starts after a pause. */
  const current = async (): Promise<Anchors | null> => {
    const now = Date.now();
    if (now - lastSync > PAUSE) anchors = null;
    lastSync = now;
    if (anchors) return anchors;
    measuring ??= measure().finally(() => {
      measuring = null;
    });
    anchors = await measuring;
    return anchors;
  };

  /** Raw's place, read: when it moved by itself, the page follows. */
  const checkRaw = async (force = false) => {
    if (!on() || !raw || !page) return;
    const at = (await raw.editor.scrollOffset()).y;
    if (!force && Math.abs(at - rawAt) < 0.5) return;
    rawAt = at;
    const now = Date.now();
    if (!force && (now < rawQuietUntil || now < pausedUntil)) return;
    const places = await current();
    if (!places || !page) return;
    pageQuietUntil = Date.now() + FOLLOWING;
    page.scroll.scrollTo({ y: follow(at, places.raw, places.page), animated: false });
  };

  const stop = () => {
    if (timer !== undefined) clearInterval(timer);
    timer = undefined;
  };

  return {
    raw(side) {
      raw = side;
      anchors = null;
    },
    page(side) {
      page = side;
      anchors = null;
    },
    rawActive(active) {
      if (active && timer === undefined) timer = setInterval(() => void checkRaw(), RAW_CHECK_EVERY);
      if (!active) stop();
    },
    rawHeight(height) {
      rawViewport = height;
      anchors = null;
    },
    pageScrolled(offset, content, height) {
      pageEnd = Math.max(0, content - height);
      const now = Date.now();
      if (!on() || now < pageQuietUntil || now < pausedUntil) return;
      void current().then((places) => {
        if (!places || !raw) return;
        const target = follow(offset, places.page, places.raw);
        rawQuietUntil = Date.now() + FOLLOWING;
        rawAt = target;
        raw.editor.setScrollOffset({ x: 0, y: target });
      });
    },
    remeasure() {
      anchors = null;
    },
    pause(ms) {
      pausedUntil = Date.now() + ms;
    },
    align() {
      anchors = null;
      void checkRaw(true);
    },
    stop,
  };
}
