import { type JSX, Text, View } from "nsh";
import { titleBar } from "nsh/desktop";
import { BrandBadge, EdgeLine, look } from "./brand";
import type { IconName } from "./icon";

/** The top of a pane's island beside a reader, as tall as the strip of tabs it lines up with. */
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

/** The top of a panel; `atTop` makes it the title bar, leaving the window's buttons their room. */
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
