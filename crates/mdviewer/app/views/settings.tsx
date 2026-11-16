// Settings, in a dialog over the window, as IP Camera's (`SettingsDialog`,
// which every product shares): Files, how a file opens in a new tab and what
// the tab strip does with more tabs than fit, and Appearance, the settings
// every product shares (`appearanceSection`). Each is saved with the other
// preferences as it changes. The sidebar's Settings row opens it over the
// project or New, which stay behind it.
import { View } from "nsh";
import type { TabOverflow, ViewMode } from "mdviewer/native";
import { Appear, appearanceSection, type KeptAppearance, Segmented, Setting, SettingsDialog, SettingsGroup, SettingsPanel, type SettingsSection } from "../../../dp/ui";
import { MODES } from "./header";
import { defaultMode, type SettingsPart, settingsOpen, settingsSection, tabOverflow } from "./model";

/** What each way a new tab can show a file does. */
const OPEN_AS: Record<ViewMode, string> = {
  raw: "As written, highlighted, a fold at each heading.",
  split: "As written on the left, the page it makes on the right.",
  rendered: "The page the Markdown makes.",
};

/** The ways the tab strip can hold more tabs than fit in one row, and what each does. */
const OVERFLOWS: { value: TabOverflow; title: string; detail: string }[] = [
  { value: "scroll", title: "Scroll", detail: "One row, which scrolls sideways." },
  { value: "wrap", title: "Wrap", detail: "As many rows as the tabs take. A tab is dragged along its own row." },
];

/** The Tabs settings: what the tab strip does with more tabs than fit in one row, saved with the other preferences. */
function TabSettings() {
  const shown = () => OVERFLOWS.find((each) => each.value === tabOverflow()) ?? OVERFLOWS[0];
  return (
    <SettingsGroup title="Tabs" description="What the tab strip does with more tabs than fit in one row.">
      <SettingsPanel>
        <Setting
          icon="panel-top"
          label="Tabs that don't fit"
          detail={shown().detail}
          control={<Segmented items={OVERFLOWS} value={tabOverflow()} onChange={(next) => tabOverflow.set(next)} />}
        />
      </SettingsPanel>
    </SettingsGroup>
  );
}

/** Files: how a file opened in a new tab shows (`defaultMode`), each tab keeping its own after that, and the Tabs settings. */
function FilesSection() {
  return (
    <Appear>
      <View class="gap-6">
        <SettingsGroup title="Files" description="How a file shows when it opens in a new tab. Each tab keeps its own after that.">
          <SettingsPanel>
            <Setting
              icon="file-text"
              label="Open files as"
              detail={OPEN_AS[defaultMode()]}
              control={<Segmented items={MODES} value={defaultMode()} onChange={(next) => defaultMode.set(next)} />}
            />
          </SettingsPanel>
        </SettingsGroup>
        <TabSettings />
      </View>
    </Appear>
  );
}

/**
 * Settings, in its dialog over the window, which `App.tsx` places once.
 * `appearance` is the app's (`keepAppearance`'s), which the Appearance section
 * shows and changes, saving it with the other preferences.
 */
export function AppSettings(props: { appearance: KeptAppearance }) {
  const sections: SettingsSection<SettingsPart>[] = [
    { value: "files", title: "Files", icon: "file-text", content: () => <FilesSection /> },
    appearanceSection(props.appearance),
  ];
  return (
    <SettingsDialog open={settingsOpen()} onClose={() => settingsOpen.set(false)} sections={sections} value={settingsSection()} onChange={(next) => settingsSection.set(next)} />
  );
}
