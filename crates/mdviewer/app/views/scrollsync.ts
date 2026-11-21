import type { EditorRef } from "nsh/base";
import type { SyncPlace } from "./pagemessages";

// A side ignores its own scrolling this long after following, since its scroll events come a frame or two late.
const FOLLOWING = 200;

/** How often Raw's place is read while it can be scrolled, in milliseconds: nsh's `Editor` reports no scrolling. */
export const RAW_CHECK_EVERY = 33;

// A scroll after a pause this long measures the places again: a fold in Raw moves them.
const PAUSE = 400;

/** The scroll offsets both sides share, in order: the top first and the end last. */
export interface Anchors {
  raw: number[];
  page: number[];
}

/** The places both sides share: the top, each place both reach in order, and the ends. */
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

/** Where scroll offset `at` on one side falls on the other, in proportion between shared places. */
export function follow(at: number, from: number[], to: number[]): number {
  if (from.length < 2) return to[0] ?? 0;
  let next = 1;
  while (next < from.length - 1 && at > from[next]) next++;
  const [start, end] = [from[next - 1], from[next]];
  const share = end > start ? Math.min(1, Math.max(0, (at - start) / (end - start))) : 0;
  return to[next - 1] + (to[next] - to[next - 1]) * share;
}

/** What the rendered view tells a sync. */
export interface PageSide {
  /** Scrolls the page to `y`, at once. */
  scrollTo: (y: number) => void;
  /** The lines to line up at, in order, each with the page's scroll offset that shows it at the top. */
  places: () => SyncPlace[];
}

/** What the raw view tells a sync. */
export interface RawSide {
  editor: EditorRef;
  /** The file's last line, from 0. */
  lastLine: () => number;
}

/** The two sides of Split, kept together while `on` says so. */
export interface ScrollSync {
  /** The raw view, once its editor is there. */
  raw(side: RawSide | undefined): void;
  /** The page, once it is there. */
  page(side: PageSide | undefined): void;
  /** Whether Raw can be scrolled now; while it can, its place is read. */
  rawActive(active: boolean): void;
  /** The raw view's height. */
  rawHeight(height: number): void;
  /** The page scrolled to `offset`, its content `content` tall in a view `height` tall. */
  pageScrolled(offset: number, content: number, height: number): void;
  /** Something changed that moves the places. */
  remeasure(): void;
  /** Leave both sides alone for `ms`. */
  pause(ms: number): void;
  /** Bring the page to where Raw is. */
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

  const measure = async (): Promise<Anchors | null> => {
    const side = raw;
    if (!side || !page) return null;
    const scrolled = (await side.editor.scrollOffset()).y;
    const lineTop = async (line: number) => {
      const box = (await side.editor.rangeBounds({ start: { line, column: 0 }, end: { line, column: 0 } }))[0]?.bounds;
      return box ? { top: box.origin.y + scrolled, bottom: box.origin.y + box.size.height + scrolled } : undefined;
    };
    const places = page.places();
    const tops = await Promise.all(places.map(async (place) => (await lineTop(place.line))?.top));
    const last = await lineTop(side.lastLine());
    const rawEnd = Math.max(0, (last?.bottom ?? 0) - rawViewport);
    return anchorsOf(
      tops,
      places.map((place) => place.top),
      rawEnd,
      pageEnd,
    );
  };

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
    page.scrollTo(follow(at, places.raw, places.page));
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
