// What the document views work out from a file, kept apart from drawing so
// the tests can read it: the rendered view's sections, the raw view's folds,
// where a link goes, and where a relative picture is on disk.
//
// The outline is Rust's (`mdviewer_core::summarize`): the file's top-level
// headings, each with its line. The raw view goes to that line. The rendered
// view shows the parsed document (`TextView.parse`) a section at a time, one
// `TextView` per top-level heading, so it can measure where each one is and
// scroll there; it counts its headings the same way, so heading `i` of the
// outline is section `i + 1` (section 0 is what comes before the first).
import type { EditorFoldRange, MarkdownOptions, TextViewBlock, TextViewDocument, TextViewInline } from "nsh/base";
import type { HeadingInfo } from "mdviewer/native";

/**
 * The Markdown the rendered view reads beyond CommonMark and GitHub's: front
 * matter as its own block rather than a rule and a heading, as the outline
 * reads it too. Formulas, alerts and HTML are on by default.
 */
export const MARKDOWN: MarkdownOptions = { frontmatter: true };

/** Words a minute, for the status bar's reading time. */
const READING_SPEED = 200;

/** About how many minutes `words` take to read, at least one for any text. */
export function readingMinutes(words: number): number {
  return words === 0 ? 0 : Math.max(1, Math.round(words / READING_SPEED));
}

/**
 * `document` cut at its top-level headings: what comes before the first one
 * (possibly nothing), then each heading with what follows it up to the next.
 * The footnotes go with the last section, where the page shows them.
 */
export function splitSections(document: TextViewDocument): TextViewDocument[] {
  const sections: TextViewBlock[][] = [[]];
  for (const block of document.blocks) {
    if (block.type === "heading") sections.push([]);
    sections[sections.length - 1].push(block);
  }
  return sections.map((blocks, index) => ({
    blocks,
    footnotes: index === sections.length - 1 ? document.footnotes : undefined,
  }));
}

/** The anchors of `document`'s top-level headings, in order: what an `#anchor` link names. */
export function headingAnchors(document: TextViewDocument): string[] {
  return document.blocks.filter((block) => block.type === "heading").map((block) => (block.type === "heading" ? (block.anchor ?? "") : ""));
}

/**
 * Where each heading folds in the raw view: from its line to the last line
 * before the next heading of its level or higher, blank lines at the end left
 * out, so a folded section shows only its heading. `lines` is the file's text,
 * a line each.
 */
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

/** The heading `line` is under: the last that starts on or before it; -1 above the first. */
export function headingAt(headings: HeadingInfo[], line: number): number {
  let found = -1;
  headings.forEach((heading, index) => {
    if (heading.line <= line) found = index;
  });
  return found;
}

/** Where a link goes: a heading of this file, another file of the project, or somewhere outside the app. */
export type LinkTarget = { kind: "anchor"; anchor: string } | { kind: "file"; path: string; anchor: string } | { kind: "outside"; url: string } | { kind: "none" };

const SCHEME = /^[a-z][a-z0-9+.-]*:/i;

/**
 * Where link `url` goes, pressed in file `from` (its path in the project,
 * `docs/setup.md`): `#install` is a heading of this file, `guide.md#usage`
 * another file of the project, relative to this one's folder, and anything
 * with a scheme (`https:`, `mailto:`) is outside the app. A path that would
 * leave the project goes nowhere.
 */
export function linkTarget(from: string, url: string): LinkTarget {
  if (url.startsWith("#")) return { kind: "anchor", anchor: decode(url.slice(1)) };
  if (SCHEME.test(url)) return { kind: "outside", url };
  const [path, anchor = ""] = url.split("#", 2);
  const resolved = resolve(folderOf(from), decode(path));
  return resolved === undefined || resolved === "" ? { kind: "none" } : { kind: "file", path: resolved, anchor: decode(anchor) };
}

/**
 * Where picture `src`, written in file `from`, is: a URL with a scheme as it
 * is, else the file on disk, relative to `from`'s folder in the project at
 * `project` (an absolute folder), as `<Image src>` takes it. A path that would
 * leave the project stays as written, and doesn't load.
 */
export function pictureSource(project: string, from: string, src: string): string {
  if (SCHEME.test(src) || src === "") return src;
  const resolved = resolve(folderOf(from), decode(src.split(/[?#]/, 1)[0]));
  if (resolved === undefined) return src;
  const separator = project.includes("\\") ? "\\" : "/";
  return [project.replace(/[\\/]+$/, ""), ...resolved.split("/")].join(separator);
}

/** `document` with each relative picture's `src` made the file's on disk (`pictureSource`), so the page shows it. */
export function withPictures(document: TextViewDocument, source: (src: string) => string): TextViewDocument {
  return mapLeaves(document, (each) => [each.type === "image" ? { ...each, src: source(each.src) } : each]);
}

/**
 * `document` with each inline that holds no others (text, code, a picture)
 * replaced by what `leaf` makes of it, none or several, inside every block and
 * every inline that holds others (a link, strong text). `leaf` sees them in the
 * order the page shows them, the footnotes last.
 */
export function mapLeaves(document: TextViewDocument, leaf: (each: TextViewInline) => TextViewInline[]): TextViewDocument {
  const inlines = (each: TextViewInline[]): TextViewInline[] => each.flatMap(inline);
  const inline = (each: TextViewInline): TextViewInline[] => {
    switch (each.type) {
      case "link":
      case "strong":
      case "emphasis":
      case "strikethrough":
      case "underline":
      case "superscript":
      case "subscript":
      case "small":
      case "highlight":
        return [{ ...each, content: inlines(each.content) }];
      default:
        return leaf(each);
    }
  };
  const block = (each: TextViewBlock): TextViewBlock => {
    switch (each.type) {
      case "paragraph":
      case "heading":
        return { ...each, content: inlines(each.content) };
      case "blockquote":
        return { ...each, children: each.children.map(block) };
      case "details":
        return { ...each, summary: inlines(each.summary), children: each.children.map(block) };
      case "list":
        return { ...each, items: each.items.map((item) => ({ ...item, children: item.children.map(block) })) };
      case "table":
        return {
          ...each,
          head: each.head.map((cell) => ({ ...cell, content: inlines(cell.content) })),
          rows: each.rows?.map((row) => row.map((cell) => ({ ...cell, content: inlines(cell.content) }))),
        };
      default:
        return each;
    }
  };
  return {
    blocks: document.blocks.map(block),
    footnotes: document.footnotes?.map((note) => ({ ...note, children: note.children.map(block) })),
  };
}

/** The folder of `path` in the project (`docs` for `docs/setup.md`), or `""` at its top. */
export function folderOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? "" : path.slice(0, at);
}

/** `relative` taken from `folder`, both in the project with `/` between their parts; `undefined` when it leaves the project. */
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
