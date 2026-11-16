// A search box for a page's bar: a text field as the forms draw one, with a
// magnifier at its start and, once something is typed, a button at its end
// that clears it.
import { View } from "nsh";
import { Input } from "nsh/base";
import { look } from "./brand";
import { IconButton } from "./button";
import { Icon } from "./icon";

/**
 * A search box. `label` is what it searches, for a screen reader ("Search
 * cameras"), and its placeholder; `value` is what is typed, which `onChange`
 * is told of at each key.
 */
export function SearchField(props: { label: string; value: string; class?: string; onChange: (value: string) => void }) {
  return (
    <View class={`h-8 justify-center ${props.class ?? ""}`}>
      <Input
        value={props.value}
        placeholder={props.label}
        autoCapitalize="none"
        autoCorrect={false}
        accessibilityLabel={props.label}
        // Room for the magnifier at the start and the clear button at the end.
        class={`h-8 rounded-md border pl-8 pr-8 text-sm ${look().field.frame} ${look().field.border} ${look().rings.field}`}
        style={look().well}
        onChange={({ value }) => props.onChange(value)}
      />
      <View class="absolute left-2.5 top-2" style={{ pointerEvents: "none" }}>
        <Icon name="search" size={16} tint="muted-foreground" />
      </View>
      <If when={props.value !== ""}>
        <IconButton icon="x" size="small" label="Clear search" class="absolute right-1 top-1" onPress={() => props.onChange("")} />
      </If>
    </View>
  );
}
