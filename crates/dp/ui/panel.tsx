import { type JSX, Text, View } from "nsh";
import { titleBar } from "nsh/desktop";
import { BrandBadge, EdgeLine, look } from "./brand";
import type { IconName } from "./icon";

/**
 * The top of a pane's island beside a reader (its files, its outline): its
 * name, quietly, then its buttons (`children`), as tall as the strip of tabs
 * it lines up with (`FileTabs`), on the island's own color over a panel's line.
 */
export function PaneHeader(props: { title: string; children?: JSX.Element }) {
  return (
    <View class="h-9 flex-row items-center gap-0.5 pl-3 pr-1">
      <Text role="header" class="flex-1 text-xs font-medium text-muted-foreground" style={{ numberOfLines: 1 }}>
        {props.title}
      </Text>
      {props.children}
      <EdgeLine edge="bottom" place="panel" />
    </View>
  );
}

/**
 * The top of a panel (the PTZ panel, an editor's dialog): its icon on the
 * theme's badge, its title, and the button that closes it (`action`), in the
 * theme's header color over the theme's line; on an island (`island`, the PTZ
 * panel), in the island's own color over a panel's line, as a `PaneHeader` is.
 * `atTop` is for a panel that reaches the window's top edge, where its header
 * is the title bar: it drags the window and leaves the system's window buttons
 * their room, so they never cover its own. `ruled` draws a panel's line under
 * it in every theme, where a page's header draws none in some (a dialog's,
 * which parts its header from its body).
 */
export function PanelHeader(props: { title: string; icon: IconName; action: JSX.Element; atTop?: boolean; island?: boolean; ruled?: boolean }) {
  return (
    <View
      class={`h-12 flex-row items-center gap-2.5 pl-3 pr-1.5 ${props.island ? "" : look().header}`}
      style={props.atTop ? { paddingRight: 6 + titleBar().right } : {}}
      desktop={props.atTop ? { windowDrag: true } : undefined}
    >
      <BrandBadge icon={props.icon} size={26} />
      <Text role="header" class="flex-1 text-sm font-semibold text-foreground" style={{ numberOfLines: 1 }}>
        {props.title}
      </Text>
      {props.action}
      <EdgeLine edge="bottom" place={props.island || props.ruled ? "panel" : "page"} />
    </View>
  );
}
