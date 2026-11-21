import { Clipboard, computed, effect, signal, untracked, WebView, type WebViewRef } from "nsh";
import { Documents, Page } from "mdviewer/native";
import { look, notify, popupMenu, theme, themeColors, themeScheme } from "../../../dp/ui";
import { failed } from "./actions";
import { type FromPage, fromPage, type PageBlock, type PageFind, type ToPage } from "./pagemessages";
import { pressFromPage } from "./shortcuts";

/** What the reader asks of the page. */
export type PageRef = {
  /** Scrolls to `y`, in CSS pixels from the page's top. */
  scrollTo(y: number, animated: boolean): void;
  /** Scrolls to the block that starts on `line` (from 0), `above` pixels under the top. */
  goToLine(line: number, above: number): void;
};

/** A file's page in a web view: loaded once, then each file, theme, search and scroll sent to it. */
export function PageView(props: {
  /** The file shown, by its path in the project: another one starts at the top. */
  file: string;
  /** Its folder, where its pictures are. */
  project: string;
  /** The file's HTML; `undefined` while it is read, which keeps the last one showing. */
  html: string | undefined;
  /** What the search highlights on it. */
  find?: PageFind;
  label: string;
  testID?: string;
  ref?: (page: PageRef | undefined) => void;
  onLayout?: (height: number, blocks: PageBlock[]) => void;
  onScroll?: (y: number, viewport: number, content: number) => void;
  onLink?: (url: string, control: boolean) => void;
  /** Where on the page each match the search highlights is. */
  onFound?: (places: number[]) => void;
}) {
  const shell = Page.shell();
  // The theme's colors as the page's CSS variables, and the document's island behind it.
  const pageTheme = computed(() => {
    const name = theme();
    const vars: Record<string, string> = {};
    for (const [token, value] of Object.entries(themeColors(name))) vars[`--${token}`] = value;
    vars["--page"] = look().tiers.document;
    vars["--found-match"] = look().found.match;
    vars["--found-current"] = look().found.current;
    return { vars, dark: themeScheme(name) === "dark" };
  });
  let web: WebViewRef | undefined;
  // Counts the page's loads, so a page that loaded again gets everything again.
  const loads = signal(0);
  let shownFile: string | undefined;
  let shownHtml: string | undefined;
  let shownFind: PageFind | undefined;
  const send = (message: ToPage) => {
    if (untracked(loads) > 0) web?.postMessage(JSON.stringify(message));
  };

  effect(() => {
    const { vars, dark } = pageTheme();
    if (loads() > 0) send({ type: "theme", vars, dark });
  });
  // One effect, so the search always comes after the HTML it highlights.
  effect(() => {
    const html = props.html;
    const file = props.file;
    const find = props.find;
    if (loads() === 0 || html === undefined) return;
    if (html !== shownHtml || file !== shownFile) {
      send({ type: "show", html, top: file === shownFile ? null : 0 });
      shownHtml = html;
      shownFile = file;
      shownFind = undefined;
    }
    if (find && find !== shownFind) send({ type: "find", ...find });
    shownFind = find;
  });

  props.ref?.({
    scrollTo: (y, animated) => send({ type: "scrollTo", y, animated }),
    goToLine: (line, above) => send({ type: "goToLine", line, above, animated: true }),
  });
  effect(() => () => props.ref?.(undefined));

  const openMenu = async (menu: Extract<FromPage, { type: "menu" }>) => {
    const place = await web?.measure();
    const at = place ? { x: place.pagePosition.x + menu.x, y: place.pagePosition.y + menu.y } : undefined;
    popupMenu(
      [
        { label: "Copy", disabled: menu.selection === "", onSelect: () => Clipboard.writeText(menu.selection).catch(failed) },
        menu.link !== "" && { label: "Copy link", onSelect: () => Clipboard.writeText(menu.link).catch(failed) },
        { type: "separator" as const },
        { label: "Select all", onSelect: () => send({ type: "selectAll" }) },
      ],
      at,
    );
  };

  const receive = (data: string) => {
    const message = fromPage(data);
    switch (message?.type) {
      case "ready":
        shownFile = undefined;
        shownHtml = undefined;
        loads.set(untracked(loads) + 1);
        return;
      case "layout":
        props.onLayout?.(message.height, message.blocks);
        return;
      case "scroll":
        props.onScroll?.(message.y, message.viewport, message.content);
        return;
      case "link":
        props.onLink?.(message.url, message.control);
        return;
      case "key":
        pressFromPage(message);
        return;
      case "menu":
        void openMenu(message);
        return;
      case "copy":
        Clipboard.writeText(message.text).catch(failed);
        return;
      case "found":
        props.onFound?.(message.places);
        return;
      case "wantPicture": {
        const id = message.id;
        Documents.picture(props.project, message.src).then(
          (picture) => send({ type: "picture", id, data: picture }),
          (error: unknown) => send({ type: "picture", id, problem: String((error as { message?: string })?.message ?? error) }),
        );
        return;
      }
    }
  };

  return (
    <WebView
      ref={(ref) => {
        web = ref;
      }}
      testID={props.testID}
      class="flex-1"
      style={{ backgroundColor: look().tiers.document }}
      html={shell}
      incognito
      accessibilityLabel={props.label}
      onMessage={(event) => receive(event.data)}
      onError={(event) => notify(`The page couldn't show: ${event.message}`, "warn")}
    />
  );
}
