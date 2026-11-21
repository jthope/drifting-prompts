import { computed, createAnimatedValue, effect, keymap, List, Scroll, type ScrollRef, signal, Text, View, type ViewStyle } from "nsh";
import { Library } from "ipcamera/native";
import {
  Button,
  CheckBox,
  CollapsingHeader,
  ColumnTitle,
  collapsedness,
  EmptyState,
  HEADER_BAR,
  HEADER_COLLAPSE,
  Icon,
  IconButton,
  LiveDot,
  MenuChip,
  MenuLine,
  MenuOpener,
  MenuRow,
  PopupMenu,
  rowText,
  SearchField,
  SelectionBar,
  StatusDot,
  type SortDirection,
  TABLE_ROW,
  TableRow,
  TitleRow,
} from "../../../dp/ui";
import { bulkEditing, cameraMenu, deleteCameras, duplicateCameras, editCamera, editCameras, editing, groupChoices, groupEditing, type MenuEntries, newCamera, popup, selectionMenu } from "./actions";
import { countLabel } from "./library";
import { Live, Stats } from "./masthead";
import { CHART, groups, show, states, views } from "./model";
import { ListingFooter, PageCheck } from "./paged";
import {
  addressOf,
  type CameraRow,
  chosen,
  clearFilters,
  clearSelection,
  filtered,
  groupFilter,
  keepShown,
  listing,
  onPage,
  pick,
  pickOf,
  query,
  reload,
  rows,
  STATUSES,
  STREAMS,
  type SortKey,
  selectAll,
  selected,
  shown,
  sort,
  sortBy,
  statusFilter,
  streamFilter,
  streamOf,
  summary,
  turnTo,
} from "./roster";

const tabular = { fontVariant: ["tabular-nums" as const] };

const GUTTER = 24;

const COLUMN: Record<"check" | "name" | "address" | "stream" | "login" | "groups" | "actions", ViewStyle> = {
  check: { width: 40 },
  name: { flex: 2, minWidth: 140, maxWidth: 280 },
  address: { flex: 1.4, minWidth: 130, maxWidth: 210 },
  stream: { width: 160 },
  login: { width: 150 },
  groups: { flex: 1.4, minWidth: 110, maxWidth: 320 },
  actions: { width: 64 },
};

const SHOWN_FROM = { stream: 620, login: 760, groups: 900 };

type Columns = { stream: boolean; login: boolean; groups: boolean };

const cameraCount = (count: number) => (count === 1 ? "1 camera" : `${countLabel(count)} cameras`);

const directionOf = (key: SortKey): SortDirection => {
  const by = sort();
  return by?.key === key ? (by.descending ? "descending" : "ascending") : null;
};

function rowMenu(row: CameraRow): MenuEntries {
  if (selected().includes(row.id) && chosen().length > 1) return selectionMenu(chosen().map((each) => each.state));
  pick(row.id, "only");
  return [{ label: "Show live video", onSelect: () => show({ kind: "camera", id: row.id }) }, { type: "separator" }, ...cameraMenu(row.state)];
}

function CameraLine(props: { row: CameraRow; columns: Columns; line: boolean }) {
  const on = () => selected().includes(props.row.id);
  const row = () => props.row;
  return (
    <View style={{ paddingHorizontal: GUTTER - 8 }} onContextMenu={(event) => popup(rowMenu(row()), event.pagePosition)}>
      <TableRow selected={on()} line={props.line}>
        <View class="items-center" style={COLUMN.check}>
          <CheckBox label={`Select ${row().name}`} checked={on()} onChange={() => pick(row().id, "toggle")} />
        </View>

        <View
          role="button"
          accessibilityLabel={`${row().name}, ${addressOf(row()) || "no address"}`}
          accessibilityState={{ selected: on() }}
          testID={`row:${row().name}`}
          class="h-full min-w-0 flex-1 flex-row items-center"
          onPress={(event) => {
            if (event.clickCount === 2) editCamera(row().id);
            else pick(row().id, pickOf(event.modifiers));
          }}
        >
          <View class="flex-row items-center gap-2.5 px-2" style={COLUMN.name}>
            <StatusDot tone={row().state.tone} />
            <Text class={`min-w-0 flex-1 text-sm font-medium ${rowText(on())}`} style={{ numberOfLines: 1 }}>
              {row().name}
            </Text>
            <If when={row().state.recording}>
              <LiveDot label="Recording" />
            </If>
          </View>
          <View class="px-2" style={COLUMN.address}>
            <Text class={`text-sm ${row().host === "" ? "text-muted-foreground" : rowText(on())}`} style={{ numberOfLines: 1, ...tabular }}>
              {addressOf(row()) || "No address yet"}
            </Text>
          </View>
          <If when={props.columns.stream}>
            <View class="px-2" style={COLUMN.stream}>
              <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1, ...tabular }}>
                {streamOf(row())}
              </Text>
            </View>
          </If>
          <If when={props.columns.login}>
            <View class="flex-row items-center gap-1.5 px-2" style={COLUMN.login}>
              <If
                when={row().hasPassword}
                else={
                  <>
                    <Icon name="key-round" size={14} tint="warning" />
                    <Text class="text-sm text-warning" style={{ numberOfLines: 1 }}>
                      No password
                    </Text>
                  </>
                }
              >
                <Text class={`text-sm ${rowText(on())}`} style={{ numberOfLines: 1 }}>
                  {row().user}
                </Text>
              </If>
            </View>
          </If>
          <If when={props.columns.groups}>
            <View class="px-2" style={COLUMN.groups}>
              <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1 }}>
                {row().groups.join(", ") || "—"}
              </Text>
            </View>
          </If>
        </View>

        <View class="flex-row items-center justify-end gap-0.5 pr-1" style={COLUMN.actions}>
          <IconButton icon="pencil" size="small" label={`Edit ${row().name}`} onPress={() => editCamera(row().id)} />
          <MenuOpener>{(open) => <IconButton icon="ellipsis" size="small" label={`${row().name} options`} onPress={() => open(rowMenu(row()))} />}</MenuOpener>
        </View>
      </TableRow>
    </View>
  );
}

function Titles(props: { columns: Columns }) {
  return (
    <View class="bg-background" style={{ paddingHorizontal: GUTTER - 8 }}>
      <TitleRow>
        <View class="items-center" style={COLUMN.check}>
          <PageCheck listing={listing} />
        </View>
        <View class="min-w-0 flex-1 flex-row items-center">
          <ColumnTitle title="Name" sort={directionOf("name")} onSort={() => sortBy("name")} style={COLUMN.name} />
          <ColumnTitle title="Address" sort={directionOf("address")} onSort={() => sortBy("address")} style={COLUMN.address} />
          <If when={props.columns.stream}>
            <ColumnTitle title="Stream" sort={directionOf("stream")} onSort={() => sortBy("stream")} style={COLUMN.stream} />
          </If>
          <If when={props.columns.login}>
            <ColumnTitle title="Login" sort={directionOf("login")} onSort={() => sortBy("login")} style={COLUMN.login} />
          </If>
          <If when={props.columns.groups}>
            <ColumnTitle title="Groups" sort={directionOf("groups")} onSort={() => sortBy("groups")} style={COLUMN.groups} />
          </If>
        </View>
        <View style={COLUMN.actions} />
      </TitleRow>
    </View>
  );
}

function Filters() {
  const groupLabel = () => {
    const chosenGroup = groupFilter();
    if (chosenGroup === null) return "All groups";
    if (chosenGroup === "") return "In no group";
    return groups().find((group) => group.id === chosenGroup)?.name ?? "All groups";
  };
  const statusLabel = () => STATUSES.find((each) => each.key === statusFilter())?.title ?? "Any status";
  const streamLabel = () => STREAMS.find((each) => each.key === streamFilter())?.title ?? "Any stream";
  const inGroup = (id: string) => rows().filter((row) => groups().some((group) => group.id === id && group.cameras.includes(row.id))).length;

  return (
    <View class="flex-row items-center gap-2">
      <SearchField label="Search cameras" value={query()} class="w-56" onChange={(value) => query.set(value)} />
      <PopupMenu trigger={<MenuChip icon="folder" label={groupLabel()} accessibilityLabel="Group filter" />}>
        <MenuRow label="All groups" checked={groupFilter() === null} onSelect={() => groupFilter.set(null)} />
        <MenuRow label="In no group" checked={groupFilter() === ""} onSelect={() => groupFilter.set("")} />
        <If when={groups().length > 0}>
          <MenuLine />
        </If>
        <Each of={groups()} key={(group) => group.id}>
          {(group) => (
            <MenuRow
              label={`${group().name}  ·  ${countLabel(inGroup(group().id))}`}
              checked={groupFilter() === group().id}
              onSelect={() => groupFilter.set(group().id)}
            />
          )}
        </Each>
      </PopupMenu>
      <PopupMenu trigger={<MenuChip icon="circle" label={statusLabel()} accessibilityLabel="Status filter" />}>
        <Each of={STATUSES} key={(each) => each.key}>
          {(each) => <MenuRow label={each().title} checked={statusFilter() === each().key} onSelect={() => statusFilter.set(each().key)} />}
        </Each>
      </PopupMenu>
      <PopupMenu trigger={<MenuChip icon="video" label={streamLabel()} accessibilityLabel="Stream filter" />}>
        <Each of={STREAMS} key={(each) => each.key}>
          {(each) => <MenuRow label={each().title} checked={streamFilter() === each().key} onSelect={() => streamFilter.set(each().key)} />}
        </Each>
      </PopupMenu>
    </View>
  );
}

function CamerasHeader(props: { scroll: ReturnType<typeof createAnimatedValue> }) {
  const counted = () => `${filtered() ? `${countLabel(shown().length)} of ` : ""}${cameraCount(rows().length)}`;
  const subtitle = () => [counted(), summary()].filter((part) => part !== "").join("  ·  ");
  const inNoView = () => rows().filter((row) => !views().some((view) => view.tiles.includes(row.id))).length;
  const noPassword = () => rows().filter((row) => !row.hasPassword).length;
  const recordingNow = () => rows().filter((row) => row.state.recording).length;

  return (
    <CollapsingHeader
      scroll={props.scroll}
      icon="cctv"
      chart={CHART.cameras}
      title="Cameras"
      subtitle={subtitle()}
      details={
        <If when={recordingNow() > 0}>
          <Live text={`${cameraCount(recordingNow())} recording now`} />
        </If>
      }
      aside={
        <Stats
          stats={[
            { icon: "folder", label: groups().length === 1 ? "group" : "groups", value: countLabel(groups().length) },
            { icon: "layout-grid", label: "in no view", value: countLabel(inNoView()) },
            { icon: "key-round", label: "no password", value: countLabel(noPassword()) },
          ]}
        />
      }
    >
      <Scroll horizontal scrollbar={false} class="min-w-0 flex-1" contentClass="flex-row items-center pr-2">
        <Filters />
      </Scroll>
      <View class="flex-row items-center gap-2">
        <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1, opacity: collapsedness(props.scroll), ...tabular }}>
          {counted()}
        </Text>
        <If when={filtered()}>
          <Button label="Clear filters" onPress={clearFilters} />
        </If>
        <Button kind="primary" icon="plus" label="Add camera" onPress={() => newCamera()} />
      </View>
    </CollapsingHeader>
  );
}

function SelectionKeys() {
  // A group's dialog opens from the sidebar too, so it can be over this page.
  const free = () => editing() === null && bulkEditing() === null && groupEditing() === null;
  keymap.bind("mod+a", () => free() && selectAll(), { when: "notEditing" });
  keymap.bind("delete", () => free() && chosen().length > 0 && deleteCameras(chosen().map((row) => row.state)), { when: "notEditing" });
  keymap.bind("enter", () => free() && editCameras(chosen().map((row) => row.id)), { when: "notEditing" });
  keymap.bind("mod+d", () => free() && duplicateCameras(chosen()), { when: "notEditing" });
  return null;
}

function Selection() {
  const ids = () => chosen().map((row) => row.id);
  return (
    <If when={chosen().length > 0}>
      <SelectionBar count={`${countLabel(chosen().length)} selected`} onClear={clearSelection}>
        <Button kind="ghost" icon="pencil" label="Edit" accessibilityLabel={`Edit ${cameraCount(chosen().length)}`} onPress={() => editCameras(ids())} />
        <Button kind="ghost" icon="copy" label="Duplicate" accessibilityLabel={`Duplicate ${cameraCount(chosen().length)}`} onPress={() => duplicateCameras(chosen())} />
        <MenuOpener>
          {(open) => <Button kind="ghost" icon="folder" label="Groups" accessibilityLabel={`Groups for ${cameraCount(chosen().length)}`} onPress={() => open(groupChoices(chosen()))} />}
        </MenuOpener>
        <Button kind="ghost" icon="trash-2" label="Delete" accessibilityLabel={`Delete ${cameraCount(chosen().length)}`} onPress={() => deleteCameras(chosen().map((row) => row.state))} />
      </SelectionBar>
    </If>
  );
}

function NoCameras() {
  return (
    <EmptyState
      icon="cctv"
      title="No cameras yet"
      text="Add a camera with its address and login. Once you have several, this is where to find them and change many at once."
      action={<Button kind="primary" icon="plus" label="Add camera" onPress={() => newCamera()} />}
    />
  );
}

function NothingMatches() {
  return (
    <View class="items-center gap-3 px-6 py-12">
      <Text class="text-sm text-muted-foreground">No cameras match the search and filters.</Text>
      <Button label="Clear filters" onPress={clearFilters} />
    </View>
  );
}

type Line = { kind: "titles"; key: string } | { kind: "camera"; key: string; row: CameraRow };

/** The Cameras page, in place of the stage and the PTZ panel. */
export function CamerasPage() {
  const scroll = createAnimatedValue(0);
  const width = signal(0);
  const columns = computed(
    (): Columns => ({ stream: width() >= SHOWN_FROM.stream, login: width() >= SHOWN_FROM.login, groups: width() >= SHOWN_FROM.groups }),
  );

  Library.onChange(() => reload());
  // A camera added or deleted changes the states before the library's listener hears of it.
  const ids = computed(() => states().map((state) => state.id).join(" "));
  effect(() => {
    ids();
    reload();
  });
  // The selection holds only what the table shows.
  effect(() => {
    shown();
    keepShown();
  });

  const lines = computed((): Line[] => (onPage().length === 0 ? [] : [{ kind: "titles", key: "titles" }, ...onPage().map((row) => ({ kind: "camera" as const, key: row.id, row }))]));
  const lastKey = computed(() => lines().at(-1)?.key);

  let scroller: ScrollRef | undefined;
  const turn = (index: number) => {
    turnTo(index);
    scroll.getValue().then((at) => {
      if (at > HEADER_COLLAPSE) scroller?.scrollTo({ y: HEADER_COLLAPSE, animated: false });
    });
  };

  return (
    <View class="flex-1 bg-background" onLayout={(event) => width.set(event.bounds.size.width)}>
      <SelectionKeys />
      <If when={states().length > 0} else={<NoCameras />}>
        <View class="flex-1">
          <Scroll ref={(ref: ScrollRef) => (scroller = ref)} testID="cameras" scrollValue={scroll} class="flex-1">
            <CamerasHeader scroll={scroll} />
            <List
              viewport="parent"
              each={lines()}
              keyExtractor={(line) => line.key}
              empty={<NothingMatches />}
              stickyHeaderIndices={[0]}
              stickyHeaderOffset={HEADER_BAR}
              estimatedItemSize={TABLE_ROW}
              contentStyle={{ paddingTop: 8, paddingBottom: 96 }}
            >
              {(line) => (
                <If when={line().kind === "camera" ? (line() as Extract<Line, { kind: "camera" }>).row : undefined} else={<Titles columns={columns()} />}>
                  {(row) => <CameraLine row={row()} columns={columns()} line={line().key !== lastKey()} />}
                </If>
              )}
            </List>
          </Scroll>
          <Selection />
        </View>
        <ListingFooter listing={listing} total={rows().length} filtered={filtered()} count={cameraCount} onTurn={turn} />
      </If>
    </View>
  );
}
