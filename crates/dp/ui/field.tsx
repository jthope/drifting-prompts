import { type JSX, Text, View } from "nsh";
import { Checkbox, Input, Select, Slider } from "nsh/base";
import { look } from "./brand";
import { Icon } from "./icon";
import { ChipTrigger, popupSurface } from "./menu";

/**
 * A label above a control, with help or an error under it. An error replaces
 * the help. `action` sits at the label's end: a small button about the field,
 * such as one that undoes a change to it.
 */
export function Field(props: { label: string; help?: string; error?: string; action?: JSX.Element; class?: string; children: JSX.Element }) {
  return (
    <View class={`gap-1.5 ${props.class ?? ""}`}>
      {/* As tall as a small button, so the label stays put when the action comes and goes. */}
      <View class={`flex-row items-center gap-2 ${props.action ? "-my-1 h-6" : ""}`}>
        <Text class="min-w-0 flex-1 text-sm text-muted-foreground" style={{ numberOfLines: 1 }}>
          {props.label}
        </Text>
        {props.action}
      </View>
      {props.children}
      <If when={props.error}>{(error) => <Text class="text-xs text-destructive">{error()}</Text>}</If>
      <If when={!props.error && props.help}>
        <Text class="text-xs text-muted-foreground">{props.help}</Text>
      </If>
    </View>
  );
}

/** A titled group of fields in a form. */
export function FormSection(props: { title: string; children: JSX.Element }) {
  return (
    <View class="gap-3">
      <Text role="header" class="text-sm font-semibold text-foreground">
        {props.title}
      </Text>
      {props.children}
    </View>
  );
}

/** Fields side by side, sharing the width evenly. */
export function FieldRow(props: { children: JSX.Element }) {
  return <View class="flex-row items-start gap-3">{props.children}</View>;
}

/**
 * A labeled text input. `masked` hides what is typed, for passwords. `action` is the `Field`'s, in
 * the label's row; `trailing`, a button beside the input itself. `readOnly` shows the value and
 * takes no typing: a value something else on the form decides.
 */
export function TextField(props: {
  label: string;
  value: string;
  placeholder?: string;
  help?: string;
  error?: string;
  masked?: boolean;
  numeric?: boolean;
  autoFocus?: boolean;
  readOnly?: boolean;
  action?: JSX.Element;
  trailing?: JSX.Element;
  class?: string;
  onChange: (value: string) => void;
  onSubmit?: () => void;
}) {
  const input = (wide: string) => (
    <Input
      value={props.value}
      placeholder={props.placeholder}
      masked={props.masked}
      autoFocus={props.autoFocus}
      readonly={props.readOnly}
      keyboardType={props.numeric ? "number-pad" : "default"}
      autoCapitalize="none"
      autoCorrect={false}
      accessibilityLabel={props.label}
      class={`${wide} h-8 rounded-md border px-2.5 text-sm ${look().field.frame} ${look().rings.field} ${props.error ? "border-destructive" : look().field.border}`}
      style={look().well}
      onChange={({ value }) => props.onChange(value)}
      onSubmit={() => props.onSubmit?.()}
    />
  );
  return (
    <Field label={props.label} help={props.help} error={props.error} action={props.action} class={props.class}>
      {props.trailing ? (
        <View class="flex-row items-center gap-2">
          {input("min-w-0 flex-1")}
          {props.trailing}
        </View>
      ) : (
        input("")
      )}
    </Field>
  );
}

/**
 * One choice from a short list, as a compact button showing the choice, for a
 * toolbar. `label` is what it chooses: what a screen reader says.
 */
export function InlineSelect<T extends string>(props: {
  label: string;
  items: { value: T; title: string }[];
  value: T;
  class?: string;
  onChange: (value: T) => void;
}) {
  const title = () => props.items.find((item) => item.value === props.value)?.title ?? "";
  const surface = () => popupSurface("p-1");
  return (
    <Select
      items={props.items}
      value={props.value}
      accessibilityLabel={props.label}
      offset={4}
      popupClass={surface().class}
      popupStyle={surface().style}
      itemClass={`rounded-md px-2 py-1.5 text-sm ${look().menuItem.option}`}
      trigger={
        <ChipTrigger class={`justify-between ${props.class ?? ""}`}>
          <Text class={`flex-1 text-sm ${look().chip.text}`} style={{ numberOfLines: 1 }}>
            {title()}
          </Text>
        </ChipTrigger>
      }
      onChange={({ value }) => value !== null && props.onChange(value as T)}
    />
  );
}

/** One choice from a short list, with a label above it. `action` is the `Field`'s. */
export function SelectField<T extends string>(props: {
  label: string;
  items: { value: T; title: string }[];
  value: T;
  help?: string;
  action?: JSX.Element;
  class?: string;
  onChange: (value: T) => void;
}) {
  return (
    <Field label={props.label} help={props.help} action={props.action} class={props.class}>
      <InlineSelect label={props.label} items={props.items} value={props.value} onChange={props.onChange} />
    </Field>
  );
}

/**
 * A checkbox's box alone, checked in the theme's colors. `mixed` shows a dash
 * for neither, as a "select all" over some does; a press then asks for
 * checked. `label` is what a screen reader says.
 */
export function CheckBox(props: { label: string; checked: boolean; mixed?: boolean; class?: string; onChange: (checked: boolean) => void; children?: JSX.Element }) {
  const check = () => look().check;
  const marked = () => props.checked || props.mixed === true;
  return (
    <Checkbox
      checked={props.checked}
      indeterminate={props.mixed}
      accessibilityLabel={props.label}
      class={`min-h-6 flex-row items-center gap-2 ${props.class ?? ""}`}
      indicatorClass={`size-4 items-center justify-center rounded border ${check().box} ${look().rings.visible}`}
      indicatorStyle={marked() ? check().style : {}}
      indicator={
        <If when={marked()}>
          <Icon name={props.mixed ? "minus" : "check"} size={12} tint="primary-foreground" color={check().icon} />
        </If>
      }
      onChange={({ checked }) => props.onChange(checked)}
    >
      {props.children}
    </Checkbox>
  );
}

/**
 * A checkbox with its label beside it, checked in the theme's colors. `mixed`
 * is `CheckBox`'s; `action` sits after the label, as a `Field`'s does.
 */
export function CheckField(props: { label: string; checked: boolean; mixed?: boolean; action?: JSX.Element; onChange: (checked: boolean) => void }) {
  return (
    <View class="flex-row items-center gap-2">
      <CheckBox label={props.label} checked={props.checked} mixed={props.mixed} class="min-w-0 flex-1" onChange={props.onChange}>
        <Text class="text-sm text-foreground">{props.label}</Text>
      </CheckBox>
      {props.action}
    </View>
  );
}

/** The theme color a slider's filled part and thumb are drawn in. */
export type SliderTone = "primary" | "info";

const tones: Record<SliderTone, { range: string; thumb: string }> = {
  primary: { range: "bg-primary", thumb: "border-primary" },
  info: { range: "bg-info", thumb: "border-info" },
};

/** A horizontal slider, with its value after it when `format` gives one. A theme can fill it in its own colors, whatever its tone. */
export function LevelSlider(props: {
  label: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  tone?: SliderTone;
  format?: (value: number) => string;
  class?: string;
  onChange: (value: number) => void;
  onChangeEnd?: (value: number) => void;
}) {
  const tone = () => tones[props.tone ?? "primary"];
  const themed = () => look().slider;
  return (
    <Slider
      value={props.value}
      min={props.min}
      max={props.max}
      step={props.step}
      accessibilityLabel={props.label}
      class={`flex-row items-center gap-2 ${props.class ?? ""}`}
      trackClass={`h-1.5 flex-1 rounded-full bg-muted px-2 ${look().rings.within}`}
      trackStyle={look().well}
      indicatorClass="h-full"
      rangeClass={`-ml-2 rounded-full ${tone().range}`}
      rangeStyle={themed()?.rangeStyle ?? {}}
      thumbClass={`size-3.5 rounded-full border ${themed()?.thumb ?? tone().thumb} bg-background`}
      thumbStyle={look().thumb}
      onChange={({ value }) => props.onChange(value as number)}
      onChangeEnd={({ value }) => props.onChangeEnd?.(value as number)}
    >
      <If when={props.format}>
        {(format) => <Text class="w-10 text-right text-xs tabular-nums text-muted-foreground">{format()(props.value)}</Text>}
      </If>
    </Slider>
  );
}
