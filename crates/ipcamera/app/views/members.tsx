// A group's dialog: its name across the top, then two lists side by side, its
// cameras on the left in the order the sidebar lists them, each moved up or
// down or taken out, and the other cameras on the right, each put in with a
// press, under a search. Each list scrolls on its own. Save writes the name and
// the cameras together, in one save (`Groups.save`); Create makes a new group of
// them (`Groups.add`). Cancel, Escape and the backdrop discard the edits.
import { computed, effect, type JSX, Scroll, signal, Text, untracked, View } from "nsh";
import { type CameraState, Groups } from "ipcamera/native";
import { Button, EditorDialog, IconButton, look, SearchField, StatusDot, TextField } from "../../../dp/ui";
import { groupAdded, groupEditing } from "./actions";
import { countLabel } from "./library";
import { groups, refresh, states } from "./model";

/** "1 camera", "3 cameras". */
const cameraCount = (count: number) => (count === 1 ? "1 camera" : `${countLabel(count)} cameras`);

/** A camera in one of the dialog's lists: its status and name, then its buttons (`children`). */
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

/**
 * One of the dialog's two lists, sharing its width with the other: its title,
 * a line about it (`note`) or a search over it (`search`), and its cameras
 * (`children`), which scroll under them.
 */
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
  /** The group's cameras, by id, in its order, as edited so far. */
  const members = signal<string[]>([]);
  /** What the search over the other cameras holds. */
  const search = signal("");
  /** Why the last save failed. It stays until the next try, rather than closing as if it had worked. */
  const failure = signal("");
  const adding = () => groupEditing()?.group === null;

  // Each opening starts from what is saved, or from nothing for a new group, which is what makes
  // closing discard the edits.
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

  /** The group's cameras that exist, in its order: one deleted while the dialog is open drops out. */
  const inGroup = computed(() => members().flatMap((id) => states().find((camera) => camera.id === id) ?? []));
  /** The cameras not in it, in the order they were added, that every word of the search finds. */
  const others = computed(() => {
    const words = search().toLowerCase().split(/\s+/).filter((word) => word !== "");
    return states().filter((camera) => !members().includes(camera.id) && words.every((word) => camera.name.toLowerCase().includes(word)));
  });
  const outside = () => states().filter((camera) => !members().includes(camera.id)).length;
  const blankName = () => name().trim() === "";

  const close = () => groupEditing.set(null);

  // Saving takes a moment, so a second press while it does is ignored rather than saved twice.
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

  /** Moves camera `id` by `step` places in the group's order. */
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

  /** The group's cameras, each with where it stands, for the buttons that move it. */
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
