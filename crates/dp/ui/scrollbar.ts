import type { ScrollbarTheme } from "nsh/base";

/** Scroll bars always shown while content is longer than its view; sideways ones stay the platform's. */
export const SCROLLBARS: ScrollbarTheme = {
  mode: "always",
  minThumbLength: 32,
  vertical: {
    class: "absolute inset-y-0 right-0 w-3.5 bg-foreground/5 p-0.5",
    thumbClass: "w-full rounded-full bg-muted-foreground/40 hover:bg-muted-foreground/60 active:bg-muted-foreground/75",
  },
  horizontal: null,
};
