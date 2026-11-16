// An app's Settings, in a dialog over the window, the same in every product:
// the editors' dialog (`editor.tsx`) at its wide size, its sections down the
// left side in the sidebar's look, each with its icon, and the chosen one's
// settings scrolling beside them, rows in cards (`rows.tsx`). Each setting
// applies and is saved as it changes, so the footer has nothing to save: it
// says so, beside Done. Escape, the backdrop, the close button and Done close
// it, and the page behind it stays as it was. The app keeps which section shows,
// so opening Settings again finds the one it was left on, and places the dialog
// once, over everything but the toasts and the menus. The Appearance section is
// every product's (`appearanceSection`), so each app lists it and writes only
// its own.
import { type JSX, Text } from "nsh";
import { AppearanceSettings, type KeptAppearance } from "./appearance";
import { Button } from "./button";
import { EditorDialog, type EditorSection } from "./editor";

/** A section of Settings: its value, its name and icon down the dialog's side, and its settings (`content`), made as it shows. */
export type SettingsSection<T extends string> = EditorSection<T> & { content: () => JSX.Element };

/**
 * The Appearance section every product's Settings has: the mode, the texture,
 * the elevation and the themes (`AppearanceSettings`), as `appearance`, the
 * app's (`keepAppearance`'s), shows them and keeps each pick.
 */
export function appearanceSection(appearance: KeptAppearance): SettingsSection<"appearance"> {
  return {
    value: "appearance",
    title: "Appearance",
    icon: "palette",
    content: () => (
      <AppearanceSettings
        mode={appearance.mode()}
        onMode={appearance.setMode}
        onTheme={appearance.pickTheme}
        onDefault={appearance.pickDefault}
        onTexture={appearance.setTexture}
        onElevation={appearance.setElevation}
      />
    ),
  };
}

/**
 * An app's Settings, in a dialog over the window while `open`: `sections` down
 * its left side, `value` the one shown, which `onChange` changes. Escape, the
 * backdrop, the close button and Done call `onClose`.
 */
export function SettingsDialog<T extends string>(props: {
  open: boolean;
  onClose: () => void;
  sections: readonly SettingsSection<T>[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <EditorDialog
      title="Settings"
      icon="settings"
      size="wide"
      open={props.open}
      onClose={props.onClose}
      sections={{ items: props.sections, value: props.value, onChange: props.onChange }}
      footer={
        <>
          <Text class="min-w-0 flex-1 text-xs text-muted-foreground">Changes take effect and are saved as you make them.</Text>
          <Button kind="primary" label="Done" onPress={props.onClose} />
        </>
      }
    >
      {/* Each section's settings are made as it shows, so a section's own fade-up plays each time it is chosen. */}
      <Each of={props.sections} key={(section) => section.value}>
        {(section) => <If when={section().value === props.value}>{section().content()}</If>}
      </Each>
    </EditorDialog>
  );
}
