import type { CameraForm } from "ipcamera/native";

/** The streams the camera listed, as the menus show them, and the saved path when it is none of them; empty for a camera that listed none. */
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
