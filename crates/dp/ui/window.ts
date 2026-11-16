// The main window: reopening where it was left, and full screen.
//
// Each app keeps where its window was left in its own `settings.json` (its
// bridge's `AppWindow`, as a `WindowPlace`). `keepWindowPlace` gives it to nsh
// as the app first renders, before the first frame shows, and nsh puts the
// window there, fitted on the displays there are now. nsh then reports where
// the window settles once a move or a resize stops (`state().placement`), and
// each new place is saved. Full screen is the window's mode, which F11 toggles
// natively, so what the app shows follows the mode rather than its own button.
import { effect } from "nsh";
import { Window, type WindowMode } from "nsh/desktop";

/**
 * Where the window was left, as an app saves it (`dp_ui::WindowPlace`): its
 * bounds out of maximized and full screen, in points on the screen, and
 * whether it was maximized. A width of 0 is nothing saved yet.
 */
export interface WindowPlace {
  x: number;
  y: number;
  width: number;
  height: number;
  maximized: boolean;
}

/** The mode leaving full screen goes back to: the last other one the window was in. */
let before: WindowMode = "normal";

/** Whether the main window is in full screen (reactive), however it got there. Never off the desktops. */
export function isFullScreen(): boolean {
  return Window.get("main")?.state()?.mode === "fullScreen";
}

/** Puts the main window in full screen, or takes it back to the mode it had before. */
export function toggleFullScreen(): void {
  Window.get("main")?.setMode(isFullScreen() ? before : "fullScreen");
}

/** Takes the main window out of full screen, back to the mode it had before; outside full screen, nothing. */
export function leaveFullScreen(): void {
  if (isFullScreen()) Window.get("main")?.setMode(before);
}

/**
 * Opens the main window where `saved` says it was left, then calls `save`
 * with each new place it settles in. Call it once, from the app's root as it
 * first renders, so the window is there before it shows. Nothing is saved
 * while the window is in full screen or minimized, which isn't where it goes
 * back to.
 */
export function keepWindowPlace(saved: WindowPlace, save: (place: WindowPlace) => void): void {
  const main = Window.get("main");
  if (!main) return;
  if (saved.width > 0 && saved.height > 0) {
    main.setPlacement({
      mode: saved.maximized ? "maximized" : "normal",
      bounds: { origin: { x: saved.x, y: saved.y }, size: { width: saved.width, height: saved.height } },
    });
  }
  let last = saved;
  effect(() => {
    const state = main.state();
    if (!state) return;
    if (state.mode !== "fullScreen") before = state.mode;
    if (state.mode === "fullScreen" || state.minimized || !state.visible) return;
    const { origin, size } = state.placement.bounds;
    const place = { x: origin.x, y: origin.y, width: size.width, height: size.height, maximized: state.placement.mode === "maximized" };
    if (place.x === last.x && place.y === last.y && place.width === last.width && place.height === last.height && place.maximized === last.maximized) return;
    last = place;
    save(place);
  });
}
