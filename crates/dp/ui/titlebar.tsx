import { type JSX, View } from "nsh";
import { titleBar } from "nsh/desktop";
import { look } from "./brand";

/** The app's title bar: pressing anything but a control drags the window; it leaves the window's buttons room. */
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
