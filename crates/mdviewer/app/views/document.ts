import type { EditorFoldRange, MarkdownOptions } from "nsh/base";
import type { HeadingInfo } from "mdviewer/native";

/** The map preview's Markdown options: front matter as its own block, as the outline reads it. */
export const MARKDOWN: MarkdownOptions = { frontmatter: true };

const READING_SPEED = 200;

/** About how many minutes `words` take to read, at least one for any text. */
export function readingMinutes(words: number): number {
  return words === 0 ? 0 : Math.max(1, Math.round(words / READING_SPEED));
}

/** Where each heading folds in the raw view, trailing blank lines left out. */
export function foldRanges(headings: HeadingInfo[], lines: string[]): EditorFoldRange[] {
  const ranges: EditorFoldRange[] = [];
  headings.forEach((heading, index) => {
    const next = headings.slice(index + 1).find((each) => each.level <= heading.level);
    let end = (next ? next.line : lines.length) - 1;
    while (end > heading.line && lines[end].trim() === "") end -= 1;
    if (end > heading.line) ranges.push({ startLine: heading.line, endLine: end });
  });
  return ranges;
}

/** The heading `line` is under; -1 above the first. */
export function headingAt(headings: HeadingInfo[], line: number): number {
  let found = -1;
  headings.forEach((heading, index) => {
    if (heading.line <= line) found = index;
  });
  return found;
}

/** Where a link goes. */
export type LinkTarget = { kind: "anchor"; anchor: string } | { kind: "file"; path: string; anchor: string } | { kind: "outside"; url: string } | { kind: "none" };

const SCHEME = /^[a-z][a-z0-9+.-]*:/i;

/** Where link `url` in file `from` goes; a path that would leave the project goes nowhere. */
export function linkTarget(from: string, url: string): LinkTarget {
  if (url.startsWith("#")) return { kind: "anchor", anchor: decode(url.slice(1)) };
  if (SCHEME.test(url)) return { kind: "outside", url };
  const [path, anchor = ""] = url.split("#", 2);
  const resolved = resolve(folderOf(from), decode(path));
  return resolved === undefined || resolved === "" ? { kind: "none" } : { kind: "file", path: resolved, anchor: decode(anchor) };
}

/** The folder of `path` in the project, or `""` at its top. */
export function folderOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? "" : path.slice(0, at);
}

/** `undefined` when `relative` leaves the project. */
function resolve(folder: string, relative: string): string | undefined {
  const parts = relative.startsWith("/") ? [] : folder.split("/").filter((part) => part !== "");
  for (const part of relative.split(/[\\/]/)) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (parts.length === 0) return undefined;
      parts.pop();
    } else {
      parts.push(part);
    }
  }
  return parts.join("/");
}

function decode(text: string): string {
  try {
    return decodeURIComponent(text);
  } catch {
    return text;
  }
}
