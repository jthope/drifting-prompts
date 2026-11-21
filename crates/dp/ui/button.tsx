import { context, Text, View, type ViewStyle } from "nsh";
import { Button as BaseButton, Tooltip } from "nsh/base";
import { look, OVERLAY } from "./brand";
import { Icon, type IconName, type IconTint } from "./icon";

/** `primary` the main action, `secondary` an ordinary one, `ghost` a quiet one, `danger` one that deletes, `overlay` one over a picture. */
export type ButtonKind = "primary" | "secondary" | "ghost" | "danger" | "overlay";

type KindLook = { frame: string; style: ViewStyle; label: string; tint: IconTint; color?: string };

function kindLook(kind: ButtonKind): KindLook {
  const shown = look();
  if (kind === "primary") {
    const primary = shown.primary;
    return { frame: primary.frame, style: primary.style, label: primary.label, tint: "primary-foreground" };
  }
  if (kind === "danger") return { frame: shown.danger.frame, style: {}, label: shown.danger.label, tint: "foreground", color: shown.danger.ink };
  if (kind === "overlay") return { frame: OVERLAY.button, style: {}, label: "text-white", tint: "foreground", color: OVERLAY.ink };
  const plain = shown.buttons[kind];
  return { ...plain, frame: `${plain.frame} ${shown.on}`, style: kind === "secondary" ? shown.control : {} };
}

const SHAPE = "flex-row items-center justify-center gap-1.5 disabled:opacity-50";

/** Whether buttons sit on a pill (`OverlayToolbar`), rounding their own ends to follow its curve. */
export const OnPill = context(false);

/** Read as the button is made, under its pill's provider. */
const corners = () => (OnPill() ? "rounded-full" : "rounded-md");

const FRAME = `h-8 ${SHAPE}`;

const SIZES = {
  tiny: { square: "h-5 w-5", icon: 12 },
  small: { square: "h-6 w-6", icon: 14 },
  regular: { square: "h-8 w-8", icon: 16 },
  large: { square: "h-11 w-11", icon: 16 },
} as const;

/** A short hint under a control, as a view's `tooltip`. */
export function Hint(props: { text: string }) {
  const tooltip = () => look().tooltip;
  return (
    <Tooltip placement="bottom" offset={6} class={tooltip().class} style={tooltip().style}>
      <Text class={`text-xs ${tooltip().text}`}>{props.text}</Text>
    </Tooltip>
  );
}

/** A button with a label, and an icon before it when it has one. */
export function Button(props: {
  label: string;
  icon?: IconName;
  kind?: ButtonKind;
  disabled?: boolean;
  accessibilityLabel?: string;
  class?: string;
  onPress?: () => void;
}) {
  const kind = () => kindLook(props.kind ?? "secondary");
  const round = corners();
  return (
    <BaseButton
      disabled={props.disabled}
      accessibilityLabel={props.accessibilityLabel}
      class={`${FRAME} ${round} px-3 ${kind().frame} ${look().rings.visible} ${props.class ?? ""}`}
      style={kind().style}
      onPress={props.onPress}
    >
      <If when={props.icon}>{(icon) => <Icon name={icon()} tint={kind().tint} color={kind().color} />}</If>
      <Text class={`text-sm font-medium ${kind().label}`}>{props.label}</Text>
    </BaseButton>
  );
}

/** An `IconButton`'s look on a plain view, for a `PopupMenu` trigger, where a `Button` would take the press. */
export function IconTrigger(props: { icon: IconName; label: string; class?: string }) {
  const kind = () => kindLook("ghost");
  const round = corners();
  return (
    <View
      role="button"
      accessibilityLabel={props.label}
      tooltip={<Hint text={props.label} />}
      class={`${SHAPE} ${round} ${SIZES.regular.square} ${kind().frame} ${look().rings.visible} ${props.class ?? ""}`}
    >
      <Icon name={props.icon} size={SIZES.regular.icon} tint={kind().tint} color={kind().color} />
    </View>
  );
}

/** A square button showing only an icon; `label` is its tooltip, and `on` shows it active. */
export function IconButton(props: {
  icon: IconName;
  label: string;
  on?: boolean;
  disabled?: boolean;
  kind?: ButtonKind;
  tint?: IconTint;
  size?: keyof typeof SIZES;
  class?: string;
  onPress?: () => void;
  onPressDown?: () => void;
  onPressUp?: () => void;
}) {
  const kind = () => kindLook(props.kind ?? "ghost");
  const color = () => (props.tint ? undefined : props.on && props.kind !== "overlay" ? (look().onInk ?? kind().color) : kind().color);
  const round = corners();
  return (
    <BaseButton
      accessibilityLabel={props.label}
      tooltip={<Hint text={props.label} />}
      disabled={props.disabled}
      selected={props.on}
      class={`${SHAPE} ${round} ${SIZES[props.size ?? "regular"].square} ${kind().frame} ${look().rings.visible} ${props.class ?? ""}`}
      style={kind().style}
      selectedStyle={look().onStyle}
      onPress={props.onPress}
      onPressDown={props.onPressDown}
      onPressUp={props.onPressUp}
    >
      <Icon name={props.icon} size={SIZES[props.size ?? "regular"].icon} tint={props.tint ?? kind().tint} color={color()} />
    </BaseButton>
  );
}
