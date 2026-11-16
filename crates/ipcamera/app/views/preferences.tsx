// Settings, in a dialog over the window (`SettingsDialog`, which every product
// shares), whatever page shows behind it: its sections down the left, General,
// where the app saves snapshots and videos, Automation, what it does by itself
// (the desktop's alone), and Appearance, light, dark or as the system is, the
// texture and elevation over every theme, and a dark theme and a light one
// (`appearanceSection`, every product's), the chosen one's settings beside
// them. Settings are rows in cards, as the system's own Settings and nsh's gallery
// lay them out, and each is saved as it changes. The sidebar's Settings row,
// the dashboard's disk and the status bar's drive and media limit open it; the
// cameras keep their own settings dialog.
import { signal, Text, View } from "nsh";
import { Automation, Server, Storage, type StorageFolder } from "ipcamera/native";
import {
  Appear,
  appearanceSection,
  Button,
  type IconName,
  type KeptAppearance,
  notify,
  SettingRow,
  SettingsDialog,
  SettingsGroup,
  type SettingsSection,
} from "../../../dp/ui";
import { failed } from "./actions";
import { AutomationSection } from "./automation";
import { recording, storage } from "./model";

/** A section of Settings. */
type SettingsPart = "general" | "automation" | "appearance";

/** Whether Settings shows. */
const settingsOpen = signal(false);

/** The section shown. It outlives the dialog, so opening Settings again finds the section it was left on. */
const section = signal<SettingsPart>("general");

/** Opens Settings over the page, on the section `which` (as the status bar's drive and media limit do), else on the one it was left on. */
export function showSettings(which?: SettingsPart) {
  if (which) section.set(which);
  settingsOpen.set(true);
}

type FolderKind = "snapshots" | "videos";

function readFolder(kind: FolderKind): StorageFolder | undefined {
  try {
    return Storage.folder(kind);
  } catch (error) {
    failed(error);
    return undefined;
  }
}

/** Cascadia Mono, then Consolas where it is missing. */
const MONO = "Cascadia Mono, Consolas, monospace";

/**
 * Where one kind of file is saved, and the buttons that change it: the
 * system's folder dialog, the folder in Explorer, and back to the default.
 * The videos' folder stays put while a camera records, as Rust insists too.
 * On the web the folder is the server's, which a browser can neither open nor
 * choose, so it shows without the buttons.
 */
function FolderRow(props: { kind: FolderKind; title: string; icon: IconName; help: string; first?: boolean }) {
  const folder = signal(readFolder(props.kind));
  /** While the dialog is open, so a second press doesn't open another. */
  const choosing = signal(false);
  const locked = () => props.kind === "videos" && recording();
  const custom = () => folder()?.isDefault === false;
  const what = props.title.toLowerCase();

  const choose = () => {
    choosing.set(true);
    Storage.choose(props.kind)
      .then((next) => {
        if (next.path !== folder()?.path) notify(`New ${what} are saved in ${next.path}.`, "ok");
        folder.set(next);
      })
      .catch(failed)
      .finally(() => choosing.set(false));
  };
  const reset = () => {
    try {
      folder.set(Storage.reset(props.kind));
      notify(`New ${what} are saved in the default folder again.`, "ok");
    } catch (error) {
      failed(error);
    }
  };
  const open = () => {
    try {
      Storage.open(props.kind);
    } catch (error) {
      failed(error);
    }
  };

  return (
    <SettingRow
      first={props.first}
      icon={props.icon}
      label={props.title}
      tag={folder()?.isDefault ? "Default" : undefined}
      detail={locked() ? "Stop recording to change where videos are saved." : props.help}
      control={
        <>
          <If when={storage.canChooseFolder}>
            <Button label="Change…" accessibilityLabel={`Change where ${what} are saved`} disabled={choosing() || locked()} onPress={choose} />
          </If>
          <If when={storage.canOpenFolder}>
            <Button icon="folder-open" label="Open" accessibilityLabel={`Open the ${what} folder`} onPress={open} />
          </If>
          <If when={custom() && storage.canChooseFolder}>
            <Button label="Use default" accessibilityLabel={`Save ${what} in the default folder`} disabled={choosing() || locked()} onPress={reset} />
          </If>
        </>
      }
    >
      <Text class="rounded-md bg-muted px-2.5 py-1.5 text-xs text-muted-foreground" style={{ fontFamily: MONO, numberOfLines: 1, textOverflow: "ellipsis-middle" }}>
        {folder()?.path ?? ""}
      </Text>
    </SettingRow>
  );
}

/** General: where new snapshots and videos are saved, and on a server with a password, signing out. */
function GeneralSection() {
  return (
    <Appear>
      <View class="gap-6">
        <SettingsGroup title="Storage" description="Where new snapshots and videos are saved. Those already saved stay where they are.">
          <FolderRow first kind="snapshots" title="Snapshots" icon="images" help="The camera button above a live picture saves one here." />
          <FolderRow kind="videos" title="Videos" icon="film" help="The record button saves each camera's recording here, with its thumbnail in a folder inside." />
        </SettingsGroup>
        {/* Only where someone signed in: a server with a password, on the web. */}
        <If when={Server.signedIn()}>
          <SettingsGroup title="Server" description="This server asks for its password, which ipcamera-server --set-password sets.">
            <SettingRow
              first
              icon="monitor"
              label="Signed in"
              detail="Signing out ends this page's session. The server asks for its password again."
              // Signed out, the sign-in page takes the window, and Settings closes over it.
              control={
                <Button
                  label="Sign out"
                  onPress={() =>
                    Server.signOut()
                      .then(() => settingsOpen.set(false))
                      .catch((error) => notify(String(error), "warn"))
                  }
                />
              }
            />
          </SettingsGroup>
        </If>
      </View>
    </Appear>
  );
}

/** The sections, with Automation only where the app does anything by itself: the desktop. */
function sections(appearance: KeptAppearance): SettingsSection<SettingsPart>[] {
  const automation: SettingsSection<SettingsPart>[] = Automation.available()
    ? [{ value: "automation", title: "Automation", icon: "bell", content: () => <AutomationSection /> }]
    : [];
  return [{ value: "general", title: "General", icon: "settings", content: () => <GeneralSection /> }, ...automation, appearanceSection(appearance)];
}

/**
 * Settings, in its dialog over the window, which `App.tsx` places once.
 * `appearance` is the app's (`keepAppearance`'s), which the Appearance section
 * shows and changes, saving it with the other preferences.
 */
export function AppSettings(props: { appearance: KeptAppearance }) {
  const all = sections(props.appearance);
  return <SettingsDialog open={settingsOpen()} onClose={() => settingsOpen.set(false)} sections={all} value={section()} onChange={(next) => section.set(next)} />;
}
