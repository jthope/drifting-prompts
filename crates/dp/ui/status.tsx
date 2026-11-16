import { type JSX, Text, View } from "nsh";
import { look } from "./brand";
import { Icon, type IconName, type IconTint } from "./icon";
import { Press } from "./press";

/** How a status reads: nothing to report, working as it should, or needing attention. */
export type StatusTone = "normal" | "ok" | "warn";

const tabular = { fontVariant: ["tabular-nums" as const] };

const dots: Record<StatusTone, string> = { normal: "bg-muted-foreground", ok: "bg-success", warn: "bg-warning" };
const texts: Record<StatusTone, string> = { normal: "text-muted-foreground", ok: "text-success", warn: "text-warning" };

/** A small dot in a status's color, for a list or a header. */
export function StatusDot(props: { tone: StatusTone; class?: string }) {
  return <View class={`size-2 rounded-full ${dots[props.tone]} ${props.class ?? ""}`} />;
}

/** A status with its dot before it. */
export function StatusLine(props: { tone: StatusTone; text: string; class?: string }) {
  return (
    <View class={`flex-row items-center gap-2 ${props.class ?? ""}`}>
      <StatusDot tone={props.tone} />
      <Text class={`flex-1 text-sm ${texts[props.tone]}`} style={{ numberOfLines: 1 }}>
        {props.text}
      </Text>
    </View>
  );
}

/**
 * One item of the window's status bar: an icon in its color (or `mark` in its
 * place) and a short text, which opens what it counts. `label` says both, for
 * a screen reader. It hovers as a quiet button does in the theme shown.
 */
export function StatusItem(props: { icon?: IconName; tint?: IconTint; mark?: JSX.Element; text: string; label: string; onPress: () => void }) {
  return (
    <Press label={props.label} scale={1} class={`h-6 flex-row items-center gap-1.5 rounded-md px-1.5 ${look().buttons.ghost.frame}`} onPress={props.onPress}>
      <If when={props.mark} else={<If when={props.icon}>{(icon) => <Icon name={icon()} size={14} tint={props.tint} />}</If>}>
        {props.mark}
      </If>
      <Text class={`text-xs ${look().statusBar.text}`} style={tabular}>
        {props.text}
      </Text>
    </Press>
  );
}

/**
 * A status bar item that only says something, with nothing to open: an icon
 * in its color and a short text, as `StatusItem` draws them, at rest.
 */
export function StatusText(props: { icon?: IconName; tint?: IconTint; text: string }) {
  return (
    <View class="h-6 flex-row items-center gap-1.5 px-1.5">
      <If when={props.icon}>{(icon) => <Icon name={icon()} size={14} tint={props.tint} />}</If>
      <Text class={`text-xs ${look().statusBar.text}`} style={tabular}>
        {props.text}
      </Text>
    </View>
  );
}

/**
 * A meter's fill, as the dots' are kept: in a table, where nsh's check of class
 * names, which doesn't know the app's own `warning` token, leaves it alone.
 */
const meterFills = { ok: "bg-chart-1", warn: "bg-warning" };

/** A small bar filled to `value`, 0 to 1: in the first chart color, or the warning color when `warn`. */
export function Meter(props: { value: number; warn?: boolean; class?: string }) {
  const percent = (): `${number}%` => `${Math.round(Math.max(0, Math.min(1, props.value)) * 100)}%`;
  return (
    <View class={`h-1.5 w-28 overflow-hidden rounded-full bg-muted-foreground/15 ${props.class ?? ""}`}>
      <View class={`h-full rounded-full ${props.warn ? meterFills.warn : meterFills.ok}`} style={{ width: percent() }} />
    </View>
  );
}
