import { createAnimatedValue, type JSX, View, type ViewStyle } from "nsh";

/** A quick spring that barely overshoots, so a press reads as a press and not a wobble. */
const SPRING = { stiffness: 520, damping: 34 };

type Point = { x: number; y: number };

/**
 * Something large to press, such as a picture in the gallery. It sinks a little
 * while held and springs back, as the tiles of nsh's gallery example do.
 * `class` brings its look, a hover fill included; `label` is what a screen
 * reader says.
 */
export function Press(props: {
  label: string;
  /** How far it shrinks while held; 0.97 unless given. */
  scale?: number;
  class?: string;
  /** What its look needs a style for (a card's shadow), under its own padding and motion. */
  style?: ViewStyle;
  /** Room around its content, in points, for a look that needs an exact number. */
  padding?: number;
  /** What `nsh test` finds it by, as `id:<testID>`. */
  testID?: string;
  /** Whether it is the one chosen of several, as a screen reader says it. */
  selected?: boolean;
  /** With the keys held, as a press of a view has them, for a press that picks as Explorer does. */
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
