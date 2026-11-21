import { Text } from "nsh";
import { Toggle, ToggleGroup } from "nsh/base";
import { look } from "./brand";
import { Icon, type IconName } from "./icon";

/** One choice of a few, 32 points tall in every theme; nsh's toggle group takes no name, so a label beside it says what it chooses. */
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
