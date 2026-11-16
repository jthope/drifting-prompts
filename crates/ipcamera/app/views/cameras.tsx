// The Cameras page: every camera in a table, to find them and change many at
// once. Under the header the other pages have, with the search and the filters
// in its bar, a row for each camera: its name and status, address, stream,
// login and groups, sortable by each. Rows select as files do in Explorer (a
// press for one, Control for one more, Shift for a run) or by their
// checkboxes, and the bar that floats up while some are selected edits them
// together, duplicates them onto the next channels of their recorders, puts
// them in groups or deletes them. A row's pencil opens that camera's own
// settings. The rows come a page at a time, and scroll between the column
// titles, which stick under the header's bar, and a footer that stays put
// under them with the totals and the pager. What it shows is in `roster.ts`.
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

/** Either side of the table, as the header's banner is set in. */
const GUTTER = 24;

/**
 * Each column's share of a row, the same in the titles and the rows: a fixed
 * width, or a share of what is left with the narrowest it gets.
 */
const COLUMN: Record<"check" | "name" | "address" | "stream" | "login" | "groups" | "actions", ViewStyle> = {
  check: { width: 40 },
  name: { flex: 2, minWidth: 140 },
  address: { flex: 1.4, minWidth: 130 },
  stream: { width: 160 },
  login: { width: 150 },
  groups: { flex: 1.4, minWidth: 110 },
  actions: { width: 64 },
};

/** The page's width from which each column that can give way shows: below it, the column goes and its room goes to the rest. */
const SHOWN_FROM = { stream: 620, login: 760, groups: 900 };

type Columns = { stream: boolean; login: boolean; groups: boolean };

/** "1 camera", "3 cameras". */
const cameraCount = (count: number) => (count === 1 ? "1 camera" : `${countLabel(count)} cameras`);

/** Which way the table is sorted by `key`, for its title. */
const directionOf = (key: SortKey): SortDirection => {
  const by = sort();
  return by?.key === key ? (by.descending ? "descending" : "ascending") : null;
};

/** A row's menu, from its right press or its `…`: the selection's on a row of a selection of several, else the row's own, with the row alone selected. */
function rowMenu(row: CameraRow): MenuEntries {
  if (selected().includes(row.id) && chosen().length > 1) return selectionMenu(chosen().map((each) => each.state));
  pick(row.id, "only");
  return [{ label: "Show live video", onSelect: () => show({ kind: "camera", id: row.id }) }, { type: "separator" }, ...cameraMenu(row.state)];
}

/** One camera's row: its checkbox, its cells, and its pencil and menu at the end. */
function CameraLine(props: { row: CameraRow; columns: Columns }) {
  const on = () => selected().includes(props.row.id);
  const row = () => props.row;
  return (
    <View style={{ paddingHorizontal: GUTTER - 8 }} onContextMenu={(event) => popup(rowMenu(row()), event.pagePosition)}>
      <TableRow selected={on()}>
        <View class="items-center" style={COLUMN.check}>
          <CheckBox label={`Select ${row().name}`} checked={on()} onChange={() => pick(row().id, "toggle")} />
        </View>

        {/* The cells are one press: a click selects as Explorer does, a double click opens the camera's settings. */}
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

/** The column titles, which stay under the header's bar as the rows scroll, with the checkbox for the page (`PageCheck`). */
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

/** The search and the filters, in the header's bar. */
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

/** The header: how many cameras show and what the filters keep, their stats, and the search, filters and buttons in its bar. */
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
      {/* The filters scroll sideways in a window too narrow for them, rather than pushing the buttons out. */}
      <Scroll horizontal scrollbar={false} class="min-w-0 flex-1" contentClass="flex-row items-center pr-2">
        <Filters />
      </Scroll>
      <View class="flex-row items-center gap-2">
        {/* The count the masthead said, once the masthead has gone. */}
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

/**
 * The selection's keys, bound while the page shows: Control+A, Delete, Return
 * to edit and Control+D to duplicate; never while a field has the keyboard or
 * a dialog is open.
 */
function SelectionKeys() {
  // A group's dialog opens from the sidebar too, so it can be over this page.
  const free = () => editing() === null && bulkEditing() === null && groupEditing() === null;
  keymap.bind("mod+a", () => free() && selectAll(), { when: "notEditing" });
  keymap.bind("delete", () => free() && chosen().length > 0 && deleteCameras(chosen().map((row) => row.state)), { when: "notEditing" });
  keymap.bind("enter", () => free() && editCameras(chosen().map((row) => row.id)), { when: "notEditing" });
  keymap.bind("mod+d", () => free() && duplicateCameras(chosen()), { when: "notEditing" });
  return null;
}

/** The bar over the page's foot while some cameras are selected: edit them together, duplicate them, group them, delete them. */
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

/** Before the first camera: what the page is for, and the button that adds one. */
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

/** When the search and the filters leave nothing: the button that clears them. */
function NothingMatches() {
  return (
    <View class="items-center gap-3 px-6 py-12">
      <Text class="text-sm text-muted-foreground">No cameras match the search and filters.</Text>
      <Button label="Clear filters" onPress={clearFilters} />
    </View>
  );
}

/** A row of the list: the titles, or a camera's. */
type Line = { kind: "titles"; key: string } | { kind: "camera"; key: string; row: CameraRow };

/** The Cameras page, in place of the stage and the PTZ panel. */
export function CamerasPage() {
  const scroll = createAnimatedValue(0);
  const width = signal(0);
  const columns = computed(
    (): Columns => ({ stream: width() >= SHOWN_FROM.stream, login: width() >= SHOWN_FROM.login, groups: width() >= SHOWN_FROM.groups }),
  );

  // Reads every camera's settings now, then after each saved change, by this page or anywhere else.
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

  let scroller: ScrollRef | undefined;
  /**
   * Turns to page `index`, and brings its first row into view: scrolled past
   * the header's collapse, the list goes back to where its first row sits
   * under the column titles; above it, the header stays as open as it was.
   */
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
        {/* The rows scroll between the column titles, which stick under the header's bar, and the footer, which stays put. */}
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
              // Room under the last row for the selection's bar to float over.
              contentStyle={{ paddingTop: 8, paddingBottom: 96 }}
            >
              {(line) => (
                <If when={line().kind === "camera" ? (line() as Extract<Line, { kind: "camera" }>).row : undefined} else={<Titles columns={columns()} />}>
                  {(row) => <CameraLine row={row()} columns={columns()} />}
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
