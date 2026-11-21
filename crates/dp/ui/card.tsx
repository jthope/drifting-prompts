import { type JSX, View, type ViewStyle } from "nsh";
import { look } from "./brand";

/** A card's classes: its corners, its border and its color. */
export const cardClass = () => `rounded-xl border bg-card ${look().cardEdge}`;

/** How far a card's shadow reaches past its edge: the room a clipping parent leaves around a card. */
export const SHADOW_ROOM = 12;

/** A card: `cardClass()` with the elevation's shadow and light. */
export function Card(props: { class?: string; style?: ViewStyle; testID?: string; children?: JSX.Element }) {
  return (
    <View class={`${cardClass()} ${props.class ?? ""}`} style={{ ...look().surface, ...props.style }} testID={props.testID}>
      {props.children}
    </View>
  );
}
