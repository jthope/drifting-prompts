import { computed, effect, Scroll, type ScrollRef, signal, Text, untracked, View } from "nsh";
import { Editor, type EditorRef, ResizablePanel, ResizablePanelGroup, TextView, type TextViewCustomNode, type TextViewDocument } from "nsh/base";
import { Button, EmptyState, IconButton, Island, look, OutlineList, PaneHeader, Spinner } from "../../../dp/ui";
import { followLink } from "./actions";
import { foldRanges, headingAnchors, headingAt, MARKDOWN, pictureSource, splitSections, withPictures } from "./document";
import { FileMap, type MapPreview } from "./map";
import { blockAround, lineAtPage, MARKS, onTrack, pageMap, rawMap, rulerMarks } from "./minimap";
import {
  caret,
  currentInFront,
  currentMatch,
  goesToHeadings,
  goToHeading,
  headings,
  listing,
  mapOpen,
  matchesInFront,
  mode,
  opened,
  openPath,
  outlineLines,
  projectPath,
  reading,
  resultsInFront,
  resultsTab,
  scrollTogether,
  searchQuery,
} from "./model";
import { HeadingRail } from "./rail";
import { ResultsFiles } from "./results";
import { createScrollSync, type ScrollSync } from "./scrollsync";
import { highlighted, matchMarks, onThePage, pageOffset, pagePattern } from "./search";
import { decorationsOf, PAGE } from "./syntax";

const SIDE_MIN_WIDTH = 240;

const HOLD_AFTER_GOING = 800;

const ABOVE_HEADING = 12;

const ABOVE_MATCH = 160;

// nsh's `Editor` reports no scrolling, so Raw's map polls which lines are in view.
const MAP_VIEW_CHECK_ACTIVE = 50;
const MAP_VIEW_CHECK_IDLE = 500;

const RAW_ROW_MOST = 3;
const PAGE_SCALE_MOST = 0.2;

function crumbOf(heading: number, first: number): string {
  const title = headings()[heading]?.title;
  return title ? `${title} · line ${first + 1}` : `Line ${first + 1}`;
}

const MEASURE_TRIES = 20;
const MEASURE_WAIT = 50;

function FrontMatter(props: { node: TextViewCustomNode }) {
  return (
    <View class="mb-4 rounded-md bg-muted px-3 py-2">
      <Text class="font-mono text-xs text-muted-foreground">{props.node.text ?? ""}</Text>
    </View>
  );
}

function RawView(props: { sync?: ScrollSync }) {
  let editor: EditorRef | undefined;
  let hovered = false;
  let focused = false;
  const active = () => props.sync?.rawActive(hovered || focused);
  const text = () => opened()?.text ?? "";
  const folds = computed(() => foldRanges(headings(), text().split("\n")));
  const colors = computed(() => decorationsOf(opened()?.marks ?? []));
  const decorations = computed(() => [...colors(), ...matchMarks(matchesInFront(), look().found.mark), ...rulerMarks(headings(), reading(), text().split("\n").length)]);
  const search = computed(() => {
    const query = searchQuery();
    return query ? { query: query.text, caseSensitive: query.caseSensitive, wholeWord: query.wholeWord, regex: query.regex } : null;
  });

  const picture = computed(() => rawMap(text(), opened()?.marks ?? [], headings()));
  const inView = signal<{ top: number; height: number } | null>(null);
  const readView = async () => {
    const range = await editor?.visibleRange();
    if (range) inView.set({ top: range.start.line, height: range.end.line - range.start.line + 1 });
  };
  let overMap = false;
  effect(() => {
    if (!mapOpen()) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const next = () => {
      void readView();
      timer = setTimeout(next, hovered || focused || overMap ? MAP_VIEW_CHECK_ACTIVE : MAP_VIEW_CHECK_IDLE);
    };
    next();
    return () => clearTimeout(timer);
  });
  const lines = computed(() => text().split("\n"));
  const preview = (unit: number): MapPreview => {
    const block = blockAround(lines(), Math.floor(unit), headings());
    return { crumb: crumbOf(block.heading, block.first), text: block.lines.join("\n"), page: false };
  };

  goesToHeadings((index) => {
    const heading = headings()[index];
    if (heading) editor?.goToLine(heading.line);
  });
  // Deferred until the editor has the file's text, which comes in the same turn when the step opened another file.
  effect(() => {
    const at = matchesInFront()[currentInFront()];
    text();
    if (!at) return;
    const timer = setTimeout(() => {
      const start = { line: at.startLine, column: at.startColumn };
      editor?.setSelections([{ anchor: start, head: { line: at.endLine, column: at.endColumn } }]);
      editor?.revealPosition(start, true);
    }, 0);
    return () => clearTimeout(timer);
  });
  effect(() => () => caret.set(null));
  effect(() => () => props.sync?.raw(undefined));

  return (
    <View class="flex-1 flex-row">
      <View
        class="flex-1"
        onMouseEnter={() => {
          hovered = true;
          active();
        }}
        onMouseLeave={() => {
          hovered = false;
          active();
        }}
        onLayout={(event) => props.sync?.rawHeight(event.bounds.size.height)}
      >
        <Editor
          ref={(ref) => {
            editor = ref;
            props.sync?.raw(ref ? { editor: ref, headingLines: () => headings().map((heading) => heading.line), lastLine: () => text().split("\n").length - 1 } : undefined);
          }}
          class="flex-1"
          value={text()}
          language="markdown"
          readOnly
          softWrap
          decorations={decorations()}
          search={search()}
          styles={{ searchMatch: { backgroundColor: look().found.match }, currentSearchMatch: { backgroundColor: look().found.current } }}
          foldRanges={folds()}
          scrollBeyondLastLine={false}
          accessibilityLabel={`${openPath()}, as written`}
          onSearchChange={(event) => {
            const index = event.result.current;
            const at = untracked(currentMatch);
            if (index !== null && index !== undefined && index < untracked(matchesInFront).length && (at?.file !== untracked(openPath) || at.index !== index)) {
              currentMatch.set({ file: untracked(openPath), index });
            }
          }}
          onFocus={() => {
            focused = true;
            active();
          }}
          onBlur={() => {
            focused = false;
            active();
          }}
          onSelectionChange={(event) => {
            const head = event.selections[event.selections.length - 1]?.head;
            if (!head) return;
            caret.set({ line: head.line, column: head.column });
            reading.set(headingAt(headings(), head.line));
          }}
        />
      </View>
      <If when={mapOpen()}>
        <View
          class="h-full"
          onMouseEnter={() => {
            overMap = true;
          }}
          onMouseLeave={() => {
            overMap = false;
          }}
        >
          <FileMap
            label={`Map of ${openPath()}, as written`}
            testID="raw-map"
            picture={picture()}
            scale={(height) => Math.min(RAW_ROW_MOST, height / Math.max(1, picture().extent))}
            view={inView()}
            matches={matchesInFront().map((each) => each.startLine)}
            preview={preview}
            onGo={(unit) => {
              editor?.revealPosition({ line: Math.floor(unit), column: 0 }, true);
              void readView();
              props.sync?.align();
            }}
          />
        </View>
      </If>
    </View>
  );
}

function RenderedView(props: { sync?: ScrollSync }) {
  let scroll: ScrollRef | undefined;
  const tops: number[] = [];
  let holdUntil = 0;

  let columnHeight = 0;
  // Bumped on every layout, since `tops` and `columnHeight` aren't signals.
  const measured = signal(0);
  const track = signal(0);

  const parsed = computed<TextViewDocument>(() => {
    const folder = projectPath();
    const file = openPath();
    const document = TextView.parse(opened()?.text ?? "", MARKDOWN);
    return folder === "" ? document : withPictures(document, (src) => pictureSource(folder, file, src));
  });
  const pattern = computed(() => pagePattern(searchQuery()));
  const current = computed(() => (currentInFront() < 0 ? null : onThePage(matchesInFront(), currentInFront(), headings(), opened()?.marks ?? [])));
  const pageSections = computed(() => splitSections(parsed()));
  const sections = computed(() => {
    const at = current();
    const wholeWord = searchQuery()?.wholeWord ?? false;
    return pageSections().map((document, index) => ({
      document: highlighted(document, pattern(), wholeWord, at?.section === index ? at.nth : -1, look().found).document,
      index,
    }));
  });
  const anchors = computed(() => headingAnchors(parsed()));
  const lines = computed(() => (opened()?.text ?? "").split("\n"));
  const matchPlaces = computed(() => {
    measured();
    return matchesInFront().flatMap((each) => {
      const place = pageOffset(each.startLine, headings(), lines().length, tops, columnHeight);
      return place === undefined ? [] : [place];
    });
  });
  const barMarks = computed<BarMarks>(() => {
    measured();
    const height = track();
    const shown = headings();
    const at = (offset: number) => onTrack(offset, columnHeight, height);
    const section = reading() + 1;
    const top = tops[section];
    return {
      headings: shown.flatMap((_, index) => {
        const start = tops[index + 1];
        return start === undefined ? [] : [at(start)];
      }),
      matches: matchPlaces().map(at),
      section: top === undefined || shown.length === 0 ? null : { top: at(top), height: Math.max(2, at(tops[section + 1] ?? columnHeight) - at(top)) },
    };
  });
  const picture = computed(() => {
    measured();
    return pageMap(pageSections(), tops, columnHeight);
  });
  const inView = signal<{ top: number; height: number } | null>(null);
  const preview = (unit: number): MapPreview => {
    const line = lineAtPage(unit, headings(), lines().length, tops, columnHeight);
    const block = blockAround(lines(), line, headings());
    return { crumb: crumbOf(block.heading, block.first), text: block.lines.join("\n"), page: true };
  };

  goesToHeadings((index) => {
    const top = tops[index + 1];
    if (top === undefined) return;
    holdUntil = Date.now() + HOLD_AFTER_GOING;
    scroll?.scrollTo({ y: Math.max(0, top - ABOVE_HEADING), animated: true });
  });
  effect(() => {
    openPath();
    scroll?.scrollTo({ y: 0, animated: false });
    props.sync?.remeasure();
  });
  // A file just opened takes a moment to measure its sections; in Split, Raw goes to the match on its own.
  effect(() => {
    const at = matchesInFront()[currentInFront()];
    if (!at) return;
    const lines = (opened()?.text ?? "").split("\n").length;
    const shown = headings();
    let tries = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const go = () => {
      const y = pageOffset(at.startLine, shown, lines, tops, columnHeight);
      if (y === undefined) {
        if (++tries < MEASURE_TRIES) timer = setTimeout(go, MEASURE_WAIT);
        return;
      }
      props.sync?.pause(HOLD_AFTER_GOING);
      scroll?.scrollTo({ y: Math.max(0, y - ABOVE_MATCH), animated: true });
    };
    timer = setTimeout(go, 0);
    return () => clearTimeout(timer);
  });
  effect(() => () => props.sync?.page(undefined));

  const follow = (y: number) => {
    if (Date.now() < holdUntil) return;
    let section = 0;
    tops.forEach((top, index) => {
      if (top <= y + ABOVE_HEADING * 2) section = index;
    });
    reading.set(section - 1);
  };

  return (
    <View class="flex-1 flex-row">
      <View class="flex-1" onLayout={(event) => track.set(event.bounds.size.height)}>
        <Scroll
          ref={(ref) => {
            scroll = ref;
            props.sync?.page(ref ? { scroll: ref, headingTops: () => headings().map((_, index) => (tops[index + 1] === undefined ? undefined : tops[index + 1] - ABOVE_HEADING)) } : undefined);
          }}
          testID="rendered"
          class="flex-1"
          // Every frame, so Raw follows the page in step.
          scrollEventThrottle={16}
          onScroll={(event) => {
            follow(event.offset.y);
            props.sync?.pageScrolled(event.offset.y, event.contentSize.height, event.size.height);
            inView.set({ top: event.offset.y, height: event.size.height });
          }}
        >
          <View
            class="w-full max-w-3xl self-center px-8 pb-24 pt-6"
            onLayout={(event) => {
              columnHeight = event.bounds.size.height;
              measured.set(untracked(measured) + 1);
            }}
          >
            <Each of={sections()} key={(section) => String(section.index)}>
              {(section) => (
                <View
                  onLayout={(event) => {
                    tops[section().index] = event.bounds.origin.y;
                    props.sync?.remeasure();
                    measured.set(untracked(measured) + 1);
                  }}
                >
                  <TextView
                    document={section().document}
                    classes={PAGE}
                    selectable
                    links="report"
                    codeBlockActions={[{ id: "copy", label: "Copy code" }]}
                    renderCustom={(node) => (node.name === "frontmatter" ? <FrontMatter node={node} /> : <Text class="text-muted-foreground">{node.text ?? ""}</Text>)}
                    onLinkPress={(event) => followLink(event.url, anchors(), event.modifiers.control)}
                  />
                </View>
              )}
            </Each>
          </View>
        </Scroll>
        <PageMarks marks={barMarks()} />
      </View>
      <If when={mapOpen()}>
        <FileMap
          label={`Map of ${openPath()}, as the page shows it`}
          testID="page-map"
          picture={picture()}
          scale={(height) => Math.min(PAGE_SCALE_MOST, height / Math.max(1, picture().extent))}
          view={inView() ?? { top: 0, height: track() }}
          matches={matchPlaces()}
          preview={preview}
          onGo={(unit) => scroll?.scrollTo({ y: Math.max(0, unit - track() / 2), animated: false })}
        />
      </If>
    </View>
  );
}

type BarMarks = { headings: number[]; matches: number[]; section: { top: number; height: number } | null };

/** Lies over the scroll bar taking no presses, so the bar still drags under it. */
function PageMarks(props: { marks: BarMarks }) {
  return (
    <View testID="page-marks" class="absolute inset-y-0 right-0 w-3.5" style={{ pointerEvents: "none" }}>
      <If when={props.marks.section}>{(section) => <View class={`absolute right-0 w-0.5 rounded-full ${MARKS.section}`} style={{ top: section().top, height: section().height }} />}</If>
      <Each of={props.marks.headings.map((top, index) => ({ top, index }))} key={(mark) => String(mark.index)}>
        {(mark) => <View class={`absolute left-0.5 h-0.5 w-1.5 ${MARKS.heading}`} style={{ top: mark().top }} />}
      </Each>
      <Each of={props.marks.matches.map((top, index) => ({ top, index }))} key={(mark) => String(mark.index)}>
        {(mark) => <View class="absolute h-1 w-1.5 rounded-sm" style={{ top: mark().top, left: 6, backgroundColor: look().found.mark }} />}
      </Each>
    </View>
  );
}

/** The width of the outline beside the file, in points. */
export const OUTLINE_WIDTH = 240;

/** The outline beside the file, or the results' files while their tab is in front. */
export function Outline(props: { onHide: () => void }) {
  const empty = () => (openPath() === "" ? "No file open." : opened() ? "No headings in this file." : "");
  const results = () => resultsTab() && resultsInFront();
  return (
    <Island kind="tool" style={{ width: OUTLINE_WIDTH }}>
      <PaneHeader title={results() ? "In these results" : "Outline"}>
        <IconButton icon="panel-right-close" label="Hide outline" size="small" onPress={props.onHide} />
      </PaneHeader>
      <If when={!results()} else={<ResultsFiles />}>
        <If when={headings().length > 0} else={<Text class="px-3 pt-3 text-xs text-muted-foreground">{empty()}</Text>}>
          <Scroll testID="outline" class="flex-1" contentClass="px-2 pb-4 pt-2">
            <OutlineList lines={outlineLines()} current={reading()} onSelect={goToHeading} />
          </Scroll>
        </If>
      </If>
    </Island>
  );
}

/** The file in front, in its tab's mode, or why there is nothing to show. */
export function Reader(props: { rail: boolean; onRemove: () => void }) {
  const problem = () => listing()?.problem ?? "";
  const files = () => listing()?.files.length ?? 0;
  return (
    <If
      when={problem() === ""}
      else={<EmptyState icon="folder-open" title="This folder isn't there" text={problem()} action={<Button label="Remove from projects" onPress={props.onRemove} />} />}
    >
      <If when={listing()} else={<Spinner label="Listing the files" />}>
        <If
          when={files() > 0}
          else={<EmptyState icon="file-text" title="No Markdown files here yet" text="This folder and the folders under it hold no .md files. Add one, and it shows here by itself." />}
        >
          <If when={openPath() !== ""} else={<EmptyState icon="file-text" title="No file open" text="Open a file from the files on the left. Ctrl+Shift+T opens the one closed last." />}>
            <If when={opened()} else={<Spinner label="Reading the file" />}>
              {(file) => (
                <If when={file().problem === ""} else={<EmptyState icon="file-text" title="This file can't be shown" text={file().problem} />}>
                  <View class="min-h-0 min-w-0 flex-1 flex-row">
                    <View class="min-w-0 flex-1">
                      <If when={mode() !== "split"} else={<SplitView />}>
                        <If when={mode() === "raw"} else={<RenderedView />}>
                          <RawView />
                        </If>
                      </If>
                    </View>
                    <If when={props.rail && headings().length > 0}>
                      <HeadingRail />
                    </If>
                  </View>
                </If>
              )}
            </If>
          </If>
        </If>
      </If>
    </If>
  );
}

function SplitView() {
  const sync = createScrollSync(() => scrollTogether());
  // Both views go to the heading on their own.
  goesToHeadings(() => sync.pause(HOLD_AFTER_GOING));
  effect(() => {
    if (scrollTogether()) sync.align();
  });
  effect(() => () => sync.stop());
  return (
    <ResizablePanelGroup axis="horizontal" class="flex-1" handleClass={`h-full w-px ${look().edges.panel}`} handleAccessibilityLabel="Resize the raw and rendered views">
      <ResizablePanel minSize={SIDE_MIN_WIDTH}>
        <RawView sync={sync} />
      </ResizablePanel>
      <ResizablePanel minSize={SIDE_MIN_WIDTH}>
        <RenderedView sync={sync} />
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
