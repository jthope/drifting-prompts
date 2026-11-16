// The parts of a page's table that read its listing (`listing.ts`), the same on
// the Cameras and Groups pages: the column titles' checkbox, which selects the
// page shown, and the footer that stays under the rows with what the table
// counts and its pager.
import { Text } from "nsh";
import { CheckBox, Pager, TableFooter } from "../../../dp/ui";
import { countLabel } from "./library";
import { type Listing, PAGE_SIZES } from "./listing";

const tabular = { fontVariant: ["tabular-nums" as const] };

/**
 * The column titles' checkbox: it selects every row on the page, or takes
 * them all out of the selection; rows selected on other pages stay as they
 * are. A dash while only some of the page's rows are selected.
 */
export function PageCheck<R extends { id: string }, K extends string>(props: { listing: Listing<R, K> }) {
  const marked = () => props.listing.onPage().filter((row) => props.listing.selected().includes(row.id)).length;
  const all = () => props.listing.onPage().length > 0 && marked() === props.listing.onPage().length;
  const some = () => marked() > 0 && !all();
  return <CheckBox label={all() ? "Clear this page" : "Select this page"} checked={all()} mixed={some()} onChange={() => props.listing.togglePage()} />;
}

/**
 * The footer under a table's rows, which stays put as they scroll: which rows
 * the page holds of how many (`count` names them: "1 camera", "3 groups"), how
 * many there are in all (`total`) while the search or a filter leaves some out
 * (`filtered`), how many are selected (on every page), and the pager. `onTurn`
 * turns to a page.
 */
export function ListingFooter<R extends { id: string }, K extends string>(props: {
  listing: Listing<R, K>;
  total: number;
  filtered: boolean;
  count: (count: number) => string;
  onTurn: (index: number) => void;
}) {
  const span = () => props.listing.pageSpan();
  const counted = () => (span().total === 0 ? props.count(0) : `${countLabel(span().from)}–${countLabel(span().to)} of ${props.count(span().total)}`);
  return (
    <TableFooter
      pager={
        <Pager
          page={props.listing.currentPage()}
          pages={props.listing.pageCount()}
          size={props.listing.pageSize()}
          sizes={PAGE_SIZES}
          onPage={props.onTurn}
          onSize={(size) => {
            props.listing.pageSize.set(size);
            props.onTurn(0);
          }}
        />
      }
    >
      <Text class="text-sm text-foreground" style={{ numberOfLines: 1, ...tabular }}>
        {counted()}
      </Text>
      <If when={props.filtered}>
        <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1, ...tabular }}>
          {`·  ${countLabel(props.total)} in all`}
        </Text>
      </If>
      <If when={props.listing.chosen().length > 0}>
        <Text class="text-sm text-muted-foreground" style={{ numberOfLines: 1, ...tabular }}>
          {`·  ${countLabel(props.listing.chosen().length)} selected`}
        </Text>
      </If>
    </TableFooter>
  );
}
