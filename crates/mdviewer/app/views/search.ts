// What the title bar's search works out, kept apart from drawing so the tests
// can read it: where each scope looks (`placesFor`), the drop-down's rows and
// the files panel's tree of results, how many matches each file and each of
// its headings holds, which match is next as F3 steps across the files, the
// raw view's marks beside the lines that hold one, and the page with its
// matches highlighted. The marks, the counts and F3 go by the open project's
// matches alone (`inProject`); All projects lists the others' too, and going
// to one opens its project.
//
// The search itself is Rust's (`Search.onChange`, `mdviewer_core::search`): it
// finds what nsh's editor finds, so Raw highlights the same matches by itself
// (the editor's `search`) and the counts are the editor's. The page's text has
// no Markdown in it, so Rendered finds the query again in its own text, a piece
// at a time, with JavaScript's regular expressions (`pagePattern`): a match
// across formatting (half of it bold) isn't highlighted there, nor one in code,
// where a link goes or the front matter, which show no text to highlight. The
// match the reader is on is the page's too where it can be (`onThePage`): its
// section, and which of that section's highlights it is.
import type { CommandItem, EditorDecoration, TextViewDocument, TextViewInline, TreeItem } from "nsh/base";
import type { FileMatches, HeadingInfo, MarkInfo, MarkKind, MatchInfo, SearchPlace, SearchQuery, SearchResults } from "mdviewer/native";
import { folderOf, headingAt, mapLeaves } from "./document";

/** A match, by its file (`docs/setup.md`) and its place among that file's matches, from 0. */
export type MatchPlace = { file: string; index: number };

/** Where the search looks: the file in front, the files open in tabs, the open project, or every project. */
export type SearchScope = "file" | "tabs" | "project" | "all";

/** The scopes, as the drop-down offers them. */
export const SCOPES: { value: SearchScope; title: string }[] = [
  { value: "file", title: "This file" },
  { value: "tabs", title: "Open tabs" },
  { value: "project", title: "Project" },
  { value: "all", title: "All projects" },
];

/**
 * Where a search in `scope` looks: the file in front (`front`) or the files
 * in tabs (`tabs`) of the open project (`project`, its folder), all of its
 * files, or all of every project's (`projects`), the open one first. Nothing
 * while no project is open.
 */
export function placesFor(scope: SearchScope, project: string, front: string, tabs: string[], projects: string[]): SearchPlace[] {
  if (project === "") return [];
  switch (scope) {
    case "file":
      return front === "" ? [] : [{ project, everyFile: false, files: [front] }];
    case "tabs":
      return [{ project, everyFile: false, files: tabs }];
    case "project":
      return [{ project, everyFile: true, files: [] }];
    case "all":
      return [project, ...projects.filter((each) => each !== project)].map((each) => ({ project: each, everyFile: true, files: [] }));
  }
}

/** `results` with only the files of project `project` (its folder): what the marks, the counts and F3 go by. */
export function inProject(results: SearchResults | null, project: string): SearchResults | null {
  if (!results || results.files.every((file) => file.project === project)) return results;
  const files = results.files.filter((file) => file.project === project);
  return { ...results, files, total: files.reduce((sum, file) => sum + file.matches.length, 0) };
}

/** What a row of the results is: a project's, a file's or a match's, by its id. */
export type ResultRow = { kind: "project"; project: string } | { kind: "file"; project: string; file: string } | { kind: "match"; project: string; file: string; index: number };

/** A row's id in the results: its kind, project, file and match apart, by line breaks, which no path holds. */
export function rowId(row: ResultRow): string {
  switch (row.kind) {
    case "project":
      return `p\n${row.project}`;
    case "file":
      return `f\n${row.project}\n${row.file}`;
    case "match":
      return `m\n${row.project}\n${row.file}\n${row.index}`;
  }
}

/** The row of id `id` (`rowId`'s); `null` for any other id. */
export function rowOf(id: string): ResultRow | null {
  const [kind, project = "", file = "", index = ""] = id.split("\n");
  if (kind === "p") return { kind: "project", project };
  if (kind === "f") return { kind: "file", project, file };
  if (kind === "m" && index !== "") return { kind: "match", project, file, index: Number(index) };
  return null;
}

/** The most matches the drop-down lists: the files panel lists them all. */
export const MOST_LISTED = 500;

/**
 * The drop-down's rows: each match, its excerpt its label, under a section for
 * its file (the file's row id, which `renderGroup` turns back into the file),
 * at most `most` of them.
 */
export function resultItems(results: SearchResults | null, most = MOST_LISTED): CommandItem[] {
  const items: CommandItem[] = [];
  for (const file of results?.files ?? []) {
    const group = rowId({ kind: "file", project: file.project, file: file.path });
    for (const [index, each] of file.matches.entries()) {
      if (items.length >= most) return items;
      items.push({ id: rowId({ kind: "match", project: file.project, file: file.path, index }), label: each.excerpt, group });
    }
  }
  return items;
}

/**
 * The files panel's results as a tree: each file with matches, holding its
 * matches; under a row for each project when they come from several (All
 * projects), its name from `names` (by folder).
 */
export function resultTree(results: SearchResults | null, names: Map<string, string>): TreeItem[] {
  const files = results?.files ?? [];
  const fileItem = (file: FileMatches): TreeItem => ({
    id: rowId({ kind: "file", project: file.project, file: file.path }),
    label: file.path.split("/").pop() ?? file.path,
    children: file.matches.map((each, index) => ({ id: rowId({ kind: "match", project: file.project, file: file.path, index }), label: each.excerpt })),
  });
  const projects = [...new Set(files.map((file) => file.project))];
  if (projects.length <= 1) return files.map(fileItem);
  return projects.map((project) => ({
    id: rowId({ kind: "project", project }),
    label: names.get(project) ?? project,
    children: files.filter((file) => file.project === project).map(fileItem),
  }));
}

/** The match a results row's id names, with its file's folder in the project; `undefined` for another row. */
export function matchOf(results: SearchResults | null, id: string): { match: MatchInfo; folder: string } | undefined {
  const row = rowOf(id);
  if (row?.kind !== "match") return undefined;
  const file = results?.files.find((each) => each.project === row.project && each.path === row.file);
  const match = file?.matches[row.index];
  return match && file ? { match, folder: folderOf(file.path) } : undefined;
}

/** How many matches a results row holds: a project's, a file's; 0 for a match's. */
export function rowCount(results: SearchResults | null, id: string): number {
  const row = rowOf(id);
  const files = results?.files ?? [];
  if (row?.kind === "project") return files.filter((file) => file.project === row.project).reduce((sum, file) => sum + file.matches.length, 0);
  if (row?.kind === "file") return files.find((file) => file.project === row.project && file.path === row.file)?.matches.length ?? 0;
  return 0;
}

/** The parts of each line a file's `matches` cover, as UTF-16 ranges, by line: a match across lines covers the rest of its first line, its middle lines whole, and the start of its last. */
export function lineMarks(matches: MatchInfo[]): Map<number, [number, number][]> {
  const marks = new Map<number, [number, number][]>();
  for (const each of matches) {
    for (let line = each.startLine; line <= each.endLine; line++) {
      const range: [number, number] = [line === each.startLine ? each.startColumn : 0, line === each.endLine ? each.endColumn : Number.MAX_SAFE_INTEGER];
      if (range[1] <= range[0]) continue;
      const ranges = marks.get(line);
      if (ranges) ranges.push(range);
      else marks.set(line, [range]);
    }
  }
  return marks;
}

/** `text` cut at `ranges` (in order, apart): each part, and whether a match covers it. */
export function lineParts(text: string, ranges: [number, number][] = []): { text: string; found: boolean }[] {
  const parts: { text: string; found: boolean }[] = [];
  let at = 0;
  for (const [start, end] of ranges) {
    if (start >= text.length) break;
    if (start > at) parts.push({ text: text.slice(at, start), found: false });
    parts.push({ text: text.slice(Math.max(start, at), end), found: true });
    at = Math.min(text.length, end);
  }
  if (at < text.length || parts.length === 0) parts.push({ text: text.slice(at), found: false });
  return parts;
}

/** The match of a file's `matches` nearest line `line`: the first on it, else the closest above or below; -1 for none. */
export function nearestMatch(matches: MatchInfo[], line: number): number {
  let best = -1;
  matches.forEach((each, index) => {
    if (best < 0 || Math.abs(each.startLine - line) < Math.abs(matches[best].startLine - line)) best = index;
  });
  return best;
}

/** An excerpt cut at its match: what comes before, the match, and what comes after. */
export function excerptParts(match: MatchInfo): [string, string, string] {
  const text = match.excerpt;
  return [text.slice(0, match.excerptStart), text.slice(match.excerptStart, match.excerptEnd), text.slice(match.excerptEnd)];
}

/** The matches `results` found in file `path`. */
export function matchesIn(results: SearchResults | null, path: string): MatchInfo[] {
  return results?.files.find((file) => file.path === path)?.matches ?? [];
}

/** How many matches `results` found in file `path`. */
export function countIn(results: SearchResults | null, path: string): number {
  return matchesIn(results, path).length;
}

/** Which of all the matches `at` is, from 1, the files in their order; 0 when it isn't one of them. */
export function ordinal(results: SearchResults, at: MatchPlace): number {
  let before = 0;
  for (const file of results.files) {
    if (file.path === at.file) return at.index < file.matches.length ? before + at.index + 1 : 0;
    before += file.matches.length;
  }
  return 0;
}

/** What `results` found, said shortly: which match the reader is on (`at`) of how many, in how many files. */
export function searchSummary(results: SearchResults, at: MatchPlace | null): string {
  if (results.total === 0) return "No matches";
  const total = `${results.total.toLocaleString()}${results.truncated ? "+" : ""}`;
  const files = `${results.files.length.toLocaleString()} ${results.files.length === 1 ? "file" : "files"}`;
  const which = at ? ordinal(results, at) : 0;
  return which > 0 ? `Match ${which.toLocaleString()} of ${total} · ${files}` : `${total} ${results.total === 1 ? "match" : "matches"} · ${files}`;
}

/**
 * The match a step from `at` takes the reader to (`by` 1 the next, -1 the one
 * before): through each file's matches, then on to the next file that has
 * any, around the ends. With no match to step from (none yet, or `at` is gone),
 * the first of the file in front, `front` (its last, going back); when it has
 * none, the first of the next file in `order` (the project's files, as the
 * panel lists them) that has any, or the last of the one before it, going back.
 */
export function stepFrom(results: SearchResults, at: MatchPlace | null, front: string, order: string[], by: 1 | -1): MatchPlace | null {
  const files = results.files;
  if (files.length === 0) return null;
  const edge = (file: FileMatches): MatchPlace => ({ file: file.path, index: by > 0 ? 0 : file.matches.length - 1 });
  const here = at ? files.findIndex((file) => file.path === at.file) : -1;
  if (at && here >= 0 && at.index < files[here].matches.length) {
    const index = at.index + by;
    if (index >= 0 && index < files[here].matches.length) return { file: at.file, index };
    return edge(files[(here + by + files.length) % files.length]);
  }
  const inFront = files.find((file) => file.path === front);
  if (inFront) return edge(inFront);
  const place = (file: FileMatches) => order.indexOf(file.path);
  const from = order.indexOf(front);
  const nearest = by > 0 ? files.find((file) => place(file) > from) : [...files].reverse().find((file) => place(file) >= 0 && place(file) < from);
  return edge(nearest ?? (by > 0 ? files[0] : files[files.length - 1]));
}

/** How many of a file's `matches` fall under each of its `headings`, by heading; those before the first heading are under none. */
export function headingCounts(matches: MatchInfo[], headings: HeadingInfo[]): number[] {
  const counts = headings.map(() => 0);
  for (const each of matches) {
    const under = headingAt(headings, each.startLine);
    if (under >= 0) counts[under] += 1;
  }
  return counts;
}

/**
 * The raw view's marks of a file's `matches`, one for each line that holds
 * one: a bar in the gutter beside it, and a tick in the middle of the overview
 * ruler along the editor's edge, which maps the whole file, in `color` (the
 * headings and the reader's section have its sides, `minimap.ts`). The editor
 * highlights the matches themselves (its `search`).
 */
export function matchMarks(matches: MatchInfo[], color: string): EditorDecoration[] {
  const lines = new Set<number>();
  const marks: EditorDecoration[] = [];
  for (const each of matches) {
    if (lines.has(each.startLine)) continue;
    lines.add(each.startLine);
    marks.push({
      range: { start: { line: each.startLine, column: each.startColumn }, end: { line: each.startLine, column: each.startColumn } },
      gutter: { style: { backgroundColor: color } },
      overviewRuler: { lane: "center", style: { backgroundColor: color } },
    });
  }
  return marks;
}

/**
 * The regular expression the page finds `query` with, read as the raw view's
 * search reads it: plain text as written, or a pattern; any case unless asked;
 * `^` and `$` at each line. `null` for no query, or a pattern JavaScript can't
 * read (most of Rust's syntax is JavaScript's too).
 */
export function pagePattern(query: SearchQuery | null): RegExp | null {
  if (!query || query.text === "") return null;
  const source = query.regex ? query.text : query.text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  try {
    return new RegExp(source, query.caseSensitive ? "gmu" : "gimu");
  } catch {
    return null;
  }
}

const WORD_BEFORE = /[\p{L}\p{N}_]$/u;
const WORD_AFTER = /^[\p{L}\p{N}_]/u;

/** Whether `start` to `end` of `text` starts and ends at a word's edge, as the raw view's search has it: at each end, a character that isn't a word's on one side or the other. */
function isWholeWord(text: string, start: number, end: number): boolean {
  const before = WORD_BEFORE.test(text.slice(Math.max(0, start - 2), start));
  const first = WORD_AFTER.test(text.slice(start, end));
  const last = WORD_BEFORE.test(text.slice(start, end));
  const after = WORD_AFTER.test(text.slice(end, end + 2));
  return (!before || !first) && (!after || !last);
}

/** Where `pattern` matches in `text`, as ranges of UTF-16 units; whole words only with `wholeWord`. An empty match is left out: there is nothing of it to highlight. */
export function matchRanges(text: string, pattern: RegExp, wholeWord: boolean): [number, number][] {
  const ranges: [number, number][] = [];
  for (const found of text.matchAll(pattern)) {
    const start = found.index ?? 0;
    const end = start + found[0].length;
    if (end > start && (!wholeWord || isWholeWord(text, start, end))) ranges.push([start, end]);
  }
  return ranges;
}

/** The colors of the page's highlights: every match's, and the one the reader is on. */
export type HighlightColors = { match: string; current: string };

/**
 * `document`, a section of the page, with each match of `pattern` in its text
 * highlighted, whole words only with `wholeWord`, and the `current`th (from 0,
 * in the page's order; -1 for none) in its own color; and how many there were.
 */
export function highlighted(
  document: TextViewDocument,
  pattern: RegExp | null,
  wholeWord: boolean,
  current: number,
  colors: HighlightColors,
): { document: TextViewDocument; count: number } {
  if (!pattern) return { document, count: 0 };
  let count = 0;
  const marked = mapLeaves(document, (each) => {
    if (each.type !== "text") return [each];
    const ranges = matchRanges(each.text, pattern, wholeWord);
    if (ranges.length === 0) return [each];
    const parts: TextViewInline[] = [];
    let at = 0;
    for (const [start, end] of ranges) {
      if (start > at) parts.push({ type: "text", text: each.text.slice(at, start) });
      parts.push({ type: "highlight", color: count === current ? colors.current : colors.match, content: [{ type: "text", text: each.text.slice(start, end) }] });
      count += 1;
      at = end;
    }
    if (at < each.text.length) parts.push({ type: "text", text: each.text.slice(at) });
    return parts;
  });
  return { document: marked, count };
}

/** The parts of a file the page shows no text of to highlight: code, the language a fence names, where a link goes, the front matter and Markdown's punctuation. */
const UNSEEN = new Set<MarkKind>(["codeBlock", "code", "language", "url", "frontMatter", "punctuation"]);

/** Whether `inner` lies within `outer`, both as a raw view's ranges. */
function inside(inner: MatchInfo, outer: MarkInfo): boolean {
  const before = (line: number, column: number, otherLine: number, otherColumn: number) => line < otherLine || (line === otherLine && column <= otherColumn);
  return before(outer.startLine, outer.startColumn, inner.startLine, inner.startColumn) && before(inner.endLine, inner.endColumn, outer.endLine, outer.endColumn);
}

/**
 * Where match `index` of a file's `matches` shows on the page: its section (0
 * before the first of `headings`, then one for each) and which of that
 * section's highlights it is, counting only matches the page can highlight
 * (none in the parts `marks`, the file's, say are code, an address, the front
 * matter or punctuation); `null` for a match the page can't highlight.
 */
export function onThePage(matches: MatchInfo[], index: number, headings: HeadingInfo[], marks: MarkInfo[]): { section: number; nth: number } | null {
  const target = matches[index];
  if (!target) return null;
  const hidden = marks.filter((mark) => UNSEEN.has(mark.kind));
  const seen = (each: MatchInfo) => !hidden.some((mark) => inside(each, mark));
  if (!seen(target)) return null;
  const section = headingAt(headings, target.startLine) + 1;
  let nth = 0;
  for (const each of matches.slice(0, index)) {
    if (headingAt(headings, each.startLine) + 1 === section && seen(each)) nth += 1;
  }
  return { section, nth };
}

/**
 * About where line `line` of a file is on its page, as a scroll offset: its
 * section's top (`tops`, by section, as the page measured them) and as far
 * into the section as the line is into the section's lines, up to where the
 * next one starts (`end`, the page's height, after the last). `lineCount` is
 * the file's. `undefined` until the section is measured.
 */
export function pageOffset(line: number, headings: HeadingInfo[], lineCount: number, tops: (number | undefined)[], end: number): number | undefined {
  const section = headingAt(headings, line) + 1;
  const top = tops[section];
  if (top === undefined) return undefined;
  const first = section === 0 ? 0 : headings[section - 1].line;
  const last = section < headings.length ? headings[section].line : lineCount;
  const bottom = tops[section + 1] ?? end;
  const share = last > first ? (line - first) / (last - first) : 0;
  return top + Math.max(0, bottom - top) * share;
}
