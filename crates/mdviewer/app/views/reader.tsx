// The file in front, raw, rendered or both, and its outline, which shows
// beside it.
//
// Raw is nsh's code editor, read-only, with its Markdown highlighting and the
// theme's syntax colors over it (`syntax.ts`), line numbers, soft wrap and a
// fold at each heading (`foldRanges`). Rendered is nsh's `TextView`, in the
// same colors, a section per top-level heading (`splitSections`), each in
// a view whose place `onLayout` keeps, so the outline can scroll to it and the
// scroll can say which heading the reader is under. Split is both, side by
// side, scrolling together while the header's toggle says so
// (`scrollsync.ts`); a heading pressed in the outline goes there in both. Each
// tab shows its file its own way (`mode`); a tab brought to the front starts at
// its top.
//
// While the title bar's search finds something here (`search.ts`), Raw's editor
// highlights its matches itself, with a mark beside each line that holds one
// and a tick on the ruler along its edge; the page highlights them in its own
// text. The match F3 steps to is selected in Raw, and the page scrolls to about
// where it is: `TextView` doesn't say where a piece of its text is drawn, so the
// page goes as far into the match's section as its line is into the section's
// lines. The outline counts the matches under each heading.
//
// Each view's scroll bar carries the whole file's shape (`minimap.ts`): a dash
// for each heading, a tick for each match, a line along the section the reader
// is in. Raw's is its editor's own ruler; Rendered's marks are drawn over its
// bar (`PageMarks`). While the outline is hidden, the heading rail
// (`rail.tsx`) stands in for it at the reader's edge. While the map shows (the
// header's button), each view has its map beside it (`map.tsx`): Raw's of its
// lines, Rendered's of its page's blocks, with the part in view, which a press
// or a drag on the map moves.
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

/** The narrowest either side of Split gets, in points. */
const SIDE_MIN_WIDTH = 240;

/** How long the views leave the reader's place alone after the outline moved it, in milliseconds: the scroll's own motion. */
const HOLD_AFTER_GOING = 800;

/** Room above a heading the rendered view scrolls to, in points. */
const ABOVE_HEADING = 12;

/** Room above the place of a match the rendered view scrolls to, in points: a third of a short window, so the match shows with what leads to it. */
const ABOVE_MATCH = 160;

/**
 * How often Raw's map asks the editor which lines are in view, in
 * milliseconds: while the editor can scroll (the pointer over it or the map,
 * the keyboard in it), and otherwise. A workaround: nsh 0.5.1's `Editor`
 * reports no scrolling; an `onScroll` of its own in a later release replaces it.
 */
const MAP_VIEW_CHECK_ACTIVE = 50;
const MAP_VIEW_CHECK_IDLE = 500;

/** The most points a line of Raw takes on its map, and a point of the page on Rendered's: a short file is drawn at this size rather than stretched. */
const RAW_ROW_MOST = 3;
const PAGE_SCALE_MOST = 0.2;

/** How the map's preview says where a block is: the heading it is under, and its first line. */
function crumbOf(heading: number, first: number): string {
  const title = headings()[heading]?.title;
  return title ? `${title} · line ${first + 1}` : `Line ${first + 1}`;
}

/** How often, and how long apart in milliseconds, the rendered view looks for a match's section to be measured before it scrolls there. */
const MEASURE_TRIES = 20;
const MEASURE_WAIT = 50;

/** The front matter, as the page shows it: the file's data, quietly, in a box of its own. */
function FrontMatter(props: { node: TextViewCustomNode }) {
  return (
    <View class="mb-4 rounded-md bg-muted px-3 py-2">
      <Text class="font-mono text-xs text-muted-foreground">{props.node.text ?? ""}</Text>
    </View>
  );
}

/** The file's text, as written: highlighted, numbered, folding at each heading. In Split, `sync` keeps it with the page. */
function RawView(props: { sync?: ScrollSync }) {
  let editor: EditorRef | undefined;
  /** Whether the pointer is over the editor, and whether it has the keyboard: while either, it can be scrolled. */
  let hovered = false;
  let focused = false;
  const active = () => props.sync?.rawActive(hovered || focused);
  const text = () => opened()?.text ?? "";
  const folds = computed(() => foldRanges(headings(), text().split("\n")));
  /** The file's colors, which change only with the file. */
  const colors = computed(() => decorationsOf(opened()?.marks ?? []));
  const decorations = computed(() => [...colors(), ...matchMarks(matchesInFront(), look().found.mark), ...rulerMarks(headings(), reading(), text().split("\n").length)]);
  /** What the editor highlights: the search's query, read as the project's search reads it. */
  const search = computed(() => {
    const query = searchQuery();
    return query ? { query: query.text, caseSensitive: query.caseSensitive, wholeWord: query.wholeWord, regex: query.regex } : null;
  });

  /** The map's picture of the file, and the lines in view, which the editor says when asked (it reports no scrolling). */
  const picture = computed(() => rawMap(text(), opened()?.marks ?? [], headings()));
  const inView = signal<{ top: number; height: number } | null>(null);
  const readView = async () => {
    const range = await editor?.visibleRange();
    if (range) inView.set({ top: range.start.line, height: range.end.line - range.start.line + 1 });
  };
  // While the map shows, the lines in view are read often while the editor can scroll (the pointer over it or the
  // map, or the keyboard in it), and now and then otherwise, which a jump to a heading or a match needs.
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
  /** The map's preview of line `unit`: its block of lines, as written, under its heading. */
  const preview = (unit: number): MapPreview => {
    const block = blockAround(lines(), Math.floor(unit), headings());
    return { crumb: crumbOf(block.heading, block.first), text: block.lines.join("\n"), page: false };
  };

  goesToHeadings((index) => {
    const heading = headings()[index];
    if (heading) editor?.goToLine(heading.line);
  });
  // The match F3 stepped to, selected, which makes it the editor's current one, in the middle of the view. After
  // the editor took the file's text, which comes in the same turn when the step opened another file.
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
            // A press in the text that lands on a match makes it the one F3 steps from.
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
              // In Split, the page comes along.
              props.sync?.align();
            }}
          />
        </View>
      </If>
    </View>
  );
}

/** The page the file makes: a section per heading, as wide as text reads well. In Split, `sync` keeps it with the raw view. */
function RenderedView(props: { sync?: ScrollSync }) {
  let scroll: ScrollRef | undefined;
  /** Where each section starts in the column, in points, as its layout last said. */
  const tops: number[] = [];
  let holdUntil = 0;

  /** How tall the column of sections is, in points: where the last section ends. */
  let columnHeight = 0;
  /** Counts each time the sections or the column lay out, so the scroll bar's marks follow where they are. */
  const measured = signal(0);
  /** How tall the view is, and so its scroll bar. */
  const track = signal(0);

  const parsed = computed<TextViewDocument>(() => {
    const folder = projectPath();
    const file = openPath();
    const document = TextView.parse(opened()?.text ?? "", MARKDOWN);
    return folder === "" ? document : withPictures(document, (src) => pictureSource(folder, file, src));
  });
  const pattern = computed(() => pagePattern(searchQuery()));
  /** The match the reader is on, as the page has it: its section, and which of that section's highlights it is. */
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
  /** Where each match is down the page, in points, as far into its section as its line is. */
  const matchPlaces = computed(() => {
    measured();
    return matchesInFront().flatMap((each) => {
      const place = pageOffset(each.startLine, headings(), lines().length, tops, columnHeight);
      return place === undefined ? [] : [place];
    });
  });
  /** The scroll bar's marks: each heading's section top, each match's place, and the section the reader is in, on the bar. */
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
  /** The map's picture of the page, drawn again as the page lays out. */
  const picture = computed(() => {
    measured();
    return pageMap(pageSections(), tops, columnHeight);
  });
  /** The part of the page in view, in points down it, as it scrolls. */
  const inView = signal<{ top: number; height: number } | null>(null);
  /** The map's preview of the place `unit` points down the page: its block of lines, as the page shows them, under its heading. */
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
  // Another file brought to the front starts at its top.
  effect(() => {
    openPath();
    scroll?.scrollTo({ y: 0, animated: false });
    props.sync?.remeasure();
  });
  // The match F3 stepped to: about where its line is, once its section is measured, which a file just opened takes
  // a moment for. In Split, Raw goes to it on its own, so the page leaves the scroll sync be meanwhile.
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

  /** The heading over the top of the view, as it scrolls: the last whose section starts above it. */
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
            // A heading's place on the page, as a scroll offset: the top of its section, with the room the outline leaves above it.
            props.sync?.page(ref ? { scroll: ref, headingTops: () => headings().map((_, index) => (tops[index + 1] === undefined ? undefined : tops[index + 1] - ABOVE_HEADING)) } : undefined);
          }}
          testID="rendered"
          class="flex-1"
          // Every frame, so the raw view follows the page as smoothly as the page follows it.
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

/** Where the page's scroll bar's marks go, in points down the bar. */
type BarMarks = { headings: number[]; matches: number[]; section: { top: number; height: number } | null };

/**
 * The page's scroll bar's marks, as Raw's ruler has them: a dash for each
 * heading on the left, a tick for each match in the middle, and a line along
 * the section the reader is in on the right. They lie over the bar (the
 * suite's, 14 points wide at the view's right edge, `SCROLLBARS`) and take no
 * presses, so the bar drags under them as ever.
 */
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

/**
 * The outline beside the file, a tool pane's island: its top-level headings,
 * the one the reader is under marked, each with how many matches of the search
 * are under it, under a header as tall as the tabs, whose button hides it
 * (`onHide`).
 */
export function Outline(props: { onHide: () => void }) {
  const empty = () => (openPath() === "" ? "No file open." : opened() ? "No headings in this file." : "");
  /** While the search's results are the tab in front, the pane lists their files instead. */
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

/** The file in front, in its tab's mode; or why there is nothing to show. With `rail`, the heading rail stands at its right edge, for the outline that doesn't show. */
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

/**
 * Raw on the left and rendered on the right, the line between them dragged to
 * share the width, scrolling together while the header's toggle says so
 * (`scrollTogether`). While the outline takes both to a heading, they are left
 * to get there on their own.
 */
function SplitView() {
  const sync = createScrollSync(() => scrollTogether());
  goesToHeadings(() => sync.pause(HOLD_AFTER_GOING));
  // Turned on, the page comes to where the raw view is.
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
