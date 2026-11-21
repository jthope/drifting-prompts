import { cleanup, computed, effect, Scroll, type ScrollRef, signal, Text, View } from "nsh";
import { type ContextInfo, type FileMatches, Search } from "mdviewer/native";
import { Button, CountBadge, Icon, look, OutlineList, Press } from "../../../dp/ui";
import { goToResult } from "./actions";
import { folderOf } from "./document";
import { projectPath, projects, searchQuery, searchResults, searchScope } from "./model";
import { lineMarks, lineParts, nearestMatch, rowId, SCOPES } from "./search";

const AROUND = 2;

// The others start folded, so a large search shows at once.
const OPEN_FILES = 30;

const MOST_BLOCKS = 50;

const tabular = { fontVariant: ["tabular-nums" as const] };

const goers = new Set<(id: string) => void>();

const fileKey = (file: FileMatches) => rowId({ kind: "file", project: file.project, file: file.path });

function FileResults(props: { file: FileMatches; open: boolean; onToggle: () => void }) {
  const blocks = signal<ContextInfo[] | null>(null);
  effect(() => {
    const file = props.file;
    if (!props.open) return;
    let current = true;
    Search.context(
      file.project,
      file.path,
      file.matches.map((each) => each.startLine),
      AROUND,
    )
      .then((read) => {
        if (current) blocks.set(read);
      })
      .catch(() => {
        if (current) blocks.set([]);
      });
    return () => {
      current = false;
    };
  });
  const marks = computed(() => lineMarks(props.file.matches));
  const name = () => props.file.path.split("/").pop() ?? props.file.path;
  const elsewhere = () => (props.file.project === projectPath() ? "" : (projects().projects.find((each) => each.path === props.file.project)?.name ?? props.file.project));
  const where = () => [elsewhere(), folderOf(props.file.path)].filter(Boolean).join(" / ");
  const go = (line: number) => {
    const index = nearestMatch(props.file.matches, line);
    if (index >= 0) goToResult(rowId({ kind: "match", project: props.file.project, file: props.file.path, index }));
  };
  const shown = () => (blocks() ?? []).slice(0, MOST_BLOCKS);
  return (
    <View class="gap-1">
      <Press label={`${name()}, ${props.open ? "fold" : "open"}`} scale={1} class={`h-8 flex-row items-center gap-2 rounded-md px-2 ${look().listRow.hover}`} onPress={props.onToggle}>
        <Icon name={props.open ? "chevron-down" : "chevron-right"} size={14} tint="muted-foreground" />
        <Icon name="file-text" size={16} tint="foreground" />
        <Text class="text-sm font-semibold text-foreground" style={{ numberOfLines: 1 }}>
          {name()}
        </Text>
        <Text class="shrink text-xs text-muted-foreground" style={{ numberOfLines: 1 }}>
          {where()}
        </Text>
        <View class="flex-1" />
        <CountBadge count={props.file.matches.length} />
      </Press>
      <If when={props.open}>
        <View class="ml-5 border-l border-border pl-2">
          <If when={blocks()} else={<Text class="px-2 py-1 text-xs text-muted-foreground">Reading…</Text>}>
            <Each of={shown()} key={(block) => String(block.first)}>
              {(block, index) => (
                <View>
                  <If when={index() > 0}>
                    <Text class="py-0.5 pl-12 text-xs text-muted-foreground">⋯</Text>
                  </If>
                  <Each of={block().lines.map((text, at) => ({ text, line: block().first + at }))} key={(row) => String(row.line)}>
                    {(row) => (
                      <Press label={`Line ${row().line + 1}`} scale={1} class={`flex-row items-start gap-3 rounded px-1 ${look().listRow.hover}`} onPress={() => go(row().line)}>
                        <Text class="w-10 text-right font-mono text-xs text-muted-foreground" style={tabular}>
                          {String(row().line + 1)}
                        </Text>
                        <Text class="flex-1 font-mono text-xs text-foreground">
                          {lineParts(row().text, marks().get(row().line)).map((part) => (
                            <Text style={part.found ? { backgroundColor: look().found.match } : {}}>{part.text}</Text>
                          ))}
                        </Text>
                      </Press>
                    )}
                  </Each>
                </View>
              )}
            </Each>
            <If when={(blocks()?.length ?? 0) > MOST_BLOCKS}>
              <Text class="py-1 pl-12 text-xs text-muted-foreground">More in the file: F3 goes through them all.</Text>
            </If>
          </If>
        </View>
      </If>
    </View>
  );
}

/** The search's results as a tab of their own: each file's matches in place. */
export function ResultsView() {
  let scroll: ScrollRef | undefined;
  const folded = signal<string[] | null>(null);
  const files = computed(() => searchResults()?.files ?? []);
  const isOpen = (file: FileMatches, index: number) => {
    const closed = folded();
    return closed ? !closed.includes(fileKey(file)) : index < OPEN_FILES;
  };
  const toggle = (file: FileMatches, index: number) => {
    const closed = folded() ?? files().flatMap((each, at) => (at < OPEN_FILES ? [] : [fileKey(each)]));
    folded.set(isOpen(file, index) ? [...closed, fileKey(file)] : closed.filter((id) => id !== fileKey(file)));
  };
  const tops = new Map<string, number>();
  const go = (id: string) => {
    const top = tops.get(id);
    if (top !== undefined) scroll?.scrollTo({ y: Math.max(0, top - 8), animated: true });
  };
  goers.add(go);
  cleanup(() => goers.delete(go));

  const summary = () => {
    const results = searchResults();
    const scope = SCOPES.find((each) => each.value === searchScope())?.title ?? "";
    if (!results) return `${scope} · searching`;
    if (results.problem !== "") return results.problem;
    const total = `${results.total.toLocaleString()}${results.truncated ? "+" : ""} ${results.total === 1 ? "match" : "matches"}`;
    const count = `${results.files.length.toLocaleString()} ${results.files.length === 1 ? "file" : "files"}`;
    return `${scope} · ${total} in ${count} · updated as the files change`;
  };
  const anyOpen = () => files().some((file, index) => isOpen(file, index));
  return (
    <View testID="results-tab" class="min-h-0 flex-1">
      <View class="flex-row flex-wrap items-center gap-3 border-b border-border px-5 py-3">
        <Icon name="search" size={16} tint="muted-foreground" />
        <Text class="text-sm font-semibold text-foreground" style={{ numberOfLines: 1 }}>
          {searchQuery()?.text ?? ""}
        </Text>
        <Text class="flex-1 text-xs text-muted-foreground" style={{ ...tabular, numberOfLines: 1 }}>
          {summary()}
        </Text>
        <Button
          label={anyOpen() ? "Collapse all" : "Expand all"}
          kind="ghost"
          onPress={() => folded.set(anyOpen() ? files().map(fileKey) : [])}
        />
      </View>
      <Scroll
        ref={(ref) => {
          scroll = ref;
        }}
        class="flex-1"
        contentClass="gap-2 px-4 pb-10 pt-3"
      >
        <Each of={files()} key={fileKey}>
          {(file, index) => (
            <View onLayout={(event) => tops.set(fileKey(file()), event.bounds.origin.y)}>
              <FileResults file={file()} open={isOpen(file(), index())} onToggle={() => toggle(file(), index())} />
            </View>
          )}
        </Each>
      </Scroll>
    </View>
  );
}

/** The files in the results, for the outline's pane while the results' tab is in front. */
export function ResultsFiles() {
  const files = computed(() => searchResults()?.files ?? []);
  const lines = () => files().map((file) => ({ title: file.path, depth: 0, count: file.matches.length }));
  return (
    <Scroll testID="results-files" class="flex-1" contentClass="px-2 pb-4 pt-2">
      <OutlineList
        lines={lines()}
        current={-1}
        onSelect={(index) => {
          const file = files()[index];
          if (file) for (const go of goers) go(fileKey(file));
        }}
      />
    </Scroll>
  );
}
