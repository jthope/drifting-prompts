import { colorScheme, Image } from "nsh";
import { LOGOS, type Product } from "./logos";

/** `product`'s logo, dark on light and white on dark whatever the theme; its markup ships in the bundle. */
export function Logo(props: { product: Product; size?: number }) {
  const size = () => props.size ?? 32;
  const logo = () => LOGOS[props.product];
  const src = () => (colorScheme() === "dark" ? logo().inverse : logo().ink);
  return <Image src={src()} accessibilityLabel={logo().label} fadeDuration={0} style={{ width: size(), height: size() }} />;
}
