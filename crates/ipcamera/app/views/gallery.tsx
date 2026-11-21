import { Image, Text, View } from "nsh";
import type { SnapshotInfo } from "ipcamera/native";
import { Snapshots } from "ipcamera/native";
import { Button, EmptyState, Icon, IconButton, MediaTile, OVERLAY, OverlayChip } from "../../../dp/ui";
import { popup } from "./actions";
import { LibraryGrid, NothingMatches } from "./grid";
import { cameraName, countLabel, sizeLabel, tagTitle } from "./library";
import { fileLinks } from "./linked";
import { LibraryHeader } from "./masthead";
import { CHART, storage } from "./model";
import {
  clearFilters,
  deleteSnapshot,
  deleteSnapshots,
  filtered,
  galleryScroll,
  loaded,
  openSnapshotsFolder,
  ordered,
  reload,
  revealSnapshot,
  snapshotExtra,
  snapshotLibrary,
  snapshotMenu,
  snapshots,
  snapshotsLabel,
  takenInFull,
  takenLabel,
  tileMenu,
  viewing,
  visible,
} from "./snapshots";
import { PictureViewer } from "./viewer";

/** What an event saw as it saved a file (`Vehicle`), over its picture's corner. */
export function TagChip(props: { tags: string[] }) {
  return (
    <If when={props.tags.length > 0}>
      <OverlayChip class="absolute bottom-1.5 left-1.5">
        <Icon name="tag" size={12} color={OVERLAY.ink} />
        <Text class="text-xs font-medium text-white">{props.tags.map(tagTitle).join(", ")}</Text>
      </OverlayChip>
    </If>
  );
}

/** One picture: the snapshot's thumbnail, never the picture itself, which decodes to 8 MB at 1080p. */
function Tile(props: { shot: SnapshotInfo }) {
  const name = () => props.shot.name;
  const picture = () => (
    <>
      <If
        when={props.shot.thumbnail !== ""}
        else={
          <View class="flex-1 items-center justify-center">
            <Icon name="images" size={24} tint="muted-foreground" />
          </View>
        }
      >
        <Image src={props.shot.thumbnail} fadeDuration={150} style={{ width: "100%", height: "100%", objectFit: "cover" }} />
      </If>
      <If when={snapshotLibrary.size() !== "list"}>
        <TagChip tags={props.shot.tags} />
      </If>
    </>
  );
  return (
    <MediaTile
      label={`${cameraName(props.shot.camera)}, ${takenLabel(props.shot)}`}
      testID={`snapshot:${name()}`}
      picture={picture()}
      title={cameraName(props.shot.camera)}
      detail={takenLabel(props.shot)}
      extra={snapshotExtra(props.shot)}
      size={snapshotLibrary.size()}
      selected={snapshotLibrary.isSelected(name())}
      selecting={snapshotLibrary.selecting()}
      onPress={(keys) => snapshotLibrary.press(props.shot, keys, () => viewing.set(name()))}
      onCheck={(range) => snapshotLibrary.pick(name(), range ? "range" : "toggle")}
      onContextMenu={(at) => popup(tileMenu(props.shot, fileLinks(props.shot)), at)}
    />
  );
}

function GalleryHeader() {
  const cameras = () => new Set(visible().map((shot) => shot.camera)).size;
  const thisMonth = () => {
    const now = new Date();
    return visible().filter((shot) => {
      const at = new Date(shot.takenAt);
      return at.getFullYear() === now.getFullYear() && at.getMonth() === now.getMonth();
    }).length;
  };
  const onDisk = () => sizeLabel(visible().reduce((sum, shot) => sum + shot.bytes, 0));

  return (
    <LibraryHeader
      scroll={galleryScroll}
      icon="images"
      chart={CHART.snapshots}
      title="Snapshots"
      library={snapshotLibrary}
      sources
      count={visible().length}
      total={snapshots().length}
      one="snapshot"
      many="snapshots"
      stats={[
        { icon: "video", label: cameras() === 1 ? "camera" : "cameras", value: countLabel(cameras()) },
        { icon: "calendar", label: "this month", value: countLabel(thisMonth()) },
        { icon: "hard-drive", label: "on disk", value: onDisk() },
      ]}
      actions={
        <>
          <If when={filtered()}>
            <Button label="Clear filters" onPress={clearFilters} />
          </If>
          <If when={storage.canOpenFolder}>
            <IconButton icon="folder-open" label="Open folder" onPress={openSnapshotsFolder} />
          </If>
        </>
      }
    />
  );
}

function NoSnapshots() {
  return (
    <EmptyState
      icon="images"
      title="No snapshots yet"
      text="Press the camera button above a live picture to save one. They are kept in the snapshots folder Settings names."
      action={storage.canOpenFolder ? <Button icon="folder-open" label="Open folder" onPress={openSnapshotsFolder} /> : undefined}
    />
  );
}

function Viewer() {
  return (
    <PictureViewer
      items={ordered()}
      open={viewing()}
      onOpen={(name) => viewing.set(name)}
      noun="snapshot"
      src={(shot) => shot.path}
      icon="images"
      title={(shot) => cameraName(shot.camera)}
      detail={(shot) => [takenInFull(shot), shot.width > 0 ? `${shot.width} × ${shot.height}` : "", sizeLabel(shot.bytes)].filter((part) => part !== "").join("  ·  ")}
      menu={(shot) => snapshotMenu(shot, fileLinks(shot))}
      remove={deleteSnapshot}
      actions={(shot) => <IconButton icon="folder-open" label={storage.revealLabel} onPress={() => revealSnapshot(shot)} />}
    />
  );
}

/** The gallery page, in place of the stage and the PTZ panel. */
export function Gallery() {
  galleryScroll.set(0);
  Snapshots.onChange(() => reload());

  return (
    <View class="flex-1 bg-background">
      <If when={loaded()}>
        <If when={snapshots().length > 0} else={<NoSnapshots />}>
          <LibraryGrid
            testID="snapshots"
            scroll={galleryScroll}
            library={snapshotLibrary}
            header={<GalleryHeader />}
            empty={<NothingMatches what="snapshots" onClear={clearFilters} />}
            tile={(shot) => <Tile shot={shot()} />}
            count={snapshotsLabel}
            onDelete={deleteSnapshots}
            busy={() => viewing() !== null}
          />
        </If>
      </If>
      <Viewer />
    </View>
  );
}
