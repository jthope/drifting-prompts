import type { EditorDecoration, TextViewClasses } from "nsh/base";
import type { MarkInfo, MarkKind } from "mdviewer/native";

const MARKS: Record<MarkKind, { text?: string; line?: string }> = {
  punctuation: { text: "text-syntax-punctuation" },
  heading: { text: "font-bold text-syntax-heading" },
  strong: { text: "font-bold text-syntax-strong" },
  emphasis: { text: "italic text-syntax-emphasis" },
  strikethrough: { text: "line-through text-muted-foreground" },
  code: { text: "text-syntax-code bg-syntax-code/10" },
  link: { text: "text-syntax-link" },
  // nsh's editor drops a text color with `/alpha`, so the address is underlined instead.
  url: { text: "underline text-syntax-link" },
  marker: { text: "font-bold text-syntax-marker" },
  quote: { text: "italic text-syntax-quote" },
  codeBlock: { line: "bg-muted/50" },
  language: { text: "text-syntax-marker" },
  frontMatter: { text: "text-syntax-punctuation", line: "bg-muted/30" },
};

/** The raw view's decorations for a file's `marks`. */
export function decorationsOf(marks: MarkInfo[]): EditorDecoration[] {
  return marks.map((mark) => {
    const look = MARKS[mark.kind];
    return {
      range: { start: { line: mark.startLine, column: mark.startColumn }, end: { line: mark.endLine, column: mark.endColumn } },
      class: look.text ?? null,
      lineClass: look.line ?? null,
    };
  });
}

/** The rendered view's parts in the theme's syntax colors. */
export const PAGE: TextViewClasses = {
  heading: "text-syntax-heading",
  heading1: "border-b border-border pb-1.5",
  heading2: "border-b border-border pb-1",
  code: "rounded bg-syntax-code/10 px-1 text-syntax-code",
  link: "text-syntax-link",
  strong: "text-syntax-strong",
  emphasis: "text-syntax-emphasis",
  listMarker: "text-syntax-marker",
  blockquote: "border-syntax-quote bg-syntax-quote/10",
  codeBlock: "border border-border",
};
