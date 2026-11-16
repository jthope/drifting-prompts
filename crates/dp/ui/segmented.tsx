import { Text } from "nsh";
import { Toggle, ToggleGroup } from "nsh/base";
import { look } from "./brand";
import { Icon, type IconName } from "./icon";

/**
 * One choice of a few, all in view: a row of segments on a muted track, the
 * chosen one in the theme's fill, outline and ink, as the system's own
 * Settings shows light, dark and system. The segments are toggles in a group, so the arrow
 * keys move between them and a screen reader hears which is on. Put it beside
 * a label that says what it chooses: nsh 0.5.1's group takes no name of its own.
 * An item's `label` is what a screen reader says for it, when its title alone
 * would say the same as another control's ("None").
 *
 * It is 32 points tall in every theme, a regular button's height, and the
 * segments fill what the track leaves them: a theme file outlines the track,
 * and that outline goes inside the 32 points rather than adding to them. A
 * title bar holding it then stays as tall as the sidebar's top row beside it.
 */
export function Segmented<T extends string>(props: {
  items: { value: T; title: string; label?: string; icon?: IconName }[];
  value: T;
  onChange: (value: T) => void;
}) {
  const on = (value: T) => props.value === value;
  const toggle = () => look().toggle;
  return (
    <ToggleGroup class={`h-8 flex-row items-stretch gap-0.5 rounded-lg p-0.5 ${toggle().track}`} style={look().well}>
      <Each of={props.items} key={(item) => item.value}>
        {(item) => (
          <Toggle
            pressed={on(item().value)}
            accessibilityLabel={item().label ?? item().title}
            class={`flex-row items-center gap-1.5 rounded-md px-2.5 ${toggle().frame} ${look().rings.visible}`}
            style={on(item().value) ? look().segment : {}}
            // The chosen segment stays chosen: pressing it again changes nothing.
            onChange={() => props.onChange(item().value)}
          >
            <If when={item().icon}>
              {(icon) => (
                <Icon
                  name={icon()}
                  size={14}
                  tint={on(item().value) ? "foreground" : "muted-foreground"}
                  color={on(item().value) ? look().onInk : undefined}
                />
              )}
            </If>
            <Text class={`text-sm ${on(item().value) ? toggle().on : toggle().off}`}>{item().title}</Text>
          </Toggle>
        )}
      </Each>
    </ToggleGroup>
  );
}
