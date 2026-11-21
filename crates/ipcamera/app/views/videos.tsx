import { effect, Image, keymap, Text, Video, View } from "nsh";
import { Dialog } from "nsh/base";
import type { VideoInfo } from "ipcamera/native";
import { Videos } from "ipcamera/native";
import { Button, EmptyState, Icon, IconButton, look, MediaTile, MenuLayer, OverlayChip, RecMark, TitleBar } from "../../../dp/ui";
import { popup, toggleRecording } from "./actions";
import { LibraryGrid, NothingMatches } from "./grid";
import { TagChip } from "./gallery";
import { cameraName, countLabel, sizeLabel } from "./library";
import { fileLinks } from "./linked";
import { LibraryHeader } from "./masthead";
import { CHART, recording, storage } from "./model";
import {
  anyRecording,
  clearFilters,
  deleteVideo,
  deleteVideos,
  filtered,
  lengthLabel,
  loaded,
  openVideosFolder,
  ordered,
  play,
  playing,
  reload,
  revealVideo,
  startedInFull,
  startedLabel,
  tileMenu,
  totalLabel,
  totals,
  videoLibrary,
  videoMenu,
  videos,
  videosLabel,
  videosScroll,
} from "./recordings";

const WHILE_RECORDING_EVERY = 2000;

const tabular = { fontVariant: ["tabular-nums" as const] };

function Tile(props: { video: VideoInfo }) {
  const name = () => props.video.name;
  const list = () => videoLibrary.size() === "list";
  const picture = () => (
    <>
      <If
        when={props.video.thumbnail !== ""}
        else={
          <View class="flex-1 items-center justify-center">
            <Icon name="film" size={24} tint="muted-foreground" />
          </View>
        }
      >
        <Image src={props.video.thumbnail} fadeDuration={150} style={{ width: "100%", height: "100%", objectFit: "cover" }} />
      </If>
      <If when={!list()}>
        <TagChip tags={props.video.tags} />
        <OverlayChip class="absolute bottom-1.5 right-1.5">
          <If
            when={props.video.recording}
            else={
              <Text class="text-xs font-medium text-white" style={tabular}>
                {lengthLabel(props.video.seconds)}
              </Text>
            }
          >
            <RecMark length={lengthLabel(props.video.seconds)} />
          </If>
        </OverlayChip>
      </If>
    </>
  );
  return (
    <MediaTile
      label={`${cameraName(props.video.camera)}, ${startedLabel(props.video)}`}
      testID={`video:${name()}`}
      frame="black"
      picture={picture()}
      title={cameraName(props.video.camera)}
      detail={`${startedLabel(props.video)}  ·  ${sizeLabel(props.video.bytes)}`}
      extra={props.video.recording ? `Recording  ·  ${lengthLabel(props.video.seconds)}` : lengthLabel(props.video.seconds)}
      size={videoLibrary.size()}
      selected={videoLibrary.isSelected(name())}
      selecting={videoLibrary.selecting()}
      onPress={(keys) => videoLibrary.press(props.video, keys, () => play(props.video))}
      onCheck={(range) => videoLibrary.pick(name(), range ? "range" : "toggle")}
      onContextMenu={(at) => popup(tileMenu(props.video, fileLinks(props.video)), at)}
    />
  );
}

function VideosHeader() {
  const live = () => (totals().recording > 0 ? `${countLabel(totals().recording)} recording now` : undefined);

  return (
    <LibraryHeader
      scroll={videosScroll}
      icon="film"
      chart={CHART.videos}
      title="Videos"
      library={videoLibrary}
      sources
      count={totals().count}
      total={videos().length}
      one="video"
      many="videos"
      live={live()}
      stats={[
        { icon: "video", label: totals().cameras === 1 ? "camera" : "cameras", value: countLabel(totals().cameras) },
        { icon: "clock", label: "recorded", value: totalLabel(totals().seconds) },
        { icon: "hard-drive", label: "on disk", value: totals().onDisk },
      ]}
      actions={
        <>
          <If when={filtered()}>
            <Button label="Clear filters" onPress={clearFilters} />
          </If>
          <If when={recording()}>
            <IconButton icon="square" label="Stop recording" tint="destructive" onPress={toggleRecording} />
          </If>
          <If when={storage.canOpenFolder}>
            <IconButton icon="folder-open" label="Open folder" onPress={openVideosFolder} />
          </If>
        </>
      }
    />
  );
}

function NoVideos() {
  return (
    <EmptyState
      icon="film"
      title="No videos yet"
      text="Press the record button above the live video to record every connected camera on screen. They are kept in the videos folder Settings names."
      action={storage.canOpenFolder ? <Button icon="folder-open" label="Open folder" onPress={openVideosFolder} /> : undefined}
    />
  );
}

/** Newer bindings win, so Escape closes the player before it leaves full screen. */
function PlayerKeys(props: { onStep: (by: number) => void; onDelete: () => void; onClose: () => void }) {
  keymap.bind("left", () => props.onStep(-1));
  keymap.bind("right", () => props.onStep(1));
  keymap.bind("delete", () => props.onDelete());
  keymap.bind("escape", () => props.onClose());
  return null;
}

function Player() {
  const index = () => ordered().findIndex((video) => video.name === playing());
  // A recording that left the folder closes the player by itself.
  const video = () => ordered()[index()];
  const close = () => playing.set(null);
  const step = (by: number) => {
    const list = ordered();
    for (let at = index() + by; at >= 0 && at < list.length; at += by) {
      if (!list[at].recording) {
        playing.set(list[at].name);
        return;
      }
    }
  };
  const remove = async () => {
    const shown = video();
    if (!shown) return;
    const list = ordered();
    const at = index();
    const neighbor = list.slice(at + 1).find((each) => !each.recording) ?? list.slice(0, at).reverse().find((each) => !each.recording);
    if (await deleteVideo(shown)) playing.set(neighbor?.name ?? null);
  };

  return (
    <Dialog open={video() !== undefined} animation="fade" class="flex-1" backdropClass="bg-background" onOpenChange={(event) => !event.open && close()}>
      <If when={video()}>
        {(shown) => (
          <View class="flex-1 bg-background">
            <PlayerKeys onStep={step} onDelete={remove} onClose={close} />

            {/* It covers the header, so it is the title bar while it shows. */}
            <TitleBar class="pl-2">
              <View class="flex-row items-center">
                <IconButton icon="chevron-left" label="Previous video" disabled={index() <= 0} onPress={() => step(-1)} />
                <Text class="min-w-16 text-center text-xs text-muted-foreground" style={tabular}>
                  {`${countLabel(index() + 1)} of ${countLabel(ordered().length)}`}
                </Text>
                <IconButton icon="chevron-right" label="Next video" disabled={index() >= ordered().length - 1} onPress={() => step(1)} />
              </View>

              <View class="min-w-40 flex-1 gap-0.5">
                <Text class={`text-sm font-semibold ${look().titleText ?? "text-foreground"}`} style={{ numberOfLines: 1 }}>
                  {cameraName(shown().camera)}
                </Text>
                <Text class="text-xs text-muted-foreground" style={{ numberOfLines: 1 }}>
                  {[startedInFull(shown()), lengthLabel(shown().seconds), sizeLabel(shown().bytes)].join("  ·  ")}
                </Text>
              </View>

              <View class="flex-row items-center gap-1">
                <IconButton icon="folder-open" label={storage.revealLabel} onPress={() => revealVideo(shown())} />
                <IconButton icon="trash-2" label="Delete video" onPress={remove} />
                <IconButton icon="x" label="Close" onPress={close} />
              </View>
            </TitleBar>

            <View class="flex-1 bg-black" onContextMenu={(event) => popup(videoMenu(shown(), fileLinks(shown())), event.pagePosition)}>
              <Video src={shown().path} controls accessibilityLabel={`${cameraName(shown().camera)}, ${startedInFull(shown())}`} style={{ flex: 1 }} />
            </View>
            <MenuLayer />
          </View>
        )}
      </If>
    </Dialog>
  );
}

/** The videos page, in place of the stage and the PTZ panel. */
export function VideosPage() {
  videosScroll.set(0);
  Videos.onChange(() => reload());
  // A file being written doesn't change the folder's listing, so it is read again on a timer.
  effect(() => {
    if (!anyRecording()) return;
    const timer = setInterval(reload, WHILE_RECORDING_EVERY);
    return () => clearInterval(timer);
  });

  return (
    <View class="flex-1 bg-background">
      <If when={loaded()}>
        <If when={videos().length > 0} else={<NoVideos />}>
          <LibraryGrid
            testID="videos"
            scroll={videosScroll}
            library={videoLibrary}
            header={<VideosHeader />}
            empty={<NothingMatches what="videos" onClear={clearFilters} />}
            tile={(video) => <Tile video={video()} />}
            count={videosLabel}
            onDelete={deleteVideos}
            busy={() => playing() !== null}
          />
        </If>
      </If>
      <Player />
    </View>
  );
}
