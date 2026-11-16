// The filter, sort, group and view menus of the snapshots, videos and events
// pages, in the bar of their header, so they stay in reach however far a page
// scrolls.
import { View } from "nsh";
import { MenuChip, MenuLine, MenuRow, PopupMenu, SectionLabel } from "../../../dp/ui";
import { cameraName, countLabel, GROUPINGS, type Item, type Library, ORDERS, PERIODS, SIZES, tagFilterLabel } from "./library";

/**
 * One page's choices: which camera, which tags, which dates, in what order
 * under which headings, and at what size. `sources` offers Taken by hand and
 * From events in the Tag menu, where the page lists files: not on the events
 * page, whose every item is an event.
 */
export function LibraryFilters<T extends Item>(props: { library: Library<T>; sources?: boolean }) {
  const library = () => props.library;
  const cameraLabel = () => {
    const chosen = library().cameraFilter();
    return chosen === null ? "All cameras" : cameraName(chosen);
  };
  const tags = () => library().tags();
  const periodLabel = () => PERIODS.find((each) => each.key === library().period())?.title ?? "All time";
  const arrangeLabel = () =>
    `${ORDERS.find((each) => each.key === library().order())?.short ?? ""} · ${GROUPINGS.find((each) => each.key === library().grouping())?.short ?? ""}`;
  const size = () => SIZES.find((each) => each.key === library().size()) ?? SIZES[1];

  return (
    <View class="flex-row items-center gap-2">
      <PopupMenu trigger={<MenuChip icon="video" label={cameraLabel()} accessibilityLabel="Camera filter" />}>
        <MenuRow label="All cameras" checked={library().cameraFilter() === null} onSelect={() => library().cameraFilter.set(null)} />
        <MenuLine />
        <Each of={library().cameras()} key={(camera) => camera.id}>
          {(camera) => (
            <MenuRow
              label={`${camera().name}  ·  ${countLabel(camera().count)}`}
              checked={library().cameraFilter() === camera().id}
              onSelect={() => library().cameraFilter.set(camera().id)}
            />
          )}
        </Each>
      </PopupMenu>
      <PopupMenu trigger={<MenuChip icon="tag" label={tagFilterLabel(library().tagFilter(), "All tags")} accessibilityLabel="Tag filter" />}>
        <MenuRow label="All tags" checked={library().tagFilter() === "all"} onSelect={() => library().tagFilter.set("all")} />
        <If when={props.sources}>
          <MenuRow label={`Taken by hand  ·  ${countLabel(tags().hand)}`} checked={library().tagFilter() === "hand"} onSelect={() => library().tagFilter.set("hand")} />
          <MenuRow label={`From events  ·  ${countLabel(tags().event)}`} checked={library().tagFilter() === "event"} onSelect={() => library().tagFilter.set("event")} />
        </If>
        <If when={tags().each.length > 0}>
          <MenuLine />
          <SectionLabel text="What the camera saw" />
          <Each of={tags().each} key={(tag) => tag.key}>
            {(tag) => (
              <MenuRow
                label={`${tag().title}  ·  ${countLabel(tag().count)}`}
                checked={library().tagFilter() === tag().key}
                onSelect={() => library().tagFilter.set(tag().key)}
              />
            )}
          </Each>
        </If>
      </PopupMenu>
      <PopupMenu trigger={<MenuChip icon="calendar" label={periodLabel()} accessibilityLabel="Date filter" />}>
        <Each of={PERIODS} key={(each) => each.key}>
          {(each) => <MenuRow label={each().title} checked={library().period() === each().key} onSelect={() => library().period.set(each().key)} />}
        </Each>
      </PopupMenu>
      <PopupMenu trigger={<MenuChip icon="arrow-down-up" label={arrangeLabel()} accessibilityLabel="Sort and group" />}>
        <SectionLabel text="Sort" />
        <Each of={ORDERS} key={(each) => each.key}>
          {(each) => <MenuRow label={each().title} checked={library().order() === each().key} onSelect={() => library().order.set(each().key)} />}
        </Each>
        <MenuLine />
        <SectionLabel text="Group by" />
        <Each of={GROUPINGS} key={(each) => each.key}>
          {(each) => <MenuRow label={each().title} checked={library().grouping() === each().key} onSelect={() => library().grouping.set(each().key)} />}
        </Each>
      </PopupMenu>
      <PopupMenu trigger={<MenuChip icon={size().icon} label={size().title} accessibilityLabel="View" />}>
        <Each of={SIZES} key={(each) => each.key}>
          {(each) => <MenuRow label={each().title} checked={library().size() === each().key} onSelect={() => library().size.set(each().key)} />}
        </Each>
      </PopupMenu>
    </View>
  );
}
