import { createMotion, type JSX, View } from "nsh";

/** Fades its content up into place when first shown, `index` steps after the first; none with reduced motion. */
export function Appear(props: { index?: number; class?: string; children: JSX.Element }) {
  const motion = createMotion({ enter: "fadeInUp", delay: Math.min(props.index ?? 0, 8) * 60 });
  return (
    <View class={props.class} style={motion.style}>
      {props.children}
    </View>
  );
}
