import { effect, Scroll, signal, Text, untracked, View } from "nsh";
import { Cameras, type CameraForm, type FoundCamera } from "ipcamera/native";
import { Button, CopyableNote, EditorDialog, Icon, Press, Spinner } from "../../../dp/ui";

const BRAND_FIELDS = ["brand", "rtspPort", "onvifPort", "onvifPath", "dvripPort", "apiPort", "apiHttps", "httpApi"] as const;

/** `now` with the brand fields Connect told for the picked camera, each unless changed since the pick; none once the address or brand changed. */
export function withToldBrand(now: CameraForm, picked: CameraForm, told: CameraForm): CameraForm {
  if (now.host !== picked.host || now.brand !== picked.brand) return now;
  const merged: CameraForm = { ...now };
  const take = <K extends keyof CameraForm>(field: K) => {
    if (now[field] === picked[field]) merged[field] = told[field];
  };
  BRAND_FIELDS.forEach(take);
  return merged;
}

/** A found camera's title: its model, else its name, else its address. */
export const foundTitle = (camera: FoundCamera) => camera.model || camera.name || camera.host;

/** What is under a found camera's title: its name where the title is its model, its address, and its details. */
export const foundDetail = (camera: FoundCamera) => {
  const title = foundTitle(camera);
  return [camera.name, camera.host, ...camera.details].filter((part) => part && part !== title).join(" · ");
};

/** The window that lists the cameras on the network, searching each time it opens. */
export function FindCamerasDialog(props: { open: boolean; onClose: () => void; onPick: (camera: FoundCamera) => void }) {
  const found = signal<FoundCamera[]>([]);
  const searching = signal(false);
  const failure = signal("");
  /** The newest search, so an older one that answers late changes nothing. */
  let asked = 0;

  function search() {
    const mine = ++asked;
    searching.set(true);
    failure.set("");
    found.set([]);
    Cameras.discover()
      .then((cameras) => mine === asked && found.set(cameras))
      .catch((error) => mine === asked && failure.set(String(error?.message ?? error)))
      .finally(() => mine === asked && searching.set(false));
  }

  effect(() => {
    if (props.open) untracked(search);
    // Closed, a search still out answers nobody.
    else asked++;
  });

  return (
    <EditorDialog
      title="Cameras on the network"
      icon="search"
      open={props.open}
      onClose={props.onClose}
      footer={
        <>
          <Text class="min-w-0 flex-1 text-xs text-muted-foreground">
            {searching() ? "" : found().length === 1 ? "1 camera answered." : found().length > 1 ? `${found().length} cameras answered.` : ""}
          </Text>
          <Button icon="search" label={searching() ? "Searching…" : "Search again"} disabled={searching()} onPress={search} />
          <Button label="Close" onPress={props.onClose} />
        </>
      }
    >
      <View class="min-h-0 flex-1 gap-3">
        <Text class="text-xs text-muted-foreground">
          Cameras on the same network answer here without a login: those with ONVIF on say what they are, and the others are listed by the address their video answers at. Choose one to take its address; Connect then asks it what it is.
        </Text>
        <If when={failure()}>
          <CopyableNote text={failure()} tone="error" />
        </If>
        <If when={searching()}>
          <View class="items-center py-6">
            <Spinner label="Searching the network…" />
          </View>
        </If>
        <If when={!searching() && !failure() && found().length === 0}>
          <Text class="text-sm text-muted-foreground">No camera answered. A camera on another network, or with its video on another port, is added by typing its address.</Text>
        </If>
        <Scroll style={{ flex: 1 }}>
          <View class="gap-1">
            <Each of={found()} key={(camera) => camera.host}>
              {(camera) => (
                <Press
                  label={`Use ${foundTitle(camera())} at ${camera().host}`}
                  testID={`found:${camera().host}`}
                  scale={0.99}
                  class="flex-row items-center gap-3 rounded-md px-3 py-2 hover:bg-accent"
                  onPress={() => props.onPick(camera())}
                >
                  <Icon name="cctv" />
                  <View class="min-w-0 flex-1">
                    <Text class="text-sm font-medium text-foreground">{foundTitle(camera())}</Text>
                    <If when={foundDetail(camera())}>
                      <Text class="text-xs text-muted-foreground">{foundDetail(camera())}</Text>
                    </If>
                  </View>
                </Press>
              )}
            </Each>
          </View>
        </Scroll>
      </View>
    </EditorDialog>
  );
}
