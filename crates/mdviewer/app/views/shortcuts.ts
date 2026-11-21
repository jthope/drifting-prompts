import { cleanup, keymap } from "nsh";
import { toggleFullScreen } from "../../../dp/ui";
import { keystrokeOf, normalized, type PageKey } from "./pagemessages";

// Workaround for nsh 0.6.1: a web view with the keyboard focus keeps every key from the window's
// keymap, so the page sends the app's shortcuts back and they run from here. Replaced by
// `keymap.bind` alone once nsh hands a web view's accelerator keys to the window.

const bound = new Map<string, (() => void)[]>();

/** Binds `shortcuts` with `keymap.bind` while the calling view shows, and lets the page press them too. */
export function bindShortcuts(shortcuts: Record<string, () => void>) {
  keymap.bind(shortcuts);
  for (const [keystroke, run] of Object.entries(shortcuts)) {
    const stroke = normalized(keystroke);
    bound.set(stroke, [...(bound.get(stroke) ?? []), run]);
    cleanup(() => bound.set(stroke, (bound.get(stroke) ?? []).filter((each) => each !== run)));
  }
}

/** Runs the shortcut of a key the page passed on, the one bound last, as the window would. */
export function pressFromPage(key: PageKey) {
  const stroke = keystrokeOf(key);
  // The window's own F11, which isn't in the keymap.
  if (stroke === "f11") {
    toggleFullScreen();
    return;
  }
  const runs = bound.get(stroke);
  runs?.[runs.length - 1]?.();
}
