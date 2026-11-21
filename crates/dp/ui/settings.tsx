import { type JSX, Text } from "nsh";
import { AppearanceSettings, type KeptAppearance } from "./appearance";
import { Button } from "./button";
import { EditorDialog, type EditorSection } from "./editor";

/** A section of Settings; its `content` is made as it shows. */
export type SettingsSection<T extends string> = EditorSection<T> & { content: () => JSX.Element };

/** The Appearance section every product's Settings has. */
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

/** An app's Settings, in a dialog over the window while `open`. */
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
      {/* Made as it shows, so a section's fade-up plays each time it is chosen. */}
      <Each of={props.sections} key={(section) => section.value}>
        {(section) => <If when={section().value === props.value}>{section().content()}</If>}
      </Each>
    </EditorDialog>
  );
}
