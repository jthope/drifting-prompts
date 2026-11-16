// The heading rail: while the outline is hidden (its button, or a window too
// narrow for it), a short line for each heading of the file stands at the
// reader's right edge, as far down as the heading is into the file and longer
// for higher levels, the one the reader is under in the focus ring's color
// (`railLines` in `minimap.ts`). The pointer resting on it opens a card of the
// headings by name, as the outline lists them with their search counts; a
// press on one goes there. It is nsh's `HoverCard`, which a press on the rail
// opens on a touch screen.
import { computed, Scroll, signal, View } from "nsh";
import { HoverCard } from "nsh/base";
import { OutlineList, popupSurface } from "../../../dp/ui";
import { MARKS, railLines } from "./minimap";
import { goToHeading, headings, opened, outlineLines, reading } from "./model";

/** The room above the first line and below the last, in points. */
const RAIL_MARGIN = 16;

/** How long the pointer rests on the rail before its card opens, in milliseconds: soon, since the rail is there to be read. */
const OPEN_AFTER = 150;

/** The width of the card of headings, in points: the outline's. */
const CARD_WIDTH = 240;

/** The heading rail, at the reader's right edge. */
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
