// Breadcrumbs: the way to what a pane shows, a step at a time (a reader's
// project, its folders, the file and the heading the reader is under), with a
// chevron between one step and the next and the place itself emphasized. A
// step with a menu opens it on a press (a folder's files, the file's
// headings), with a toolbar button's hover under it; one without is plain
// text. Small and muted, it reads as the top of what it names rather than as a
// toolbar.
import { Text, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { look } from "./brand";
import { Hint } from "./button";
import { Icon, type IconName } from "./icon";
import { type MenuEntries, MenuOpener } from "./menu";

/** A step of `Breadcrumbs`. */
export type Crumb = {
  /** Tells it from the other steps (a folder's path). */
  id: string;
  /** What it says. */
  label: string;
  /** Its icon, before the label; none unless given. */
  icon?: IconName;
  /** The place itself (the file shown): emphasized. */
  current?: boolean;
  /** What a press on it offers, as a menu (a folder's files); without one it is plain text. */
  menu?: () => MenuEntries;
  /** What its menu holds, for a screen reader and its hint ("Files in docs"); its label unless given. */
  menuLabel?: string;
};

/** A step's icon and label, emphasized while it is the place itself. */
function CrumbText(props: { crumb: Crumb }) {
  return (
    <>
      <If when={props.crumb.icon}>{(icon) => <Icon name={icon()} size={12} tint={props.crumb.current ? "foreground" : "muted-foreground"} />}</If>
      <Text class={`shrink text-xs ${props.crumb.current ? "font-medium text-foreground" : "text-muted-foreground"}`} style={{ numberOfLines: 1 }}>
        {props.crumb.label}
      </Text>
    </>
  );
}

/**
 * The way to what is shown, as breadcrumbs: each of `crumbs` in turn, a
 * chevron between two, the one that is the place itself emphasized. A step
 * with a `menu` opens it on a press; the others are text. Every step keeps the
 * same room either side, so its text sits in one place whether or not it takes
 * presses: the first step's text starts 4 points in. Short of room, the steps
 * shorten their names. `class` places the row; a test finds it as `testID`.
 */
export function Breadcrumbs(props: { crumbs: Crumb[]; testID?: string; class?: string }) {
  return (
    <View testID={props.testID} class={`min-w-0 flex-row items-center ${props.class ?? ""}`}>
      <Each of={props.crumbs} key={(crumb) => crumb.id}>
        {(crumb, index) => (
          <View class="min-w-0 shrink flex-row items-center">
            <If when={index() > 0}>
              <Icon name="chevron-right" size={12} tint="muted-foreground" />
            </If>
            <If when={crumb().menu} else={<View class="min-w-0 shrink flex-row items-center gap-1 px-1 py-0.5"><CrumbText crumb={crumb()} /></View>}>
              {(menu) => (
                <MenuOpener class="min-w-0 shrink">
                  {(open) => (
                    <BaseButton
                      accessibilityLabel={crumb().menuLabel ?? crumb().label}
                      tooltip={<Hint text={crumb().menuLabel ?? crumb().label} />}
                      class={`min-w-0 flex-row items-center gap-1 rounded-sm px-1 py-0.5 ${look().buttons.ghost.frame} ${look().rings.visible}`}
                      onPress={() => open(menu()())}
                    >
                      <CrumbText crumb={crumb()} />
                    </BaseButton>
                  )}
                </MenuOpener>
              )}
            </If>
          </View>
        )}
      </Each>
    </View>
  );
}
