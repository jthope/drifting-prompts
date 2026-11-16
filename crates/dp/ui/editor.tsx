// The dialog the editors open in: a camera's settings, several cameras'
// settings at once, a group's name and cameras, and the app's Settings
// (`settings.tsx`). It covers the window, dimmed behind it, at one size
// whatever it holds, so moving between its sections never changes its size: an
// editor's form, or wider for rows of settings (`size`). Its header has the
// badge, the title and the close button over a line, in every theme, then its
// body, and its footer with the buttons. A form of several parts lists them down its left side
// (`sections`), in the sidebar's look, and shows the chosen one's fields beside
// them; a body with no sections is the page's own layout, such as two lists
// side by side that each scroll.
import { type JSX, keymap, Scroll, View } from "nsh";
import { Dialog } from "nsh/base";
import { EdgeLine } from "./brand";
import { IconButton } from "./button";
import type { IconName } from "./icon";
import { MenuLayer, popupSurface } from "./menu";
import { type NavTab, NavTabs } from "./nav";
import { PanelHeader } from "./panel";

/**
 * Escape closes the dialog from a text field too, bound while it shows, as the
 * snapshot viewer and the video player bind theirs: the dialog's own Escape
 * comes only when no field has the keyboard, which takes the key first. A
 * list open in the dialog still takes it before this does. A release whose
 * `Dialog` closes on Escape from a text field, as `Sheet` does, replaces it.
 */
function EscapeCloses(props: { onClose: () => void }) {
  keymap.bind("escape", () => props.onClose(), { when: "always" });
  return null;
}

/** A part of an `EditorDialog`'s form: its value, its name and icon, and whether a field in it has a problem. */
export type EditorSection<T extends string> = NavTab<T>;

/**
 * How large an `EditorDialog` is, smaller only in a smaller window: `form`, an
 * editor's fields (720 × 540, the default); `wide`, rows of settings with each
 * control beside its name and the theme tiles four to a row, with room to
 * spare (1000 × 640).
 */
export type EditorSize = "form" | "wide";

/** Each size's classes, written out whole so the bundler sees them. */
const SIZES: Record<EditorSize, string> = {
  form: "h-[540px] w-[720px]",
  wide: "h-[640px] w-[1000px]",
};

/**
 * A dialog for editing something, in the middle of the window over a dimmed
 * backdrop, as large as `size` says. With `sections`, its parts are listed down
 * its left side and `children` are the chosen part's fields, which scroll;
 * without, `children` fill the body as the page lays them out. `footer` holds
 * its buttons, at the end, and anything before them (why a save failed).
 * `action` is a button of the dialog's own in its header, before the one that
 * closes it. Escape, the backdrop and the close button call `onClose`.
 */
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
  // One size whatever the dialog shows, smaller only in a smaller window.
  const surface = () => popupSurface(`${SIZES[props.size ?? "form"]} max-h-full max-w-full overflow-hidden`);
  return (
    <Dialog
      open={props.open}
      animation="fade"
      class="flex-1 items-center justify-center p-6"
      // The view that centers the panel lets presses beside it through to the backdrop, which closes it.
      style={{ pointerEvents: "box-none" }}
      backdropClass="bg-black/40"
      onOpenChange={(event) => !event.open && props.onClose()}
    >
      <EscapeCloses onClose={props.onClose} />
      <View class={surface().class} style={surface().style}>
        {/* Ruled under its header, as its footer is over its buttons, though the page's header behind draws no line in some themes. */}
        <PanelHeader title={props.title} icon={props.icon} ruled action={
            <>
              {props.action}
              <IconButton icon="x" label="Close" onPress={props.onClose} />
            </>
          }
        />
        {/* Read through `props` rather than handed down once, so choosing a section updates the rail in place and its keyboard focus stays. */}
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
