import type { CommandItem, EditorDecoration, TreeItem } from "nsh/base";
import type { FileMatches, HeadingInfo, MatchInfo, SearchPlace, SearchQuery, SearchResults } from "mdviewer/native";
import { folderOf, headingAt } from "./document";

/** A match, by its file and its index among that file's matches. */
export type MatchPlace = { file: string; index: number };

/** Where the search looks. */
export type SearchScope = "file" | "tabs" | "project" | "all";

/** The scopes, with their titles. */
export const SCOPES: { value: SearchScope; title: string }[] = [
  { value: "file", title: "This file" },
  { value: "tabs", title: "Open tabs" },
  { value: "project", title: "Project" },
  { value: "all", title: "All projects" },
];

/** Where a search in `scope` looks, the open project first; nothing while none is open. */
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

/** `results` with only the files of project `project`. */
export function inProject(results: SearchResults | null, project: string): SearchResults | null {
  if (!results || results.files.every((file) => file.project === project)) return results;
  const files = results.files.filter((file) => file.project === project);
  return { ...results, files, total: files.reduce((sum, file) => sum + file.matches.length, 0) };
}

/** A row of the results: a project's, a file's or a match's. */
export type ResultRow = { kind: "project"; project: string } | { kind: "file"; project: string; file: string } | { kind: "match"; project: string; file: string; index: number };

/** A row's id: its parts apart by line breaks, which no path holds. */
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

/** The row of a `rowId`; `null` for any other id. */
export function rowOf(id: string): ResultRow | null {
  const [kind, project = "", file = "", index = ""] = id.split("\n");
  if (kind === "p") return { kind: "project", project };
  if (kind === "f") return { kind: "file", project, file };
  if (kind === "m" && index !== "") return { kind: "match", project, file, index: Number(index) };
  return null;
}

/** The most matches the drop-down lists. */
export const MOST_LISTED = 500;

/** The drop-down's rows: each match, grouped by its file's row id. */
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

/** The results as a tree of files and their matches, under their projects when there are several. */
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

/** The match a row's id names, with its file's folder; `undefined` for another row. */
export function matchOf(results: SearchResults | null, id: string): { match: MatchInfo; folder: string } | undefined {
  const row = rowOf(id);
  if (row?.kind !== "match") return undefined;
  const file = results?.files.find((each) => each.project === row.project && each.path === row.file);
  const match = file?.matches[row.index];
  return match && file ? { match, folder: folderOf(file.path) } : undefined;
}

/** How many matches a project's or a file's row holds; 0 for a match's. */
export function rowCount(results: SearchResults | null, id: string): number {
  const row = rowOf(id);
  const files = results?.files ?? [];
  if (row?.kind === "project") return files.filter((file) => file.project === row.project).reduce((sum, file) => sum + file.matches.length, 0);
  if (row?.kind === "file") return files.find((file) => file.project === row.project && file.path === row.file)?.matches.length ?? 0;
  return 0;
}

/** The UTF-16 ranges a file's `matches` cover on each line. */
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

/** `text` cut at `ranges`, which must be in order and apart. */
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

/** The index of the match nearest `line`; -1 for none. */
export function nearestMatch(matches: MatchInfo[], line: number): number {
  let best = -1;
  matches.forEach((each, index) => {
    if (best < 0 || Math.abs(each.startLine - line) < Math.abs(matches[best].startLine - line)) best = index;
  });
  return best;
}

/** An excerpt cut into before, the match, and after. */
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

/** Which of all the matches `at` is, from 1; 0 when it isn't one of them. */
export function ordinal(results: SearchResults, at: MatchPlace): number {
  let before = 0;
  for (const file of results.files) {
    if (file.path === at.file) return at.index < file.matches.length ? before + at.index + 1 : 0;
    before += file.matches.length;
  }
  return 0;
}

/** What `results` found, said shortly. */
export function searchSummary(results: SearchResults, at: MatchPlace | null): string {
  if (results.total === 0) return "No matches";
  const total = `${results.total.toLocaleString()}${results.truncated ? "+" : ""}`;
  const files = `${results.files.length.toLocaleString()} ${results.files.length === 1 ? "file" : "files"}`;
  const which = at ? ordinal(results, at) : 0;
  return which > 0 ? `Match ${which.toLocaleString()} of ${total} · ${files}` : `${total} ${results.total === 1 ? "match" : "matches"} · ${files}`;
}

/** The match a step from `at` (or from `front` when `at` is gone) takes the reader to, across files. */
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

/** How many of a file's `matches` fall under each of its `headings`. */
export function headingCounts(matches: MatchInfo[], headings: HeadingInfo[]): number[] {
  const counts = headings.map(() => 0);
  for (const each of matches) {
    const under = headingAt(headings, each.startLine);
    if (under >= 0) counts[under] += 1;
  }
  return counts;
}

/** The raw view's gutter and ruler marks, one per line that holds a match. */
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

/** The page's regular expression for `query`, read as Raw's search reads it; `null` when JavaScript can't read it. */
export function pagePattern(query: SearchQuery | null): RegExp | null {
  if (!query || query.text === "") return null;
  const source = query.regex ? query.text : query.text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  try {
    return new RegExp(source, query.caseSensitive ? "gmu" : "gimu");
  } catch {
    return null;
  }
}
