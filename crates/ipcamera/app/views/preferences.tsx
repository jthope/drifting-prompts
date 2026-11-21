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

type SettingsPart = "general" | "automation" | "appearance";

const settingsOpen = signal(false);

const section = signal<SettingsPart>("general");

/** Opens Settings on section `which`, else on the one it was left on. */
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

const MONO = "Cascadia Mono, Consolas, monospace";

/** Where one kind of file is saved; the videos' folder can't change while a camera records, as Rust insists too. */
function FolderRow(props: { kind: FolderKind; title: string; icon: IconName; help: string; first?: boolean }) {
  const folder = signal(readFolder(props.kind));
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
    Storage.open(props.kind).catch(failed);
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

function GeneralSection() {
  return (
    <Appear>
      <View class="gap-6">
        <SettingsGroup title="Storage" description="Where new snapshots and videos are saved. Those already saved stay where they are.">
          <FolderRow first kind="snapshots" title="Snapshots" icon="images" help="The camera button above a live picture saves one here." />
          <FolderRow kind="videos" title="Videos" icon="film" help="The record button saves each camera's recording here, with its thumbnail in a folder inside." />
        </SettingsGroup>
        <If when={Server.signedIn()}>
          <SettingsGroup title="Server" description="This server asks for its password, which ipcamera-server --set-password sets.">
            <SettingRow
              first
              icon="monitor"
              label="Signed in"
              detail="Signing out ends this page's session. The server asks for its password again."
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

function sections(appearance: KeptAppearance): SettingsSection<SettingsPart>[] {
  const automation: SettingsSection<SettingsPart>[] = Automation.available()
    ? [{ value: "automation", title: "Automation", icon: "bell", content: () => <AutomationSection /> }]
    : [];
  return [{ value: "general", title: "General", icon: "settings", content: () => <GeneralSection /> }, ...automation, appearanceSection(appearance)];
}

/** Settings, in its dialog over the window; `appearance` is the app's, which its Appearance section changes. */
export function AppSettings(props: { appearance: KeptAppearance }) {
  const all = sections(props.appearance);
  return <SettingsDialog open={settingsOpen()} onClose={() => settingsOpen.set(false)} sections={all} value={section()} onChange={(next) => section.set(next)} />;
}
