<p>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="../../assets/brand/mdviewer/logo/md-viewer-horizontal-inverse.svg">
    <img src="../../assets/brand/mdviewer/logo/md-viewer-horizontal-color.svg" alt="Markdown Viewer" width="320">
  </picture>
</p>

A desktop reader for folders of Markdown: notes, documentation, a repository's READMEs.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../../assets/screenshots/mdviewer-dark.png">
  <img src="../../assets/screenshots/mdviewer-light.png" alt="Markdown Viewer showing a project's sensor guide raw and rendered side by side, each with a map of the whole file, between the project's files on the left and the outline on the right">
</picture>

Markdown Viewer opens folders as projects and reads their Markdown files in tabs, each raw, rendered or both side by
side, with its outline, a map of the whole file, and a search across the project. It reads and doesn't edit
([Not yet](#not-yet)). Its logo says "MD Viewer"; the folder, the crates and the nsh package are `mdviewer`.

It's two crates: `app`, the nsh app, and `core` (`mdviewer-core`), its plain Rust. What it shares with the suite's other
products is in [`crates/README.md`](../README.md), and setting up a machine in the [root README](../../README.md#setup).

## Run it

From `crates/mdviewer/app`:

```powershell
nsh run       # a window, with hot reload
nsh test      # the crate's `cargo test`, then the *.test.ts(x) files headless
nsh check     # cargo check, then TypeScript's type check
nsh package   # .nsh\out\mdviewer\
```

Or `cargo mdviewer` and `cargo package-mdviewer` from the repository root. Only one `nsh run` goes at a time: starting
this one ends IP Camera's, and the other way round.

## The window

Left to right:

- **The sidebar**: New, the projects (newest first) and Settings. A project's `…` or right-click menu opens it, shows
  its folder in Explorer, or removes it from the list, leaving its files.
- **The header** (the title bar): the file's name, project and folder, the [search](#search), the file's mode (Raw,
  Split or Rendered), the theme menu and full screen.
- **The files panel**, **the tabs** and **the breadcrumbs** over **the file**, and **the outline**. Drag the gap beside
  the files panel to resize it; it and the outline hide from a button in their headers. A narrow window hides the
  sidebar, and one too narrow for all three panes hides the outline first, then the files panel. Full screen keeps the
  file and its outline.
- **The status bar**: the word count and reading time, the headings, the project's file count, what the search found,
  and in Raw, where the caret is.

## Keys

| Keys | What they do |
| --- | --- |
| Ctrl+B | Hides or shows the sidebar |
| Ctrl+E | Steps the file in front through Raw, Split and Rendered |
| Ctrl+W | Closes the tab in front, pinned or not, or the results' tab |
| Ctrl+Shift+T | Reopens the last closed tab |
| Ctrl+Tab, Ctrl+Shift+Tab | The next tab and the one before, wrapping around |
| Ctrl+K, Ctrl+F | Open the search's drop-down |
| F3, Shift+F3 | The next match and the one before, across files |
| F11 | Full screen; Escape leaves it |
| Return, in the files panel | Opens the file kept, as a double press does |
| Ctrl+click, Shift+click, in the files panel | Picks another file, or every file shown from the last one pressed |
| Ctrl+click, on a link | Opens its file kept, in a tab of its own |

## Projects

A project is a folder, opened from New with Choose folder or by dropping it on the window. A folder already listed
moves to the top and opens. The open project and each project's tabs are saved, so the window reopens where it was
left.

A project's files are every `.md`, `.markdown`, `.mdown`, `.mkd`, `.mkdn` and `.mdx` file in the folder and its
subfolders. Hidden folders (`.git`), `node_modules`, `target` and links to other folders are skipped, and the list stops
at 5,000 files and says so.

The folder is listed again every two seconds and the file in front checked every second, so changes made in other
programs show up by themselves, and a tab whose file was deleted closes. A project with no tabs opens its README, else
its first file. A folder that's gone says so, with a button that removes the project.

## The files panel

The button at the left of its header picks the view, which is saved:

- **Tree** (the default): the project's folder, with its folders nested as on disk.
- **Folder**: a heading for each folder that holds files, with its files.
- **List**: every file by name, with its folder beside it.

The filter lists the files whose path contains what you typed, and the file in front is shown in the panel. A press
opens a file as the preview ([Tabs](#tabs)); a double press or Return opens it kept. With several picked, a bar at the
bottom of the panel or a right click opens them all in tabs; otherwise a file's right-click menu opens it, opens it in
a new tab, or shows it in Explorer. Shift with an arrow doesn't pick a range.

## Tabs

A new tab opens after the one in front, with pinned tabs first. The tabs are saved with the project: which are pinned,
the preview, and how each shows its file.

- **The preview**: a single press on a file (in the panel, or a link) replaces the preview tab, its name in italics. A
  double press on the file or the tab keeps it, as do pinning and its menu's Keep open.
- **Pinning**: the pin beside a tab's × pins it to the start of the strip, where it stays open when the others close.
- **Moving**: drag a tab along the strip, among the pinned tabs or among the others.
- **Its menu**: Keep open, Pin or Unpin, Close, Close others, Close to the right, Close all but pinned, Show in files
  and Show in Explorer.
- **Tabs that don't fit** scroll in one row (Scroll, the default) or wrap onto more rows (Wrap), as
  [Settings](#settings) says.

Each tab shows its file in its own mode, picked from the header; a new tab takes Settings'. Two files with the same
name show their folder in the tooltip. A middle click doesn't close a tab, since nsh's presses don't say which button
was pressed.

The breadcrumbs under the tabs name the project, each folder, the file and the heading the reader is under; a press on
one lists its files or the file's headings, to go to one.

## Raw, Split and Rendered

- **Raw** is nsh's code editor, read-only, with line numbers and soft wrap, in the theme's syntax colors. Each heading
  folds the text under it.
- **Rendered** is nsh's `TextView`: CommonMark and GitHub's Markdown (tables, task lists, footnotes, alerts), inline
  HTML, and code blocks with a copy button. Front matter shows as a box of its own, and a picture's relative path is
  resolved from its file's folder. A link to a heading goes there, one to another Markdown file in the project opens
  it, and any other opens outside the app. Formulas show as their TeX source.
- **Split** is both side by side; drag the line between them to share the width. While the header's toggle is on (on
  at first, and not saved), the side you scroll leads and the other follows, meeting at every top-level heading.

## Search

The box in the title bar shows the search and where the reader is among the matches (`2 / 12`); its × ends the search.
Pressing it, or Ctrl+K or Ctrl+F, opens the drop-down:

- the field, with Match case, Whole words and Regular expression (Rust's `regex` syntax);
- where to look: This file, Open tabs, Project (the default) or All projects;
- the first 500 matches, by file, each with its line number and the words around it.

Up and Down move through the matches, Return goes to one, Ctrl+Return shows them in the files panel, Tab moves on to
the buttons and the scope, and Escape closes the drop-down, keeping the search. Moving over a match doesn't open its
file.

It finds what Raw's own search finds: case-insensitive unless asked, by Unicode case folding (`É` finds `é`), with `^`
and `$` at each line. It runs off the main thread, stops at 10,000 matches, and searches a changed file again within
two seconds. An invalid pattern says why.

While the search has a query, the files panel lists the matches by file. Each file's count shows in the panel and its
tab, and each heading's in the outline; Raw and Rendered highlight the matches, with ticks on the scroll bar; the
status bar says "Match 2 of 12 · 4 files". F3 and Shift+F3 step through the open project's matches, across files and
wrapping around; a match in another project (All projects) opens that project.

Open in a tab, in the drop-down or the files panel's header, gives the results a tab of their own: each file's matches
with two lines above and below, a press going to the match. The tab isn't saved, follows the search, and goes when the
search ends. Past the first 30 files they start folded, and a file shows its first 50 blocks of lines.

Rendered doesn't highlight a match that crosses formatting or sits in code, a link's target or the front matter, and it
scrolls near a match rather than exactly to it. Raw and the counts have every match.

## The outline

The file's top-level headings, indented by level, with the one the reader is under marked: by the caret in Raw, by the
scroll in Rendered. A heading inside a quote or a list isn't a section, and neither is front matter.

## The scroll bars and the heading rail

Each view's scroll bar marks each heading, each search match and the reader's current section. While the outline is
hidden, the heading rail stands in for it at the reader's right edge: a line for each heading, and resting the pointer
on it lists them, to go to one.

## The map

Beside each view, and each side of Split, a map draws the whole file in miniature: Raw's lines as written, or
Rendered's blocks as shapes. The top two levels of headings are named, each match is marked, and a band shows the part
in view. A press or a drag on the map takes the view there, and resting the pointer shows that part larger. The
header's map button shows and hides it, and the choice is saved.

## Settings

Settings opens in a dialog over the window, on the section you left while the app runs. Each setting applies and saves
as it changes.

| Section | Settings |
| --- | --- |
| **Files** | How a file opened in a new tab shows (Raw, Split or Rendered), and under **Tabs**, Scroll or Wrap for tabs that don't fit |
| **Appearance** | The mode (Light, Dark, or following the system), the texture, the elevation and a theme for each appearance |

## What it saves

Two files beside the executable, never in `%APPDATA%`:

| File | What it holds | Its code |
| --- | --- | --- |
| `settings.json` | The appearance, the panels' widths and whether they show, whether the map shows, the files panel's view, how a new tab shows, `tab_overflow` (`scroll` or `wrap`), and the window's place (`ui` and `window`) | `crates/mdviewer/core/src/config.rs` |
| `projects.json` | The projects (each folder's path, its tabs in order with their file, pin, preview and mode, and the file in front) and the open one (`projects` and `selected`) | `crates/mdviewer/core/src/projects.rs` |

Both are written from defaults on first launch and can be edited by hand. They're saved atomically, never over a file
that couldn't be read. The Markdown files are only read. Under `nsh run` the two are in
`crates\mdviewer\app\target\debug\.nsh\` ([why](../README.md#workarounds-for-nsh-061)); `nsh test` uses a throwaway
folder of its own.

## Releases

Tags are `mdviewer-v<version>`, matching `version` in `crates/mdviewer/app/nsh.json`:

```powershell
git tag mdviewer-v0.1.0
git push origin mdviewer-v0.1.0
```

Then in the Actions tab, pick Release, press "Run workflow" and choose the tag under "Use workflow from"
([`crates/README.md`](../README.md#releases) has the rest). The archive, `mdviewer-windows-x86_64-v<version>.zip`,
holds `mdviewer.exe` with the `nsh` runtime folder beside it, `LICENSE.txt` and `THIRD-PARTY-NOTICES.txt`.

## How the code is organized

In `app/`, `App.tsx` is the shell, `views/model.ts` what every region reads, `views/actions.ts` what buttons, rows,
tabs, keys, menus and links do, and `src/bridge.rs` everything TSX imports from `mdviewer/native`. Each region has its
view in `views/` (`files.tsx`, `tabstrip.tsx`, `reader.tsx`, `searchbar.tsx`, `results.tsx`, `rail.tsx`, `map.tsx`,
...), with what it works out beside it as plain TS (`tree.ts`, `tabs.ts`, `document.ts`, `search.ts`, `minimap.ts`,
`scrollsync.ts`, `syntax.ts`). `crates/mdviewer/core/src` holds the two saved files (`config.rs`, `projects.rs`,
`saved.rs`), a project's files (`files.rs`), a file's outline (`outline.rs`), Raw's syntax marks (`syntax.rs`) and the
search (`search.rs`).

`nsh test` runs `App.test.tsx`, which drops `assets/screenshots/projects/Weathervane` on New and walks through the app,
and `Tabs`, `Tree`, `Document`, `Syntax`, `ScrollSync`, `Search` and `Minimap` (`.test.ts`). How the views look,
scrolling to a place and dragging tabs need a look by hand.

## Not yet

- No editing: Raw is read-only, so the search doesn't replace.
- A tab brought back to the front starts at its top rather than where it was left.
- The tab strip doesn't scroll by itself while a tab is dragged toward its end, and in Wrap a tab can't be dragged to
  another row.
