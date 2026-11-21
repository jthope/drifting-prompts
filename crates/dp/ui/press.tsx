import { createAnimatedValue, type JSX, View, type ViewStyle } from "nsh";

const SPRING = { stiffness: 520, damping: 34 };

type Point = { x: number; y: number };

/** Something large to press, which sinks a little while held. */
export function Press(props: {
  label: string;
  scale?: number;
  class?: string;
  /** A style its look needs (a card's shadow), under its padding and motion. */
  style?: ViewStyle;
  padding?: number;
  /** What `nsh test` finds it by, as `id:<testID>`. */
  testID?: string;
  selected?: boolean;
  /** With the keys held, for a press that picks as Explorer does. */
  onPress?: (event: { clickCount: number; modifiers: { shift: boolean; control: boolean; platform: boolean } }) => void;
  onContextMenu?: (event: { pagePosition: Point }) => void;
  children: JSX.Element;
}) {
  const scale = createAnimatedValue(1);
  return (
    <View
      role="button"
      accessibilityLabel={props.label}
      accessibilityState={props.selected === undefined ? undefined : { selected: props.selected }}
      testID={props.testID}
      class={props.class}
      onPress={props.onPress}
      onContextMenu={props.onContextMenu}
      onPressDown={() => scale.spring(props.scale ?? 0.97, SPRING)}
      onPressUp={() => scale.spring(1, SPRING)}
      style={{ ...props.style, padding: props.padding, transform: [{ scale }] }}
    >
      {props.children}
    </View>
  );
}
