// The scroll bars of a window that reads long text, drawn by the app in place
// of the platform's (`Scrollbar.setTheme`): always there while there is more
// than fits, so where the reader is and how much is left stay in view, and as
// wide as a code editor's, so the thumb is easy to take hold of. Their colors
// are the theme's: a faint tint of its text along the track, its muted text
// for the thumb, fuller under the pointer and while it is dragged.
import type { ScrollbarTheme } from "nsh/base";

/**
 * Bars always shown while the content is longer than its view, 14 points wide
 * (the thumb 10, inside the track's padding), the thumb never shorter than 32
 * points. Sideways bars stay the platform's: the only sideways scroll is a
 * strip of tabs, which the mouse's wheel moves.
 */
export const SCROLLBARS: ScrollbarTheme = {
  mode: "always",
  minThumbLength: 32,
  vertical: {
    class: "absolute inset-y-0 right-0 w-3.5 bg-foreground/5 p-0.5",
    thumbClass: "w-full rounded-full bg-muted-foreground/40 hover:bg-muted-foreground/60 active:bg-muted-foreground/75",
  },
  horizontal: null,
};
