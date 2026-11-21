// F11 toggles full screen natively, so the app follows the window's mode rather than its own button.
import { effect } from "nsh";
import { Window, type WindowMode } from "nsh/desktop";

/** Where the window was left, as an app saves it (`dp_ui::WindowPlace`); a width of 0 is nothing saved. */
export interface WindowPlace {
  x: number;
  y: number;
  width: number;
  height: number;
  maximized: boolean;
}

let before: WindowMode = "normal";

/** Whether the main window is in full screen (reactive). */
export function isFullScreen(): boolean {
  return Window.get("main")?.state()?.mode === "fullScreen";
}

/** Puts the main window in full screen, or takes it back to the mode it had before. */
export function toggleFullScreen(): void {
  Window.get("main")?.setMode(isFullScreen() ? before : "fullScreen");
}

/** Takes the main window out of full screen, back to the mode it had before. */
export function leaveFullScreen(): void {
  if (isFullScreen()) Window.get("main")?.setMode(before);
}

/** Opens the main window where `saved` left it and saves each new place, but no full screen or minimized one. */
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
