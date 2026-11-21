import { Image, type JSX, keymap, Text, View } from "nsh";
import { Dialog } from "nsh/base";
import { Icon, type IconName, IconButton, look, MenuLayer, TitleBar } from "../../../dp/ui";
import { type MenuEntries, popup } from "./actions";
import { countLabel, type Item } from "./library";

/** Newer bindings win, so Escape closes the viewer before it leaves full screen. */
function ViewerKeys(props: { onStep: (by: number) => void; onDelete: () => void; onClose: () => void }) {
  keymap.bind("left", () => props.onStep(-1));
  keymap.bind("right", () => props.onStep(1));
  keymap.bind("delete", () => props.onDelete());
  keymap.bind("escape", () => props.onClose());
  return null;
}

/** One of the page's pictures at full size, `open` by name; a picture that leaves the page closes it. */
export function PictureViewer<T extends Item>(props: {
  items: T[];
  open: string | null;
  onOpen: (name: string | null) => void;
  noun: string;
  src: (item: T) => string;
  icon: IconName;
  title: (item: T) => string;
  detail: (item: T) => string;
  menu: (item: T) => MenuEntries;
  remove: (item: T) => Promise<boolean>;
  canDelete?: (item: T) => boolean;
  actions?: (item: T) => JSX.Element;
}) {
  const index = () => props.items.findIndex((item) => item.name === props.open);
  const shown = () => props.items[index()];
  const close = () => props.onOpen(null);
  const step = (by: number) => {
    const next = props.items[index() + by];
    if (next) props.onOpen(next.name);
  };
  const deletable = (item: T) => props.canDelete?.(item) ?? true;
  const remove = async () => {
    const item = shown();
    if (!item || !deletable(item)) return;
    const list = props.items;
    const at = index();
    const neighbor = list[at + 1] ?? list[at - 1];
    if (await props.remove(item)) props.onOpen(neighbor?.name ?? null);
  };

  return (
    <Dialog open={shown() !== undefined} animation="fade" class="flex-1" backdropClass="bg-background" onOpenChange={(event) => !event.open && close()}>
      <If when={shown()}>
        {(item) => (
          <View class="flex-1 bg-background">
            <ViewerKeys onStep={step} onDelete={remove} onClose={close} />

            {/* It covers the header, so it is the title bar while it shows. */}
            <TitleBar class="pl-2">
              <View class="flex-row items-center">
                <IconButton icon="chevron-left" label={`Previous ${props.noun}`} disabled={index() <= 0} onPress={() => step(-1)} />
                <Text class="min-w-16 text-center text-xs text-muted-foreground" style={{ fontVariant: ["tabular-nums"] }}>
                  {`${countLabel(index() + 1)} of ${countLabel(props.items.length)}`}
                </Text>
                <IconButton icon="chevron-right" label={`Next ${props.noun}`} disabled={index() >= props.items.length - 1} onPress={() => step(1)} />
              </View>

              <View class="min-w-40 flex-1 gap-0.5">
                <Text class={`text-sm font-semibold ${look().titleText ?? "text-foreground"}`} style={{ numberOfLines: 1 }}>
                  {props.title(item())}
                </Text>
                <Text class="text-xs text-muted-foreground" style={{ numberOfLines: 1 }}>
                  {props.detail(item())}
                </Text>
              </View>

              <View class="flex-row items-center gap-1">
                {props.actions?.(item())}
                <IconButton icon="trash-2" label={`Delete ${props.noun}`} disabled={!deletable(item())} onPress={remove} />
                <IconButton icon="x" label="Close" onPress={close} />
              </View>
            </TitleBar>

            <View class="flex-1 bg-black" onContextMenu={(event) => popup(props.menu(item()), event.pagePosition)}>
              <If
                when={props.src(item()) !== ""}
                else={
                  <View class="flex-1 items-center justify-center">
                    <Icon name={props.icon} size={32} tint="muted-foreground" />
                  </View>
                }
              >
                <Image src={props.src(item())} fadeDuration={0} accessibilityLabel={`${props.title(item())}, ${props.detail(item())}`} style={{ flex: 1, objectFit: "contain" }} />
              </If>
            </View>
            <MenuLayer />
          </View>
        )}
      </If>
    </Dialog>
  );
}
