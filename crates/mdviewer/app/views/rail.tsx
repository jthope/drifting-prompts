import { computed, Scroll, signal, View } from "nsh";
import { HoverCard } from "nsh/base";
import { OutlineList, popupSurface } from "../../../dp/ui";
import { MARKS, railLines } from "./minimap";
import { goToHeading, headings, opened, outlineLines, reading } from "./model";

const RAIL_MARGIN = 16;

const OPEN_AFTER = 150;

const CARD_WIDTH = 240;

/** The heading rail at the reader's right edge, the outline's stand-in while it is hidden. */
export function HeadingRail() {
  const height = signal(0);
  const open = signal(false);
  const lineCount = computed(() => (opened()?.text ?? "").split("\n").length);
  const lines = computed(() => railLines(headings(), lineCount(), height(), RAIL_MARGIN));
  const surface = () => popupSurface("p-1.5");
  return (
    <HoverCard
      open={open()}
      openDelay={OPEN_AFTER}
      placement="left"
      offset={4}
      margin={12}
      popupClass={surface().class}
      popupStyle={{ ...surface().style, width: CARD_WIDTH }}
      onOpenChange={(event) => open.set(event.open)}
      trigger={
        <View testID="rail" accessibilityLabel="Headings" class="h-full w-10" onLayout={(event) => height.set(event.bounds.size.height)}>
          <Each of={lines()} key={(line) => String(line.index)}>
            {(line) => <View class={`absolute right-3 h-0.5 rounded-full ${line().index === reading() ? MARKS.section : MARKS.line}`} style={{ top: line().top, width: line().width }} />}
          </Each>
        </View>
      }
    >
      <Scroll testID="rail-headings" class="max-h-96">
        <OutlineList
          lines={outlineLines()}
          current={reading()}
          onSelect={(index) => {
            open.set(false);
            goToHeading(index);
          }}
        />
      </Scroll>
    </HoverCard>
  );
}
