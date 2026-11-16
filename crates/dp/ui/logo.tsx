import { colorScheme, Image } from "nsh";
import { LOGOS, type Product } from "./logos";

/**
 * `product`'s logo, 32 points by default: the mark in one color, dark on light
 * and white on dark, whatever the theme. Its markup is in the code
 * (`logos.ts`), so it ships inside the bundle. It appears at once rather than
 * fading in like a loaded picture.
 */
export function Logo(props: { product: Product; size?: number }) {
  const size = () => props.size ?? 32;
  const logo = () => LOGOS[props.product];
  const src = () => (colorScheme() === "dark" ? logo().inverse : logo().ink);
  return <Image src={src()} accessibilityLabel={logo().label} fadeDuration={0} style={{ width: size(), height: size() }} />;
}
