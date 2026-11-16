import { type JSX, View } from "nsh";
import { titleBar } from "nsh/desktop";
import { look } from "./brand";

/**
 * The app's title bar, or what stands in for it over the whole window (a
 * snapshot's viewer, a video's player): pressing anything but a control drags
 * the window, and a double click maximizes it. The system's window buttons sit
 * over its right end, so it leaves them room. It is the theme's title bar color,
 * with no line under it: the color alone parts it from what it tops.
 * `class` adds to it (its left padding). It wraps rather than hiding anything,
 * so every control is still there in a narrow window.
 */
export function TitleBar(props: { class?: string; children: JSX.Element }) {
  return (
    <View
      class={`min-h-12 flex-row flex-wrap items-center gap-x-3 gap-y-1 py-2 ${look().titleBar} ${props.class ?? ""}`}
      style={{ paddingRight: 12 + titleBar().right }}
      desktop={{ windowDrag: true }}
    >
      {props.children}
    </View>
  );
}
