import { View } from "nsh";
import type { CellInfo, ViewInfo } from "ipcamera/native";
import { ChipTrigger, look, MenuLine, MenuRow, PopupMenu } from "../../../dp/ui";
import { setLayout } from "./actions";
import { draft, draftCells, matchingLayout, startEditing, startFrom } from "./arrange";
import { grid, layouts } from "./model";

const percent = (cells: number): `${number}%` => `${(cells / grid.size) * 100}%`;

/** A small picture of a view's tiles. */
export function LayoutThumb(props: { cells: CellInfo[] }) {
  return (
    <View style={{ width: 24, height: 16 }}>
      <Each of={props.cells}>
        {(cell) => (
          <View
            style={{
              position: "absolute",
              padding: 1,
              left: percent(cell().column),
              top: percent(cell().row),
              width: percent(cell().columnSpan),
              height: percent(cell().rowSpan),
            }}
          >
            <View class={`flex-1 rounded-[1px] ${look().menuItem.picture}`} />
          </View>
        )}
      </Each>
    </View>
  );
}

/** How the view divides the stage; while the layout is edited, a layout picked here lays out the draft. */
export function LayoutMenu(props: { view: ViewInfo; class?: string }) {
  const editing = () => draft()?.view === props.view.id;
  const cells = () => (editing() ? draftCells() : props.view.cells);
  const current = () => matchingLayout(cells())?.key ?? "";
  return (
    <PopupMenu
      trigger={
        <ChipTrigger menu accessibilityLabel="Layout" class={props.class}>
          <LayoutThumb cells={cells()} />
        </ChipTrigger>
      }
    >
      <Each of={layouts}>
        {(layout) => (
          <MenuRow
            label={layout().title}
            leading={<LayoutThumb cells={layout().cells} />}
            checked={layout().key === current()}
            onSelect={() => (editing() ? startFrom(layout()) : setLayout(props.view, layout().key))}
          />
        )}
      </Each>
      <MenuLine />
      <MenuRow label="Edit layout…" disabled={editing()} onSelect={() => startEditing(props.view)} />
    </PopupMenu>
  );
}
