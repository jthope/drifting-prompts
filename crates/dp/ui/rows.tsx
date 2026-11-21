import { type JSX, Text, View } from "nsh";
import { BrandBadge, look } from "./brand";
import { Card } from "./card";
import type { IconName } from "./icon";

/** A titled card of settings rows; `description` says what they're for. */
export function SettingsGroup(props: { title: string; description?: string; children: JSX.Element }) {
  return (
    <View class="gap-2">
      <View class="gap-0.5 px-1">
        <Text role="header" class="text-sm font-semibold text-foreground">
          {props.title}
        </Text>
        <If when={props.description}>
          <Text class="text-sm text-muted-foreground">{props.description}</Text>
        </If>
      </View>
      {/* Clips its rows' hover fills to its corners; its own shadow still draws whole. */}
      <Card class="overflow-hidden">{props.children}</Card>
    </View>
  );
}

type SettingProps = {
  icon: IconName;
  label: string;
  detail?: string;
  tag?: string;
  control?: JSX.Element;
  children?: JSX.Element;
};

/** One setting: its badge, name and `detail`, its control at the end, and `children` under it. */
export function Setting(props: SettingProps) {
  return (
    <View class="gap-3">
      <View class="flex-row flex-wrap items-center gap-x-4 gap-y-3">
        <View class="min-w-56 flex-1 flex-row items-center gap-3">
          <BrandBadge icon={props.icon} size={32} />
          <View class="min-w-0 flex-1 gap-0.5">
            <View class="flex-row items-center gap-2">
              <Text class="text-sm font-medium text-card-foreground">{props.label}</Text>
              <If when={props.tag}>
                <Text class="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">{props.tag}</Text>
              </If>
            </View>
            <If when={props.detail}>
              <Text class="text-xs text-muted-foreground">{props.detail}</Text>
            </If>
          </View>
        </View>
        <If when={props.control}>
          <View class="flex-row flex-wrap items-center gap-2">{props.control}</View>
        </If>
      </View>
      {/* `pl-11`: the badge's 32 points and the 12 after it. */}
      <If when={props.children}>
        <View class="pl-11">{props.children}</View>
      </If>
    </View>
  );
}

/** A `Setting` as a row of its group's card, ruled above unless `first`. */
export function SettingRow(props: SettingProps & { first?: boolean }) {
  return (
    <View class={`px-4 py-3.5 ${props.first ? "" : "border-t border-border"}`}>
      <Setting {...props} />
    </View>
  );
}

/** Several settings in one row of a group's card, with no lines between them. */
export function SettingsPanel(props: { children: JSX.Element }) {
  return <View class="gap-5 px-4 py-3.5">{props.children}</View>;
}
