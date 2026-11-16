// The card every card in the app is: the dashboard's numbers and charts, the
// groups of settings, a page header's banner. One look, so their corners and
// outlines match: rounded, a one-point border in the color the theme gives a
// card's outline (`Look.cardEdge`: none in Default, its `border` in a theme
// file), the theme's card color, and the elevation's shadow (`Look.surface`).
//
// The shadow goes on the rounded view itself, which draws it around its
// corners: on a view without them, such as a wrapper taking a share of a row,
// it would be drawn square, and its corners would show past the card's. A view
// that clips its children (`overflow-hidden`) still draws its own shadow whole,
// but a parent that clips cuts a child's off at its edge: a card in one needs
// `SHADOW_ROOM` around it.
import { type JSX, View, type ViewStyle } from "nsh";
import { look } from "./brand";

/** A card's classes: its corners, its one-point border in the theme's outline color (`Look.cardEdge`), and the theme's card color. */
export const cardClass = () => `rounded-xl border bg-card ${look().cardEdge}`;

/**
 * The farthest a card's shadow reaches past its edge, at any elevation: the
 * room a clipping parent leaves around a card so the shadow fades out before
 * the clip (`elevation.ts`, `surface`).
 */
export const SHADOW_ROOM = 12;

/**
 * A card: `cardClass()`, with the elevation's shadow and light on it. `class`
 * adds to it (padding, gaps, direction, a hover), `style` goes over it (a
 * position, a motion's opacity).
 */
export function Card(props: { class?: string; style?: ViewStyle; testID?: string; children?: JSX.Element }) {
  return (
    <View class={`${cardClass()} ${props.class ?? ""}`} style={{ ...look().surface, ...props.style }} testID={props.testID}>
      {props.children}
    </View>
  );
}
