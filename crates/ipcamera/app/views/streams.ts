// The stream choice in a camera's settings: what the camera itself lists, never
// the app's own idea of what a brand's cameras have.
import type { CameraForm } from "ipcamera/native";

/**
 * The streams to choose from, when the camera has listed any (Connect asks it):
 * each with its size, its codec and its name as the menus show it, largest
 * first and each other lens's after the main lens's, named after its lens; and
 * the saved path when that is none of them, so opening the form loses nothing.
 * Nothing here is the app's own guess at what a camera has. Empty for a camera
 * that has listed none, whose stream is reached by channel and quality or at
 * a path typed by hand.
 */
export function streamChoices(form: Pick<CameraForm, "streams" | "rtspPath">): { value: string; title: string }[] {
  if (form.streams.length === 0) return [];
  const typed = form.rtspPath.trim();
  const listed = form.streams.map((stream) => ({
    value: stream.path,
    title: stream.lens ? `${stream.title} · ${stream.lens}` : stream.title,
  }));
  const own = typed !== "" && !form.streams.some((stream) => stream.path === typed) ? [{ value: typed, title: `Saved path: ${typed}` }] : [];
  return [...listed, ...own];
}
