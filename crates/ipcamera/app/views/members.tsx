import { computed, effect, type JSX, Scroll, signal, Text, untracked, View } from "nsh";
import { type CameraState, Groups } from "ipcamera/native";
import { Button, EditorDialog, IconButton, look, SearchField, StatusDot, TextField } from "../../../dp/ui";
import { groupAdded, groupEditing } from "./actions";
import { countLabel } from "./library";
import { groups, refresh, states } from "./model";

const cameraCount = (count: number) => (count === 1 ? "1 camera" : `${countLabel(count)} cameras`);

function CameraLine(props: { camera: CameraState; children: JSX.Element }) {
  return (
    <View class={`h-9 flex-row items-center gap-2.5 rounded-md pl-2.5 pr-1 ${look().listRow.hover}`}>
      <StatusDot tone={props.camera.tone} />
      <Text class="min-w-0 flex-1 text-sm text-foreground" style={{ numberOfLines: 1 }}>
        {props.camera.name}
      </Text>
      <View class="flex-row items-center gap-0.5">{props.children}</View>
    </View>
  );
}

function Pane(props: { title: string; note?: string; search?: JSX.Element; children: JSX.Element }) {
  return (
    <View class="min-w-0 flex-1 gap-2 pt-3">
      <View class="gap-2 px-3">
        <Text role="header" class="text-sm font-semibold text-foreground">
          {props.title}
        </Text>
        <If when={props.note}>
          <Text class="text-xs text-muted-foreground">{props.note}</Text>
        </If>
        {props.search}
      </View>
      <Scroll style={{ flex: 1 }}>
        <View class="gap-0.5 px-1.5 pb-3">{props.children}</View>
      </Scroll>
    </View>
  );
}

export function GroupDialog() {
  const name = signal("");
  const members = signal<string[]>([]);
  const search = signal("");
  const failure = signal("");
  const adding = () => groupEditing()?.group === null;

  // Each opening starts from what is saved, which is what makes closing discard the edits.
  effect(() => {
    const asked = groupEditing();
    if (!asked) return;
    untracked(() => {
      const group = asked.group === null ? undefined : groups().find((each) => each.id === asked.group);
      name.set(group?.name ?? "");
      members.set(group?.cameras ?? []);
      search.set("");
      failure.set("");
    });
  });

  /** The group's cameras that still exist: one deleted while the dialog is open drops out. */
  const inGroup = computed(() => members().flatMap((id) => states().find((camera) => camera.id === id) ?? []));
  const others = computed(() => {
    const words = search().toLowerCase().split(/\s+/).filter((word) => word !== "");
    return states().filter((camera) => !members().includes(camera.id) && words.every((word) => camera.name.toLowerCase().includes(word)));
  });
  const outside = () => states().filter((camera) => !members().includes(camera.id)).length;
  const blankName = () => name().trim() === "";

  const close = () => groupEditing.set(null);

  // A second press while saving is ignored rather than saved twice.
  let saving = false;
  const save = async () => {
    const asked = untracked(groupEditing);
    if (!asked || blankName() || saving) return;
    failure.set("");
    const cameras = inGroup().map((camera) => camera.id);
    saving = true;
    try {
      if (asked.group === null) {
        const id = await Groups.add(name(), cameras);
        close();
        groupAdded(id);
      } else {
        await Groups.save(asked.group, name(), cameras);
        close();
        refresh();
      }
    } catch (error) {
      failure.set(String((error as { message?: string })?.message ?? error));
    } finally {
      saving = false;
    }
  };

  const move = (id: string, step: number) => {
    const list = inGroup().map((camera) => camera.id);
    const from = list.indexOf(id);
    const to = from + step;
    if (from < 0 || to < 0 || to >= list.length) return;
    list.splice(from, 1);
    list.splice(to, 0, id);
    members.set(list);
  };
  const takeOut = (id: string) => members.set(members().filter((each) => each !== id));
  const putIn = (id: string) => members.set([...members(), id]);

  const placed = () => inGroup().map((camera, index) => ({ camera, index }));

  return (
    <EditorDialog
      title={adding() ? "New group" : "Edit group"}
      icon="folder"
      open={groupEditing() !== null}
      onClose={close}
      footer={
        <>
          <If when={failure()}>
            <Text class="min-w-0 flex-1 text-xs text-destructive">{failure()}</Text>
          </If>
          <Button label="Cancel" onPress={close} />
          <Button kind="primary" label={adding() ? "Create" : "Save"} disabled={blankName()} onPress={save} />
        </>
      }
    >
      <View class="flex-1 gap-4">
        <TextField
          label="Name"
          value={name()}
          help="The sidebar lists the group's cameras under it."
          autoFocus={adding()}
          onChange={(value) => name.set(value)}
          onSubmit={save}
        />

        <View class="min-h-0 flex-1 flex-row overflow-hidden rounded-lg border border-border">
          <Pane title="In this group" note={inGroup().length > 0 ? `${cameraCount(inGroup().length)}, in the order the sidebar lists them.` : undefined}>
            <If
              when={inGroup().length > 0}
              else={<Text class="px-2.5 text-sm text-muted-foreground">{states().length === 0 ? "There are no cameras yet. Add one, then put it in a group." : "No cameras yet. Put some in from the other cameras."}</Text>}
            >
              <Each of={placed()} key={(each) => each.camera.id}>
                {(each) => (
                  <CameraLine camera={each().camera}>
                    <IconButton icon="arrow-up" size="small" label={`Move ${each().camera.name} up`} disabled={each().index === 0} onPress={() => move(each().camera.id, -1)} />
                    <IconButton
                      icon="arrow-down"
                      size="small"
                      label={`Move ${each().camera.name} down`}
                      disabled={each().index === inGroup().length - 1}
                      onPress={() => move(each().camera.id, 1)}
                    />
                    <IconButton icon="x" size="small" label={`Take ${each().camera.name} out`} onPress={() => takeOut(each().camera.id)} />
                  </CameraLine>
                )}
              </Each>
            </If>
          </Pane>

          <View class="w-px bg-border" />

          <Pane
            title="Other cameras"
            search={
              <If when={outside() > 0}>
                <SearchField label="Find a camera" value={search()} onChange={(value) => search.set(value)} />
              </If>
            }
          >
            <If
              when={outside() > 0}
              else={
                <If when={states().length > 0}>
                  <Text class="px-2.5 text-sm text-muted-foreground">Every camera is in this group.</Text>
                </If>
              }
            >
              <If when={others().length > 0} else={<Text class="px-2.5 text-sm text-muted-foreground">No other camera has that name.</Text>}>
                <Each of={others()} key={(camera) => camera.id}>
                  {(camera) => (
                    <CameraLine camera={camera()}>
                      <IconButton icon="plus" size="small" label={`Put ${camera().name} in`} onPress={() => putIn(camera().id)} />
                    </CameraLine>
                  )}
                </Each>
              </If>
            </If>
          </Pane>
        </View>
      </View>
    </EditorDialog>
  );
}
