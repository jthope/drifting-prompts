import { type JSX, keymap, Scroll, View } from "nsh";
import { Dialog } from "nsh/base";
import { EdgeLine } from "./brand";
import { IconButton } from "./button";
import type { IconName } from "./icon";
import { MenuLayer, popupSurface } from "./menu";
import { type NavTab, NavTabs } from "./nav";
import { PanelHeader } from "./panel";

/** nsh's `Dialog` takes no Escape from a text field, so this binds it while the dialog shows. */
function EscapeCloses(props: { onClose: () => void }) {
  keymap.bind("escape", () => props.onClose(), { when: "always" });
  return null;
}

/** A part of an `EditorDialog`'s form. */
export type EditorSection<T extends string> = NavTab<T>;

/** An `EditorDialog`'s size, smaller only in a smaller window: `form` (the default) or `wide`. */
export type EditorSize = "form" | "wide";

/** Each size's classes, written out whole so the bundler sees them. */
const SIZES: Record<EditorSize, string> = {
  form: "h-[540px] w-[720px]",
  wide: "h-[640px] w-[1000px]",
};

/** A dialog for editing something; with `sections`, its parts are listed down its left side. */
export function EditorDialog<T extends string>(props: {
  title: string;
  icon: IconName;
  open: boolean;
  onClose: () => void;
  size?: EditorSize;
  sections?: { items: readonly EditorSection<T>[]; value: T; onChange: (value: T) => void };
  action?: JSX.Element;
  footer: JSX.Element;
  children: JSX.Element;
}) {
  const surface = () => popupSurface(`${SIZES[props.size ?? "form"]} max-h-full max-w-full overflow-hidden`);
  return (
    <Dialog
      open={props.open}
      animation="fade"
      class="flex-1 items-center justify-center p-6"
      // Presses beside the panel reach the backdrop, which closes it.
      style={{ pointerEvents: "box-none" }}
      backdropClass="bg-black/40"
      onOpenChange={(event) => !event.open && props.onClose()}
    >
      <EscapeCloses onClose={props.onClose} />
      <View class={surface().class} style={surface().style}>
        <PanelHeader title={props.title} icon={props.icon} ruled action={
            <>
              {props.action}
              <IconButton icon="x" label="Close" onPress={props.onClose} />
            </>
          }
        />
        {/* Read through `props`, so choosing a section keeps the rail and its keyboard focus. */}
        <If when={props.sections !== undefined} else={<View class="min-h-0 flex-1 p-4">{props.children}</View>}>
          <View class="min-h-0 flex-1 flex-row">
            <View class="w-44 bg-sidebar p-2">
              <NavTabs items={props.sections?.items ?? []} value={props.sections?.value as T} onChange={(value) => props.sections?.onChange(value)} />
              <EdgeLine edge="right" place="sidebar" />
            </View>
            <Scroll style={{ flex: 1 }}>
              <View class="gap-6 p-5">{props.children}</View>
            </Scroll>
          </View>
        </If>
        <View class="flex-row items-center justify-end gap-2 border-t border-border p-3">{props.footer}</View>
      </View>
      <MenuLayer />
    </Dialog>
  );
}
