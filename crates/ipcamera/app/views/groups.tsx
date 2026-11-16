// The Groups page: every group in a table, to find them and change many at
// once, as the Cameras page lists the cameras. Under the header the other
// pages have, with the search and the filters in its bar, a row for each
// group: its name, how many cameras it holds, which, and what they are doing,
// sortable by each. Rows select as the Cameras page's do, and the bar that
// floats up while some are selected puts cameras in them or takes them out,
// duplicates them or deletes them. A row's pencil opens the group's dialog
// (`members.tsx`): its name, and its cameras in order. The rows come a page at
// a time, between the column titles and the footer, as on the Cameras page.
// What it shows is in `grouplist.ts`.
import { computed, createAnimatedValue, effect, keymap, List, Scroll, type ScrollRef, signal, Text, View, type ViewStyle } from "nsh";
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
import {
  bulkEditing,
  deleteGroups,
  duplicateGroups,
  editGroup,
  editing,
  groupCount,
  groupEditing,
  groupMenu,
  groupSelectionMenu,
  memberChoices,
  type MenuEntries,
  popup,
} from "./actions";
import {
  cameraFilter,
  chosen,
  clearFilters,
  clearSelection,
  emptyCount,
  filtered,
  type GroupRow,
  grouping,
  keepShown,
  listing,
  membersOf,
  onPage,
  pick,
  query,
  rows,
  STATUSES,
  type SortKey,
  selectAll,
  selected,
  shown,
  sort,
  sortBy,
  statusFilter,
  statusOf,
  summary,
  turnTo,
} from "./grouplist";
import { countLabel } from "./library";
import { pickOf } from "./listing";
import { Live, Stats } from "./masthead";
import { CHART, states } from "./model";
import { ListingFooter, PageCheck } from "./paged";

const tabular = { fontVariant: ["tabular-nums" as const] };

/** Either side of the table, as the header's banner is set in. */
const GUTTER = 24;

/**
 * Each column's share of a row, the same in the titles and the rows: a fixed
 * width, or a share of what is left with the narrowest it gets.
 */
const COLUMN: Record<"check" | "name" | "cameras" | "members" | "status" | "actions", ViewStyle> = {
  check: { width: 40 },
  name: { flex: 1.4, minWidth: 140 },
  cameras: { width: 100 },
  members: { flex: 2, minWidth: 140 },
  status: { width: 180 },
  actions: { width: 64 },
};

/** The page's width from which each column that can give way shows: below it, the column goes and its room goes to the rest. */
const SHOWN_FROM = { cameras: 560, status: 680, members: 800 };

type Columns = { cameras: boolean; members: boolean; status: boolean };

/** "1 camera", "3 cameras". */
const cameraCount = (count: number) => (count === 1 ? "1 camera" : `${countLabel(count)} cameras`);

/** Which way the table is sorted by `key`, for its title. */
const directionOf = (key: SortKey): SortDirection => {
  const by = sort();
  return by?.key === key ? (by.descending ? "descending" : "ascending") : null;
};

/** A row's menu, from its right press or its `…`: the selection's on a row of a selection of several, else the row's own, with the row alone selected. */
function rowMenu(row: GroupRow): MenuEntries {
  if (selected().includes(row.id) && chosen().length > 1) return groupSelectionMenu(chosen().map((each) => each.group));
  pick(row.id, "only");
  // Moving a group is in the sidebar's order, which the table shows only while no column sorts it.
  return groupMenu(row.group, sort() === null);
}

/** One group's row: its checkbox, its cells, and its pencil and menu at the end. */
function GroupLine(props: { row: GroupRow; columns: Columns }) {
  const on = () => selected().includes(props.row.id);
  const row = () => props.row;
  const status = () => statusOf(row());
  return (
    <View style={{ paddingHorizontal: GUTTER - 8 }} onContextMenu={(event) => popup(rowMenu(row()), event.pagePosition)}>
      <TableRow selected={on()}>
        <View class="items-center" style={COLUMN.check}>
          <CheckBox label={`Select ${row().name}`} checked={on()} onChange={() => pick(row().id, "toggle")} />
        </View>

        {/* The cells are one press: a click selects as Explorer does, a double click opens the group's dialog. */}
        <View
          role="button"
          accessibilityLabel={`${row().name}, ${cameraCount(row().cameras.length)}`}
          accessibilityState={{ selected: on() }}
          testID={`group:${row().name}`}
          class="h-full min-w-0 flex-1 flex-row items-center"
          onPress={(event) => {
            if (event.clickCount === 2) editGroup(row().id);
            else pick(row().id, pickOf(event.modifiers));
          }}
        >
          <View class="flex-row items-center gap-2.5 px-2" style={COLUMN.name}>
            <Icon name="folder" size={16} tint="muted-foreground" />
            <Text class={`min-w-0 flex-1 text-sm font-medium ${rowText(on())}`} style={{ numberOfLines: 1 }}>
              {row().name}
            </Text>
            <If when={row().recording > 0}>
              <LiveDot label="Recording" />
            </If>
          </View>
          <If when={props.columns.cameras}>
            <View class="px-2" style={COLUMN.cameras}>
              <Text class={`text-sm ${rowText(on())}`} style={{ numberOfLines: 1, ...tabular }}>
                {countLabel(row().cameras.length)}
              </Text>
            </View>
          </If>
          <If when={props.columns.members}>
            <View class="px-2" style={COLUMN.members}>
              <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1 }}>
                {membersOf(row()) || "No cameras yet"}
              </Text>
            </View>
          </If>
          <If when={props.columns.status}>
            <View class="flex-row items-center gap-2 px-2" style={COLUMN.status}>
              <StatusDot tone={status().tone} />
              <Text class={`text-sm ${status().tone === "warn" ? "text-warning" : "text-muted-foreground"}`} style={{ numberOfLines: 1, ...tabular }}>
                {status().text}
              </Text>
            </View>
          </If>
        </View>

        <View class="flex-row items-center justify-end gap-0.5 pr-1" style={COLUMN.actions}>
          <IconButton icon="pencil" size="small" label={`Edit ${row().name}`} onPress={() => editGroup(row().id)} />
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
          <If when={props.columns.cameras}>
            <ColumnTitle title="Cameras" sort={directionOf("cameras")} onSort={() => sortBy("cameras")} style={COLUMN.cameras} />
          </If>
          <If when={props.columns.members}>
            <ColumnTitle title="Members" sort={directionOf("members")} onSort={() => sortBy("members")} style={COLUMN.members} />
          </If>
          <If when={props.columns.status}>
            <ColumnTitle title="Status" sort={directionOf("status")} onSort={() => sortBy("status")} style={COLUMN.status} />
          </If>
        </View>
        <View style={COLUMN.actions} />
      </TitleRow>
    </View>
  );
}

/** The search and the filters, in the header's bar. */
function Filters() {
  const cameraLabel = () => {
    const camera = cameraFilter();
    return camera === null ? "Any camera" : (states().find((each) => each.id === camera)?.name ?? "Any camera");
  };
  const statusLabel = () => STATUSES.find((each) => each.key === statusFilter())?.title ?? "Any status";
  const holding = (id: string) => rows().filter((row) => row.group.cameras.includes(id)).length;

  return (
    <View class="flex-row items-center gap-2">
      <SearchField label="Search groups" value={query()} class="w-56" onChange={(value) => query.set(value)} />
      <PopupMenu trigger={<MenuChip icon="video" label={cameraLabel()} accessibilityLabel="Camera filter" />}>
        <MenuRow label="Any camera" checked={cameraFilter() === null} onSelect={() => cameraFilter.set(null)} />
        <If when={states().length > 0}>
          <MenuLine />
        </If>
        <Each of={states()} key={(camera) => camera.id}>
          {(camera) => (
            <MenuRow
              label={`${camera().name}  ·  ${countLabel(holding(camera().id))}`}
              checked={cameraFilter() === camera().id}
              onSelect={() => cameraFilter.set(camera().id)}
            />
          )}
        </Each>
      </PopupMenu>
      <PopupMenu trigger={<MenuChip icon="circle" label={statusLabel()} accessibilityLabel="Status filter" />}>
        <Each of={STATUSES} key={(each) => each.key}>
          {(each) => <MenuRow label={each().title} checked={statusFilter() === each().key} onSelect={() => statusFilter.set(each().key)} />}
        </Each>
      </PopupMenu>
    </View>
  );
}

/** The header: how many groups show and what the filters keep, their stats, and the search, filters and button in its bar. */
function GroupsHeader(props: { scroll: ReturnType<typeof createAnimatedValue> }) {
  const counted = () => `${filtered() ? `${countLabel(shown().length)} of ` : ""}${groupCount(rows().length)}`;
  const subtitle = () => [counted(), summary()].filter((part) => part !== "").join("  ·  ");
  const recordingNow = () => rows().filter((row) => row.recording > 0).length;

  return (
    <CollapsingHeader
      scroll={props.scroll}
      icon="folder"
      chart={CHART.cameras}
      title="Groups"
      subtitle={subtitle()}
      details={
        <If when={recordingNow() > 0}>
          <Live text={`${groupCount(recordingNow())} recording now`} />
        </If>
      }
      aside={
        <Stats
          stats={[
            { icon: "cctv", label: "in a group", value: countLabel(grouping().grouped) },
            { icon: "video", label: "in no group", value: countLabel(grouping().ungrouped) },
            { icon: "folder-open", label: "empty", value: countLabel(emptyCount()) },
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
        <Button kind="primary" icon="folder-plus" label="New group" onPress={() => editGroup(null)} />
      </View>
    </CollapsingHeader>
  );
}

/**
 * The selection's keys, bound while the page shows: Control+A, Delete, Return
 * to edit one group and Control+D to duplicate; never while a field has the
 * keyboard or a dialog is open.
 */
function SelectionKeys() {
  const free = () => editing() === null && bulkEditing() === null && groupEditing() === null;
  const list = () => chosen().map((row) => row.group);
  keymap.bind("mod+a", () => free() && selectAll(), { when: "notEditing" });
  keymap.bind("delete", () => free() && chosen().length > 0 && deleteGroups(list()), { when: "notEditing" });
  keymap.bind("enter", () => free() && chosen().length === 1 && editGroup(chosen()[0].id), { when: "notEditing" });
  keymap.bind("mod+d", () => free() && chosen().length > 0 && duplicateGroups(list()), { when: "notEditing" });
  return null;
}

/** The bar over the page's foot while some groups are selected: put cameras in them or take them out, duplicate them, delete them. */
function Selection() {
  const list = () => chosen().map((row) => row.group);
  return (
    <If when={chosen().length > 0}>
      <SelectionBar count={`${countLabel(chosen().length)} selected`} onClear={clearSelection}>
        <MenuOpener>
          {(open) => <Button kind="ghost" icon="video" label="Cameras" accessibilityLabel={`Cameras in ${groupCount(chosen().length)}`} onPress={() => open(memberChoices(list()))} />}
        </MenuOpener>
        <Button kind="ghost" icon="copy" label="Duplicate" accessibilityLabel={`Duplicate ${groupCount(chosen().length)}`} onPress={() => duplicateGroups(list())} />
        <Button kind="ghost" icon="trash-2" label="Delete" accessibilityLabel={`Delete ${groupCount(chosen().length)}`} onPress={() => deleteGroups(list())} />
      </SelectionBar>
    </If>
  );
}

/** Before the first group: what groups are for, and the button that makes one. */
function NoGroups() {
  return (
    <EmptyState
      icon="folder"
      title="No groups yet"
      text="Put cameras in groups, such as Outside or Garage, to find them by place. A camera can be in several, and the sidebar lists it under each."
      action={<Button kind="primary" icon="folder-plus" label="New group" onPress={() => editGroup(null)} />}
    />
  );
}

/** When the search and the filters leave nothing: the button that clears them. */
function NothingMatches() {
  return (
    <View class="items-center gap-3 px-6 py-12">
      <Text class="text-sm text-muted-foreground">No groups match the search and filters.</Text>
      <Button label="Clear filters" onPress={clearFilters} />
    </View>
  );
}

/** A row of the list: the titles, or a group's. */
type Line = { kind: "titles"; key: string } | { kind: "group"; key: string; row: GroupRow };

/** The Groups page, in place of the stage and the PTZ panel. */
export function GroupsPage() {
  const scroll = createAnimatedValue(0);
  const width = signal(0);
  const columns = computed(
    (): Columns => ({ cameras: width() >= SHOWN_FROM.cameras, members: width() >= SHOWN_FROM.members, status: width() >= SHOWN_FROM.status }),
  );

  // The selection holds only what the table shows.
  effect(() => {
    shown();
    keepShown();
  });

  const lines = computed((): Line[] => (onPage().length === 0 ? [] : [{ kind: "titles", key: "titles" }, ...onPage().map((row) => ({ kind: "group" as const, key: row.id, row }))]));

  let scroller: ScrollRef | undefined;
  /** Turns to page `index`, and brings its first row into view under the column titles, as the Cameras page does. */
  const turn = (index: number) => {
    turnTo(index);
    scroll.getValue().then((at) => {
      if (at > HEADER_COLLAPSE) scroller?.scrollTo({ y: HEADER_COLLAPSE, animated: false });
    });
  };

  return (
    <View testID="groups" class="flex-1 bg-background" onLayout={(event) => width.set(event.bounds.size.width)}>
      <SelectionKeys />
      <If when={rows().length > 0} else={<NoGroups />}>
        {/* The rows scroll between the column titles, which stick under the header's bar, and the footer, which stays put. */}
        <View class="flex-1">
          <Scroll ref={(ref: ScrollRef) => (scroller = ref)} scrollValue={scroll} class="flex-1">
            <GroupsHeader scroll={scroll} />
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
                <If when={line().kind === "group" ? (line() as Extract<Line, { kind: "group" }>).row : undefined} else={<Titles columns={columns()} />}>
                  {(row) => <GroupLine row={row()} columns={columns()} />}
                </If>
              )}
            </List>
          </Scroll>
          <Selection />
        </View>
        <ListingFooter listing={listing} total={rows().length} filtered={filtered()} count={groupCount} onTurn={turn} />
      </If>
    </View>
  );
}
