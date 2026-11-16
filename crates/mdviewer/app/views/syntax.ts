// How the reader colors Markdown, in both its views, in the shown theme's
// syntax colors (`syntax-*` in `crates/dp/ui/theme.json` and every theme file).
//
// Raw is nsh's code editor, which highlights Markdown in nsh's own colors, the
// same in every theme. Over that go the app's decorations (`decorationsOf`),
// one for each part the file's marks name (`mdviewer_core::marks`, through the
// bridge): Markdown's punctuation quieted, so the words stand out; headings and
// strong text bold, emphasis and quotes in italics; code in text on a tint of
// its color; a link's text apart from where it goes; and a band behind each
// code block's lines. The code inside a fenced block keeps nsh's colors, which
// the rendered view's code blocks share.
//
// Rendered is nsh's `TextView`, its parts given the same colors (`PAGE`):
// headings, a line under the top two levels, code in text, links, strong and
// emphasis, list markers, quotes and code blocks.
import type { EditorDecoration, TextViewClasses } from "nsh/base";
import type { MarkInfo, MarkKind } from "mdviewer/native";

/** The classes of each kind of mark: of its text, and of its whole lines (a band behind them). */
const MARKS: Record<MarkKind, { text?: string; line?: string }> = {
  punctuation: { text: "text-syntax-punctuation" },
  heading: { text: "font-bold text-syntax-heading" },
  strong: { text: "font-bold text-syntax-strong" },
  emphasis: { text: "italic text-syntax-emphasis" },
  strikethrough: { text: "line-through text-muted-foreground" },
  code: { text: "text-syntax-code bg-syntax-code/10" },
  link: { text: "text-syntax-link" },
  // A text color takes no `/alpha` here (nsh 0.5.1's editor drops the class), so the address is the link's color, underlined.
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
