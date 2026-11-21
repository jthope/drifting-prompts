import { View } from "nsh";
import { look, StatusItem, StatusText } from "../../../dp/ui";
import { stepMatch } from "./actions";
import { readingMinutes } from "./document";
import { caret, currentMatch, found, headings, listing, mode, opened, outlineOpen, page, searchQuery } from "./model";
import { searchSummary } from "./search";

const count = (n: number, one: string, many: string) => `${n.toLocaleString()} ${n === 1 ? one : many}`;

/** The status bar. */
export function StatusBar() {
  const words = () => opened()?.words ?? 0;
  return (
    <View class={`min-h-8 flex-row items-center gap-1 px-2 ${look().statusBar.frame}`}>
      <If when={page() === "project" && opened()}>
        <StatusText icon="file-text" tint="chart-1" text={count(words(), "word", "words")} />
        <If when={words() > 0}>
          <StatusText icon="clock" tint="chart-2" text={`${readingMinutes(words())} min read`} />
        </If>
        <StatusItem
          icon="list-tree"
          tint="chart-3"
          text={count(headings().length, "heading", "headings")}
          label={`${count(headings().length, "heading", "headings")}. Show the outline`}
          onPress={() => outlineOpen.set(true)}
        />
      </If>
      <If when={page() === "project"}>
        <If when={listing()}>{(shown) => <StatusText icon="folder" tint="chart-4" text={`${count(shown().files.length, "file", "files")}${shown().truncated ? " (more not listed)" : ""}`} />}</If>
      </If>
      <If when={page() === "project" && searchQuery() !== null}>
        <If when={found()} else={<StatusText icon="search" tint="warning" text="Searching" />}>
          {(results) => (
            <If when={results().problem === ""} else={<StatusText icon="triangle-alert" tint="warning" text={results().problem} />}>
              <StatusItem
                icon="search"
                tint="warning"
                text={searchSummary(results(), currentMatch())}
                label={`${searchSummary(results(), currentMatch())}. Go to the next match (F3)`}
                onPress={() => stepMatch(1)}
              />
            </If>
          )}
        </If>
      </If>
      <View class="flex-1" />
      <If when={page() === "project" && mode() !== "rendered"}>
        <If when={caret()}>{(at) => <StatusText text={`Ln ${at().line + 1}, Col ${at().column + 1}`} />}</If>
      </If>
    </View>
  );
}
