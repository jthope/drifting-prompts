import { computed, effect, signal, Text, untracked } from "nsh";
import { type CameraChanges, Cameras } from "ipcamera/native";
import { Button, CheckField, EditorDialog, FieldRow, FormSection, IconButton, notify, SelectField, TextField } from "../../../dp/ui";
import { bulkEditing } from "./actions";
import { refresh } from "./model";
import { changesToSave, commonOf, reload, type SharedField, saved } from "./roster";
import { CAMERA_SECTIONS, type CameraSection, markProblems } from "./settings";

type TextName = { [K in SharedField]: CameraChanges[K] extends string | null | undefined ? K : never }[SharedField];
type Switch = "useTcp" | "autoReconnect";

const KEEP = "keep";

const SECTIONS = CAMERA_SECTIONS.filter((section) => section.value !== "features" && section.value !== "events");

const STREAMS = [
  { value: "0", title: "Main stream (best quality, more bandwidth)" },
  { value: "1", title: "Sub stream (lighter, lower latency)" },
];

function namesOf(names: string[]): string {
  if (names.length <= 3) return names.length < 2 ? (names[0] ?? "") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
  return `${names.slice(0, 2).join(", ")} and ${names.length - 2} others`;
}

export function BulkDialog() {
  const edits = signal<CameraChanges>({});
  const saving = signal(false);
  const failure = signal("");

  /** Kept after the dialog closes, until it opens for others. */
  const editingIds = signal<string[]>([]);
  const cameras = computed(() => {
    const ids = new Set(editingIds());
    return saved().filter((each) => ids.has(each.id));
  });
  const count = () => cameras().length;
  const common = computed(() => commonOf(cameras()));
  const changes = computed(() => changesToSave(edits(), common()));
  const problems = computed(() => Cameras.changeProblems(changes()));
  const problem = (field: string) => problems().find((found) => found.field === field)?.message;
  const nothing = () => Object.keys(changes()).length === 0;
  const section = signal<CameraSection>("connection");
  const sections = computed(() => markProblems(SECTIONS, problems()));

  // Each opening starts with nothing changed, which is what makes closing discard the edits.
  effect(() => {
    const ids = bulkEditing();
    if (ids === null) return;
    untracked(() => {
      editingIds.set(ids);
      edits.set({});
      failure.set("");
      section.set("connection");
    });
  });

  const close = () => bulkEditing.set(null);

  const save = () => {
    if (saving() || nothing()) return;
    if (problems().length > 0) {
      const unfixed = sections().first;
      if (unfixed) section.set(unfixed);
      return;
    }
    failure.set("");
    saving.set(true);
    Cameras.saveChanges(
      cameras().map((each) => each.id),
      changes(),
    )
      .then((changed) => {
        refresh();
        reload();
        close();
        notify(changed === 1 ? "Saved 1 camera." : `Saved ${changed.toLocaleString()} cameras.`, "ok");
      })
      .catch((error) => failure.set(String(error?.message ?? error)))
      .finally(() => saving.set(false));
  };

  const edited = (field: keyof CameraChanges) => edits()[field] !== undefined;
  const set = <K extends keyof CameraChanges>(field: K, value: CameraChanges[K]) => edits.set({ ...edits(), [field]: value });
  const keep = (field: keyof CameraChanges) => {
    const { [field]: _, ...rest } = edits();
    edits.set(rest);
  };
  const Undo = (undo: { field: keyof CameraChanges; label: string }) => (
    <If when={edited(undo.field)}>
      <IconButton icon="undo-2" size="small" label={`Keep each camera's ${undo.label.toLowerCase()}`} onPress={() => keep(undo.field)} />
    </If>
  );

  const Entry = (entry: { label: string; name: TextName; numeric?: boolean; wide?: boolean }) => {
    const differs = () => common()[entry.name] === undefined;
    return (
      <TextField
        label={entry.label}
        value={edits()[entry.name] ?? common()[entry.name] ?? ""}
        placeholder={differs() ? "Mixed" : undefined}
        help={differs() && !edited(entry.name) ? "They differ: each keeps its own." : undefined}
        error={problem(entry.name)}
        numeric={entry.numeric}
        action={<Undo field={entry.name} label={entry.label} />}
        class={entry.wide ? undefined : "min-w-0 flex-1"}
        onChange={(value) => (differs() && value === "" ? keep(entry.name) : set(entry.name, value))}
        onSubmit={save}
      />
    );
  };

  const Toggle = (toggle: { label: string; name: Switch }) => (
    <CheckField
      label={toggle.label}
      checked={edits()[toggle.name] ?? common()[toggle.name] ?? false}
      mixed={edits()[toggle.name] === undefined && common()[toggle.name] === undefined}
      action={<Undo field={toggle.name} label={toggle.label} />}
      onChange={(on) => set(toggle.name, on)}
    />
  );

  const streams = () => (common().subStream === undefined ? [{ value: KEEP, title: "Mixed: each keeps its own" }, ...STREAMS] : STREAMS);
  const stream = () => {
    const chosen = edits().subStream ?? common().subStream;
    return chosen === undefined || chosen === null ? KEEP : String(chosen);
  };

  return (
    <EditorDialog
      title={`Edit ${count().toLocaleString()} cameras`}
      icon="video"
      open={bulkEditing() !== null}
      onClose={close}
      sections={{ items: sections().items, value: section(), onChange: (value) => section.set(value) }}
      footer={
        <>
          <If when={failure()}>
            <Text class="min-w-0 flex-1 text-xs text-destructive">{failure()}</Text>
          </If>
          <Button label="Cancel" onPress={close} />
          <Button
            kind="primary"
            label={saving() ? "Saving…" : `Apply to ${count().toLocaleString()}`}
            accessibilityLabel={`Apply the changes to ${count().toLocaleString()} cameras`}
            disabled={saving() || problems().length > 0 || nothing()}
            onPress={save}
          />
        </>
      }
    >
      <Text class="text-sm text-muted-foreground">
        {`What you change here goes to ${namesOf(cameras().map((each) => each.name))}. Names and channels stay each camera's own.`}
      </Text>

      <If when={section() === "connection"}>
        <FormSection title="Connection">
          <Entry label="Camera address / hostname" name="host" wide />
          <FieldRow>
            <Entry label="Username" name="user" />
            <TextField
              label="Password"
              value={edits().password ?? ""}
              placeholder="Unchanged"
              help="Type one to give them all the same."
              masked
              action={<Undo field="password" label="Password" />}
              class="min-w-0 flex-1"
              onChange={(value) => (value === "" ? keep("password") : set("password", value))}
              onSubmit={save}
            />
          </FieldRow>
        </FormSection>
      </If>

      <If when={section() === "stream"}>
        <FormSection title="Stream">
          <Entry label="RTSP port" name="rtspPort" numeric wide />
          <SelectField
            label="Stream quality"
            items={streams()}
            value={stream()}
            onChange={(value) => (value === KEEP ? keep("subStream") : set("subStream", Number(value)))}
          />
          <Entry label="RTSP path override" name="rtspPath" wide />
          <Toggle label="Force RTSP over TCP (recommended)" name="useTcp" />
          <Toggle label="Reconnect automatically" name="autoReconnect" />
        </FormSection>
      </If>

      <If when={section() === "ptz"}>
        <FormSection title="PTZ">
          <FieldRow>
            <Entry label="ONVIF port" name="onvifPort" numeric />
            <Entry label="DVRIP port" name="dvripPort" numeric />
          </FieldRow>
          <Entry label="ONVIF device path" name="onvifPath" wide />
        </FormSection>
      </If>
    </EditorDialog>
  );
}
