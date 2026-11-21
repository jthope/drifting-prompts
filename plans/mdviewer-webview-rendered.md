# Plan: Markdown Viewer's Rendered view on a WebView

A working plan, not documentation: delete it when the work is done. Written 2026-11-20.

## Goal

Replace nsh's `TextView` in Markdown Viewer's Rendered view (and Split's rendered side) with nsh's own `WebView`
showing HTML that Rust makes from the file. The owner wants full control of how the page looks, and every `TextView`
workaround in the Rendered view comes from one gap: `TextView` can't say where it drew something. A page in a web
view can.

Raw, the outline, the files panel, the tabs and the search itself (Rust's, in `mdviewer-core`) stay as they are.

## Why this and not something else

Researched 2026-11-20. No native Markdown control fits into nsh: egui_commonmark, egui_markdown, frostmark (iced) and
Zed's GPUI Markdown each draw only inside their own framework, as do the XAML ones. Blitz (DioxusLabs' Rust HTML/CSS
engine) is alpha (0.3.0-alpha.4, May 2026) and would have to be hosted as a native component of our own with our own
input, selection and scrolling: watch it, don't build on it. A WebView is nsh's own component, already linked into
the app (`webview2-com` in `crates/mdviewer/app/.nsh/rust/Cargo.lock`), and nsh draws its menus, dialogs, hover
cards and the scroll-bar marks over it like any other view.

## What it removes

| Today (`TextView`) | With the page |
| --- | --- |
| One `TextView` per top-level heading (`splitSections`), each measured with `onLayout`, so the outline can scroll to a section | One page; every block carries its source lines (`data-sourcepos`), and the page reports where each block is |
| F3 scrolls "as far into the match's section as its line is into the section's lines" (`pageOffset`) | The page scrolls to the match itself |
| A match that crosses formatting, or is in code or a link's target, isn't highlighted (README "Rendered's limits") | The page finds matches in its text across elements and highlights them with the CSS Custom Highlight API |
| Split lines up only at headings | Split can line up at every block |
| The map draws block shapes guessed from `TextView`'s blocks (`pageMap`, `blockWeight`) | The map draws the blocks where the page says they are |
| The look is whatever `TextView`'s parts and classes allow | The look is CSS |

## Shape of the change

```
crates/mdviewer/core/src/render.rs     NEW  the file as an HTML fragment (comrak), with source lines on every block
crates/mdviewer/app/page/              NEW  the page: page.html (the shell), page.css, page.js; plain files, no build step
crates/mdviewer/app/src/page.rs        NEW  the shell embedded with include_str! (as ipcamera-server's player/ and page.rs)
crates/mdviewer/app/src/bridge.rs           OpenFile gains the page's HTML; Page.shell(); Documents.picture()
crates/mdviewer/app/views/page.tsx     NEW  PageView: the WebView, the messages both ways, a PageRef for the reader
crates/mdviewer/app/views/pagemessages.ts NEW  the messages' types and the pure functions over them (tested)
crates/mdviewer/app/views/reader.tsx        RenderedView on PageView
crates/mdviewer/app/views/search.ts         the page half: the query goes to the page; highlighted/onThePage/pageOffset go
crates/mdviewer/app/views/minimap.ts        pageMap from the page's block boxes; blockWeight and the TextView walk go
crates/mdviewer/app/views/scrollsync.ts     PageSide stops being a ScrollRef
crates/mdviewer/app/views/document.ts       splitSections, headingAnchors, withPictures, mapLeaves go; linkTarget stays
crates/mdviewer/app/views/syntax.ts         PAGE goes; its colors become the page's CSS variables
```

### Rust: `mdviewer-core::render`

- **comrak** makes the HTML. It reads GitHub's Markdown (tables, task lists, footnotes, alerts, autolinks,
  strikethrough), front matter, `$…$` and `$$…$$` formulas, and gives headings GitHub's ids (`header_ids`).
  `render.sourcepos` puts `data-sourcepos="3:1-5:10"` (1-based lines) on every block, which is how the outline, F3,
  Split and the map find things on the page. License BSD-2-Clause, on `.github/notices/about.toml`'s accepted list.
  Declare it once in the root `[workspace.dependencies]` with a comment saying why it's there, as the root's rules
  ask, and check its `rust-version` against the root's 1.95 floor before taking it.
- markdown-rs (`markdown` 1.0, MIT) is the fallback if comrak can't do something; pulldown-cmark (already used for
  the outline and Raw's colors) has no source positions in its HTML, so it would mean writing our own HTML writer.
- The outline stays on pulldown-cmark. The two parsers are matched **by line**, not by count: outline heading `i`
  is the page element whose `data-sourcepos` starts on `heading.line + 1`. Test the cases where they could disagree
  (setext headings, a heading in a list or a quote, front matter).
- Front matter becomes a block of its own (`<pre class="front-matter">`), as the `FrontMatter` box shows it today.
- **Raw HTML in a file is untrusted.** Keep GitHub's tag filter (`tagfilter`: `<script>`, `<iframe>`, `<style>` and
  the rest escaped), and the shell's Content-Security-Policy (below) is the real guard.
- Pictures: a relative `src` is checked to stay inside the project (the rule `pictureSource` in `document.ts` has
  today) and left as a project path in a `data-src` attribute; the page asks for it (below). A `src` with a scheme
  stays as written.
- `render(text) -> String`, called in the bridge's `open_file`, which already runs off the main thread.
- Unit tests at the end of `render.rs`: sourcepos on headings, paragraphs, lists, tables, code blocks and quotes; ids;
  front matter; a `<script>` in the file escaped; a picture leaving the project refused.

### The page (`crates/mdviewer/app/page/`)

- `page.html`: the shell. A CSP of `default-src 'none'; script-src 'nonce-…'; style-src 'nonce-…'; img-src data:
  https:; font-src data:` so nothing in a file can run script (inline handlers like `onclick` are blocked by it too).
  Rust fills in a fresh nonce each time it hands the shell out (`Page.shell()`).
- `page.css`: start from github-markdown-css (MIT, 5.9.0) and keep its structure, but **every color is a CSS
  variable** the app sets from the theme's tokens: no color literals, as the suite's rule for components says
  (`crates/dp/ui/brand.tsx` alone holds them). Fonts: the system's UI and monospace fonts (`system-ui`,
  `ui-monospace`), the column as wide as today (`max-w-3xl`, padding as `RenderedView`'s). The page's own scroll bar
  styled to the suite's (`SCROLLBARS` in `crates/dp/ui/scrollbar.ts`: always shown, 14 points wide, 10-point thumb)
  with `::-webkit-scrollbar`.
- `page.js`: plain JS, no build step, as small as it can be. Everything it does is a message (next section).
- The WebView: `incognito` (no website data kept), `javaScriptEnabled` (our script only, per the CSP), the shell
  loaded once through `html`, and every file, theme and query after that sent with `postMessage`, so changing tabs
  doesn't reload the page or flash.

### The messages (`views/pagemessages.ts`)

App → page:

| Message | What the page does |
| --- | --- |
| `show { html, top }` | Replaces the content, scrolls to `top`, reports `layout` |
| `theme { vars, dark }` | Sets the CSS variables (from `themeColors` in `crates/dp/ui/brand.tsx`) |
| `find { source, flags, wholeWord, current }` | Highlights matches; `current` is `{ line, nth }`: the `nth` match in the block holding source line `line` |
| `scrollTo { y, animated }`, `goToLine { line }`, `goToAnchor { id }` | Scrolls |
| `picture { id, data }` | The bytes of a picture it asked for, as a `data:` URL, or a failure |

Page → app:

| Message | What the app does |
| --- | --- |
| `ready` | Sends `show`, `theme`, `find` (also after the page reloads itself) |
| `layout { height, blocks: [{ line, endLine, top, height, kind, depth }] }` | Keeps the tops: the outline, the marks, Split's places, the map |
| `scroll { y, viewport, content }` | `reading`, Split's following, the map's band, as `onScroll` does today |
| `link { url, control }` | `followLink`; a `#anchor` the page couldn't find comes back here |
| `found { count, places }` | The scroll bar's ticks |
| `key { key, control, shift, alt }` | The app's shortcut, as if the window had it (see the gaps) |
| `menu { x, y, link, selection }` | The app's own context menu (`popup` in `views/actions.ts`) |
| `copy { text }` | The app writes the clipboard (Copy code, Copy, Copy link) |
| `wantPicture { id, src }` | `Documents.picture(project, file, src)` |

Keep `pagemessages.ts` pure (types, parsing, the mapping from blocks to map bars and bar marks) so headless tests
cover it: `Page.test.ts`.

### TSX

- `views/page.tsx`: `PageView` wraps the `WebView`, sends and receives the messages above, and hands the reader a
  `PageRef`: `scrollTo`, `blockTops()`, `headingTops()`, `contentHeight()`.
- `reader.tsx`: `RenderedView` drops the `Scroll`, the sections and `onLayout`; `goesToHeadings`, F3, `follow`,
  `PageMarks`, the map and Split use the `PageRef` and the `layout`/`scroll` messages.
- `scrollsync.ts`: `PageSide` becomes `{ scrollTo, headingTops }` instead of a `ScrollRef`. First keep the heading
  places as they are; lining up at every block is phase 3.
- `search.ts`: the page half (`pagePattern` stays, `highlighted`, `onThePage`, `pageOffset` go). The counts and
  Raw's marks are unchanged.
- `minimap.ts`: `pageMap(blocks, end)` from the page's block boxes; `blockWeight`, `lineAtPage` and the TextView walk
  go.
- `map.tsx`'s hover preview keeps its small `TextView` card (a web view in a hover card is too heavy); it's a preview
  of a few lines, and the only `TextView` left.
- `document.ts`: `splitSections`, `headingAnchors`, `withPictures`, `mapLeaves` go; `linkTarget`, `folderOf`,
  `foldRanges`, `headingAt`, `readingMinutes` stay. `followLink` takes the page's ids in place of `anchors`.
- `syntax.ts`: `PAGE` goes; the syntax colors are CSS variables on the page.

### The bridge

- `OpenFile.page: String`, the HTML fragment, made in `open_file` beside the outline.
- `Page.shell() -> String`: the shell with its nonce.
- `Documents.picture(project, file, src) -> Result<String, String>`, async, off the main thread: the picture as a
  `data:` URL, refused outside the project, with a size limit and an error a person can read.
- Every field documented: those docs become the TypeScript declarations'.

## Gaps in nsh 0.6.1 this runs into

Each is a workaround by the project's rules: a comment naming what replaces it, and a line in `crates/README.md`
"Workarounds for nsh 0.6.1".

1. **Pictures.** nsh's `url_scheme` (Rust's `web_view_html(…).url_scheme("…", serve)`) would serve the project's
   pictures to the page, but `nsh-platform-windows` 0.6.1 doesn't register app schemes (nor does `main` today), and
   `html` with `baseUrl` only answers the page's own request. So the page asks for each picture as it comes into
   view and gets it as a `data:` URL (`Documents.picture`). Replaced by `url_scheme` once nsh serves it there.
2. **Keys.** A click on the page gives it the keyboard (nsh's `MoveFocus`), and nsh doesn't forward the page's
   accelerator keys. The page catches the app's shortcuts (Ctrl+W, Ctrl+Tab, Ctrl+Shift+T, Ctrl+K, Ctrl+F, F3,
   Shift+F3, Escape, and whatever `App.tsx` binds), stops the browser's own (F5 and Ctrl+R reload, Ctrl+P, Ctrl+F's
   find bar, Ctrl+plus/minus zoom), and sends the app's as `key`. Copy and select all stay the page's.
3. **The context menu.** WebView2's default menu (Back, Reload, Inspect, Save as) is left on by nsh. The page stops
   `contextmenu` and sends `menu`; the app shows its own (Copy, Copy link, Select all).
4. **Dropping a folder.** Check that a folder dropped on the page still opens as a project (`open_dropped`) rather than
   being navigated to; if WebView2 takes the drop, the page stops `dragover`/`drop`.

## Phases

Each phase ends with its tests passing, clippy and fmt clean, and the docs it changes updated in the same change.
After each, stop and let the owner look at the window.

### Phase 0: spike (decides go or no-go)

Behind a temporary switch (an environment variable `MDVIEWER_PAGE=web` read in `App.tsx` through the bridge, so
nothing in Settings changes), Rendered shows the open file through comrak in a `WebView` with a rough `page.css`.
Links, headings from the outline, pictures and the theme work; search, Split's sync and the map don't yet.
It answers, for the owner to judge side by side with `TextView`:

- Does the page look right with the theme's colors, light and dark, and does the text look as crisp as the rest of
  the app?
- Does it flash on first show or on a tab change (WebView2's background before the page paints)?
- Pictures through `data:` URLs: fine for a few large PNGs?
- Keys, the context menu and a dropped folder, as listed in the gaps.
- The scroll bar styled like the suite's, and `PageMarks` drawn over it.
- How long the first Rendered tab takes to show (WebView2 starts its browser process then).

Memory and frame numbers only if the owner asks (AGENTS.md "Performance rules").

### Phase 1: Rendered on the page

`render.rs` with its tests; the shell, CSS and script; `PageView`; the bridge; the outline going to headings and
following the scroll; links (anchors, files, outside); front matter; pictures; theme changes; Copy code; the context
menu; keys. The switch goes; Rendered and Split's right side are the page. `App.test.tsx`'s Rendered checks that read
the page's contents move to `render.rs`'s tests and `Page.test.ts`; the ones about modes, tabs, the marks and the map
existing stay.

### Phase 2: search on the page

`find` and `found`; highlights across formatting, in code and in link text; the current match stronger; F3 scrolls to
it exactly; ticks on the scroll bar from `found.places`. The README's "Rendered's limits" section mostly goes.

### Phase 3: Split and the map from the page's blocks

Split lines up at every block both sides can place (Raw's lines from `rangeBounds`, the page's from `layout`), not
only at headings; `anchorsOf` already takes any ordered places. The map draws the page's real blocks.

### Phase 4: formulas, code colors, diagrams

- Formulas: KaTeX (MIT), its script and CSS in the shell with the fonts as `data:` URLs in the CSS, or its MathML
  output with no fonts. Today's `TextView` typesets with RaTeX (KaTeX's layout), so KaTeX keeps the look. Use
  `katex.render` (which sets styles through the DOM) rather than `renderToString` into `innerHTML`: the CSP's
  `style-src` without `'unsafe-inline'` blocks the `style="…"` attributes that string carries.
- Code blocks: comrak's syntect highlighting with classes (not inline colors), so the theme's `syntax-*` tokens color
  them; check the `syntect` version against the one nsh-base uses.
- Mermaid diagrams (MIT, large): only if the owner wants them, loaded only when a file has a ```` ```mermaid ```` block.

### Phase 5: clean up and document

- Remove what only `TextView` needed (`document.ts`, `syntax.ts`, `minimap.ts`, `search.ts` pieces above) and their
  tests in `Document.test.ts`, `Minimap.test.ts`, `Search.test.ts`.
- `crates/mdviewer/README.md`: "Rendered", "Rendered's limits", "Split", "The outline", "The scroll bars and the
  heading rail", "The map"; drop the line about formulas showing as TeX (nsh 0.6.1 already typesets them with RaTeX;
  check that by eye first).
- `AGENTS.md`'s layout: `render.rs` in mdviewer-core, `page/`, `src/page.rs`, `views/page.tsx`,
  `views/pagemessages.ts`; and its Documentation table's Markdown Viewer row.
- `crates/README.md` "Workarounds for nsh 0.6.1": the gaps above.
- Licenses: comrak is a crate, so `about.toml` covers it. The page's vendored files (github-markdown-css, KaTeX,
  Mermaid) aren't crates: check how a release ships third-party notices (`crates/README.md` "Releases") and add theirs.

## Working rules that apply (from AGENTS.md)

- Subagents write code and tests but never run `cargo build`, `cargo test`, `cargo clippy`, `nsh run` or `nsh test`;
  the main session runs them, one at a time, from PowerShell.
- Tests to run: `cargo test -p mdviewer-core` for `render.rs`; in `crates/mdviewer/app`, `cargo test` for the bridge
  and `nsh test <file>.test.ts(x)` for the TSX that changed; clippy with `-D warnings` and `cargo fmt --check` where
  the change is (`cargo fmt --all` at the root, `cargo fmt` in the app).
- Nothing that opens a window unless the owner asks: say what's worth a look by hand. Phase 0 is mostly that, so
  ask the owner to run `cargo mdviewer` with `MDVIEWER_PAGE=web`.
- nsh is pinned to 0.6.1: check anything taken from nsh's website against `git -C D:\Development\nsh show
  v0.6.1:<path>` or `~/.cargo/registry/src/*/nsh-*-0.6.1`. The WebView's docs: `website/modules/nsh-webview.md`,
  `website/docs/web-view-and-video.md`; its implementation: `crates/nsh-platform-windows/src/native/web.rs`.
- Everything in American English, no color literals outside `crates/dp/ui`'s brand files, no platform talk in code,
  comments or docs.

## To start the new session

Point it here: "Read `plans/mdviewer-webview-rendered.md` and do Phase 0."
