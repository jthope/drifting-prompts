import { createMotion, type JSX, View } from "nsh";

/**
 * Fades its content up into place when it is first shown, `index` steps of
 * 60 ms after the first, as nsh's gallery brings in a page's cards one after
 * another. It runs natively, and with reduced motion the content is simply there.
 */
export function Appear(props: { index?: number; class?: string; children: JSX.Element }) {
  const motion = createMotion({ enter: "fadeInUp", delay: Math.min(props.index ?? 0, 8) * 60 });
  return (
    <View class={props.class} style={motion.style}>
      {props.children}
    </View>
  );
}
