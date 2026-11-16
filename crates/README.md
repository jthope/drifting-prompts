# The crates

Each product has a folder here. Crates that more than one product uses live in `dp/` as `dp-*` crates; a crate moves
there when a second product needs it, not before. A package name carries its product (`ipcamera-core` in
`crates/ipcamera/core`) because Cargo needs every name in a workspace to differ, and `core` is taken by Rust itself.

| Folder | What it is | Its README |
| --- | --- | --- |
| `ipcamera/` | IP Camera: `core`, `dpapi`, `media`, `storage`, `server` and `app` | [`ipcamera/README.md`](ipcamera/README.md) |
| `mdviewer/` | Markdown Viewer: `core` and `app` | [`mdviewer/README.md`](mdviewer/README.md) |
| `dp/store` | `dp-store`: settings files saved safely, and the lock helper | [below](#dp-store) |
| `dp/ui` | `dp-ui`: the styled TSX controls, the themes, and the Rust under them | [below](#dp-ui) |
| `xtask/` | The repository's own commands, run through cargo | [below](#the-repositorys-commands) |

This README is for working on the suite. It covers:

- [The workspaces](#the-workspaces): what builds where, the repository's commands, the Rust floor, `.cargo/config.toml`
- [Checks and CI](#checks-and-ci) and [Releases](#releases)
- [dp-store](#dp-store) and [dp-ui](#dp-ui), the two shared crates
- The shared look: [Themes](#themes), [Islands](#islands), [Textures](#textures) and [Elevation](#elevation)
- [Workarounds for nsh 0.5.1](#workarounds-for-nsh-051)
- [Brands](#brands) and the READMEs' [Screenshots](#screenshots)

## The workspaces

The repository has four Cargo workspaces. nsh needs the two apps and `dp-ui` to be workspaces of their own, so `cargo`
at the root never builds them.

| Workspace | Crates | Built and tested by |
| --- | --- | --- |
| The root (`Cargo.toml`) | `dp-store`, `ipcamera-core`, `ipcamera-dpapi`, `ipcamera-media`, `ipcamera-storage`, `ipcamera-server`, `mdviewer-core`, `xtask` | `cargo` at the root |
| `crates/ipcamera/app` | IP Camera's app | `nsh` in its folder, and `cargo` there for its Rust |
| `crates/mdviewer/app` | Markdown Viewer's app | `nsh` in its folder, and `cargo` there for its Rust |
| `crates/dp/ui` | `dp-ui` | `cargo` in its folder for its Rust; either app's `nsh check` type checks its TSX |

An app builds through a workspace nsh generates in its `.nsh/rust`, which pulls in `dp-ui` too. Cargo reads profiles
only from the workspace it builds, so a profile meant for the apps goes in `.cargo/config.toml`, not in a
`Cargo.toml`.

### The repository's commands

`crates/xtask` holds the repository's own commands, named after each product, and `.cargo/config.toml` makes them
cargo aliases. It depends on none of the other crates.

| Command | What it does |
| --- | --- |
| `cargo ipcamera`, `cargo mdviewer` | `nsh run` in the app's folder, passing on any arguments |
| `cargo package-ipcamera`, `cargo package-mdviewer` | `nsh package --target windows` in the app's folder |
| `cargo package-ipcamera-web` | `nsh package --target web` in IP Camera's folder |
| `cargo ipcamera-web` | `ipcamera-server` and the web build together ([the server's README](ipcamera/server/README.md)) |
| `cargo screenshots` | The READMEs' screenshots of both apps, light and dark, in real windows ([Screenshots](#screenshots)) |

### The Rust floor

The root workspace needs Rust 1.95, the highest `rust-version` in its dependency graph (glass2glass's `g2g-core` pulls
in `windows` 0.100 through `loom`). The root `Cargo.toml` records it as `rust-version`, and CI checks the workspace
with exactly that version. The nsh workspaces need 1.97, which nsh 0.5.1 declares; `rust-version` in `crates/dp/ui`
and `crates/mdviewer/app` says so, and CI builds them with stable.

Keep the `nsh-*` dependencies at the version of the `nsh` you run (`nsh --version`), and raise them together.

### `.cargo/config.toml`

The root `.cargo/config.toml`, which `nsh run` also reads:

- Optimizes `rust_h264`, `rust_h265` and `g2g-plugins` in the dev profile. Unoptimized, a 1080p H.265 camera falls
  behind.
- Links with `rust-lld.exe`, which every toolchain ships.
- Gives full debug info only to the suite's own crates, listed by name, and line tables to every other crate.
  Backtraces still show where, and each PDB is a quarter to half the size (IP Camera's test runner's is 520 MB rather
  than 1.2 GB). A new crate of ours gets a line there.

## Checks and CI

These are the root workspace's gates, run from the repository root. CI runs the same:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`cargo test -p ipcamera-core` is the fast subset (no media stack, no display, milliseconds); `-p mdviewer-core` and
`-p dp-store` are as fast. The nsh workspaces have their own gates:

| From | Run |
| --- | --- |
| `crates/ipcamera/app`, `crates/mdviewer/app` | `nsh test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` |
| `crates/dp/ui` | `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and either app's `nsh check` for its TSX |

[`ci.yml`](../.github/workflows/ci.yml) runs only by hand for now: in the Actions tab, pick CI, press "Run workflow"
and choose a branch or tag under "Use workflow from". Pushes and pull requests don't start it. It has six jobs:

| Job | Where | What it runs | Why |
| --- | --- | --- | --- |
| Formatting | Linux | `cargo fmt --all --check` | |
| Core | Linux, with no system libraries at all | clippy and tests of `ipcamera-core`, `mdviewer-core` and `dp-store` | keeps them free of the media stack |
| Linux | Linux | clippy and tests of the root workspace | the whole suite of tests |
| Rust 1.95 | Linux | `cargo check` of the root workspace, tests included | the floor holds against the same dependencies |
| Windows | Windows | clippy of the root workspace, without its tests | the media crate uses WASAPI there; the tests already ran on Linux, and linking every test binary again isn't worth the wait |
| The apps and `dp-ui` | Windows, one job each | `cargo fmt --check`, clippy and `cargo test` in `crates/ipcamera/app`, `crates/mdviewer/app` and `crates/dp/ui` | their Rust, which needs no `nsh` on the runner |

`nsh check` and `nsh test` (TypeScript's types and the TSX tests) need `nsh`, so CI doesn't run them; the release
workflow builds both apps whole.

## Releases

[`release.yml`](../.github/workflows/release.yml) packages the apps and publishes them. Like CI, it runs only by hand
for now: in the Actions tab, pick Release, press "Run workflow" and choose a tag or branch under "Use workflow from".
Pushing a tag doesn't start it.

Each product has its own version (`version` in its app's `nsh.json`) and its own tags: `ipcamera-v<version>` and
`mdviewer-v<version>`.

- **A tag** packages that app and publishes it to the tag's release as `<product>-windows-x86_64-v<version>.zip`,
  titled with the product's name and version.
- **A branch** packages both apps as downloads on the run's page
  (`<product>-windows-x86_64-v<version>-<branch>-<commit>.zip`) and publishes nothing. It's a test build.

A run uses `release.yml` as it was at the tag or branch picked, so a fix to the workflow only reaches releases tagged
after it.

The workflow has three jobs, so a build that fails halfway leaves no release at all rather than an empty one:

1. `prepare` reads the product from the tag and checks the tag's version against that app's `nsh.json`. It stops in
   seconds if the tag names no product or the versions disagree.
2. A job for each app being built installs the `nsh` its crates are built against (`nsh-core`'s version in its
   `Cargo.toml`), runs `nsh package --target windows`, and adds the licenses.
3. `publish`, for a tag only, creates the release once the package is built, with that product's notes from the
   workflow.

### What an archive holds

The app's executable, the `nsh` runtime folder beside it (nothing else to install), and two text files:

- `LICENSE.txt`, the repository's MIT license
- `THIRD-PARTY-NOTICES.txt`, the license of every crate compiled into the app, with where to find each crate's source

Nothing is signed, so the system warns that the binary is unsigned.

The packaging job writes the two files after `nsh package` and adds them to nsh's zip. The notices come from
[cargo-about](https://github.com/EmbarkStudios/cargo-about), run on the executable's crate in nsh's generated
workspace (`.nsh/rust/windows-app`), so they list exactly what the release build compiled. Its settings and template
are in `.github/notices/`. `accepted` in `about.toml` lists the licenses a release may ship, and a crate under any
other license stops the release, so check a new dependency's license before adding it there. To see the notices
locally, package the app (`cargo package-ipcamera`), then from the root:

```powershell
cargo install cargo-about --version 0.9.3 --locked --features cli
cargo about generate --config .github/notices/about.toml --manifest-path crates/ipcamera/app/.nsh/rust/windows-app/Cargo.toml --output-file "$env:TEMP\notices.txt" .github/notices/notices.hbs
```

## dp-store

`crates/dp/store` (`dp-store`) saves settings files safely (JSON, write then rename, never over a file it couldn't
read) and holds the lock helper every crate uses. It's plain Rust, in the root workspace.

| File | What it holds |
| --- | --- |
| `src/lib.rs` | Loading and atomic saving for the JSON files (each app's `settings.json`, Markdown Viewer's `projects.json`, IP Camera's `data\settings`). It never overwrites a file it couldn't read, and on Unix it writes a document marked `OWNER_ONLY` (`cameras.json`, for the passwords) readable by its owner alone. For the web preview, it keeps the documents in memory instead |
| `src/sync.rs` | Taking a lock even after a panic in another thread, the same way everywhere; also reachable as `ipcamera_core::sync` |

## dp-ui

`crates/dp/ui` (`dp-ui`) makes the apps look and behave alike.

- **Its TSX**: the styled controls every page is built from (`index.ts`), the themes and their looks (`brand.tsx`),
  the Settings dialog with its Appearance section, and the logos. Each app imports it by path (`../../dp/ui` from
  `App.tsx`, `../../../dp/ui` from `views/`) and names its theme file in `nsh.json`.
- **Its Rust**: the `Themes` module each app registers in its native module; helpers for the bridges' listeners; the
  window's saved look and place (`SavedAppearance`, `WindowPlace`, from `dp-store`'s `Appearance` and `WindowState`,
  which each product's `settings.json` keeps); the folder picker; and showing a file in its folder.

It's built with nsh, so like the apps it's a workspace of its own that `cargo` at the root doesn't build. The paths
below are in `crates/dp/ui`.

### Its entry, the themes and the appearance

| Path | Holds |
| --- | --- |
| `index.ts` | The styled controls every product shares. `nsh/base` brings behavior and accessibility; these bring the look, from the current theme. `App.tsx` hands them the app's `Themes` module first (`connectThemes`) |
| `appearance.tsx` | The Appearance settings every product's Settings shows: the mode, the texture, the elevation, and a dark and a light theme, each a tile drawn in its own colors. Also the title bar's theme menu (`ThemeMenu`), and the app's saved appearance, shown before its first frame (`showAppearance`) and kept as the user changes it, following the system while the mode is System (`keepAppearance`) |
| `brand.tsx` | The themes, the dark and light picks, and what each decides for every themed place (`look()`), with the elevation on top; the card, badge and line drawn from it |
| `theme.json` | Every product's own theme file (`nsh.json` `theme`): the tokens added to nsh's default theme, Default's chart, page and card colors, readable muted text, focus and field outlines, and the tokens the theme files' looks name (`title-bar`, `toolbar-hover`, ...) |
| `themes/` | The other themes, each a theme file like `theme.json`, with a value for every token |
| `texture.ts` | The textures over the page headers, in any theme: their light as gradients, and their dots as a canvas drawing |
| `elevation.ts` | The elevations: the shadows and lights every theme takes, and how each place is raised or pressed in |

### The window's frame

| Path | Holds |
| --- | --- |
| `titlebar.tsx`, `nav.tsx`, `panel.tsx` | The title bar; the sidebar (`SidebarFrame`, the frame's column under the product's logo, name and Hide button), its rows, and a dialog's parts in the same style; a panel's header (`PanelHeader`, on an island with `island`) and a pane's header on its island (`PaneHeader`) |
| `window.ts` | The main window: reopening where it was left (`keepWindowPlace`, with the place each app keeps in its `settings.json` through its bridge's `AppWindow`), and full screen (`isFullScreen`, `toggleFullScreen`, `leaveFullScreen`), both from `nsh/desktop`'s window state |
| `island.tsx` | The window's panes as islands ([Islands](#islands)): `Frame`, the frame's color; `Islands`, where islands sit; `Island`, a pane as a rounded block in a tool pane's or the document's color (`Look.tiers`); `ISLAND_GAP`, the space around one; `GRIP_HANDLE`, a resize handle with no line, in a gap or along the sidebar's edge; and `LINE_HANDLE`, a handle between two parts of a page that scroll together, its line showing under the pointer |
| `edge.tsx` | `ScrollEdge`: where content scrolls under something that stays put (a page's header, the sidebar's cameras heading and pinned rows), with the line, and the elevation's shade or a fade into the surface while content lies beyond it; `EdgedScroll`, a scroll view with one at each end |
| `scrollbar.ts` | `SCROLLBARS`, the scroll bars an app sets for every scroll view (`Scrollbar.setTheme`): always shown while there's more than fits, 14 points wide, in the theme's colors. Markdown Viewer uses them |
| `collapsing.tsx`, `tabs.tsx` | A page header that collapses to its bar as the page scrolls, and the tabs in that bar (`PageTabs`); and a reader's file tabs (`FileTabs`): rounded cards on a strip, each with a pin and a close button, scrolling in one row or wrapping onto more, with breadcrumbs under them |
| `breadcrumbs.tsx` | The path to what a pane shows, one step at a time (`Breadcrumbs`: a reader's project, folders, file and heading), with a chevron between steps and the current place emphasized; a step with a menu opens it on a press |
| `menu.tsx` | Menu items in the shape the system's menus take, the surface of anything floating over the window, and the menus drawn in a browser (`MenuOpener`, `popupMenu`, `MenuLayer`) |

### Dialogs and Settings

| Path | Holds |
| --- | --- |
| `editor.tsx`, `dialog.tsx` | The dialog the editors open in: its header over a line in every theme (`PanelHeader`'s `ruled`), a form's parts down its side, at an editor's size or wide for settings (`size`); and the dialogs that ask for a name or confirm a delete |
| `settings.tsx` | Every product's Settings, in that dialog at its wide size (`SettingsDialog`): its sections down the side, each with its icon, the chosen section's settings beside them, and a footer that says changes are saved, with Done; plus the Appearance section every product lists (`appearanceSection`), over the app's `keepAppearance` |
| `rows.tsx`, `segmented.tsx`, `appear.tsx` | Settings rows in cards, a segmented control, and the fade-up entrance |

### Controls

| Path | Holds |
| --- | --- |
| `button.tsx`, `field.tsx`, `press.tsx` | Buttons by role (`primary`, `secondary`, `ghost`, `danger`, `overlay`), an icon button's label doubling as its tooltip; a label above a control, with help or an error under it; a large pressable area that sinks while held |
| `card.tsx` | The one card every card is (the dashboard's tiles and charts, the settings groups, a page header's banner): one outline, corner and shadow, with the shadow on the rounded view |
| `chart.tsx` | The dashboard's numbers, cards and charts, over nsh's plot, in the theme's chart colors |
| `table.tsx`, `search.tsx` | A page's table (sortable column titles, rows in the theme's hover and selection, group titles that fold their rows, the footer under the rows with its pager, the bar that floats up for a selection) and a search box |
| `tree.tsx`, `count.tsx` | A tree of folders and files in a panel (picking several items; a search's results, with each match's line number and the match marked) and an outline of headings beside a page; the count of a search's matches in a row or a tab (`CountBadge`, on `Look.found`, the theme's `warning`) |
| `reorder.tsx` | Reordering by drag: rows dragged by a grip (the sidebar's cameras), or tabs dragged whole along their row (along their own row when they wrap). The dragged item lifts and follows the pointer while the others slide aside, all on the UI thread |
| `pad.tsx`, `media.tsx`, `empty.tsx` | The PTZ panel's direction pad; a gallery's tile, the labels over a picture and its toolbar (`OverlayToolbar`, with its switch and separator); what a page says while it's empty |
| `status.tsx`, `toast.tsx`, `note.tsx`, `spinner.tsx` | A status and its dot (`StatusItem`, the status bar's items); one-off messages (`notify`, `<Toaster />`); a message with a copy button; a spinning ring for work in progress |
| `icon.tsx`, `lucide.ts`, `logo.tsx` | Icons in the theme's colors, the Lucide icons the suite uses, and a product's logo (`<Logo product="ipcamera" />`) |
| `logos.ts` | The products' logos as SVG markup, in the code so each ships inside its bundle: each product's mark ([Brands](#brands)) in one color, black or white, in any theme |

### Its Rust

| Path | Holds |
| --- | --- |
| `src/lib.rs` | The `Themes` module each app registers in its native module: the theme files laid over `theme.json`; `SavedAppearance`, the window's saved look, which each app's bridge hands to `appearance.tsx`; and `WindowPlace`, where the window was left, which each app's bridge hands to `window.ts` |
| `src/listen.rs` | `listen` and `listen_later`, how the bridges' listeners check for changes: at a stated interval, `listen_later` off the main thread |
| `src/folder_dialog.rs`, `src/explorer.rs` | The folder picker (Win32 and COM calls), and showing a file in its folder |

## Themes

The themes, textures and elevation are the same in every product. Each app's Settings shows them in its Appearance
section (`appearanceSection` in `crates/dp/ui/settings.tsx`, over `AppearanceSettings` in
`crates/dp/ui/appearance.tsx`), so a new theme reaches every product at once. The places named below are IP Camera's;
Markdown Viewer's frame, panels and cards take the themes the same way.

### How a theme is layered

Every theme is light or dark. The app keeps a pick for each appearance and shows the one for the current appearance.
A theme has three layers:

- **The colors** are tokens: the app's own in `crates/dp/ui/theme.json` (Default's, dark and light), with a theme
  file's laid over them ([Theme files](#theme-files)). Classes name them (`bg-title-bar`, `ring-ring`), and
  `Themes.colors` reads them for styles a class can't express.
- **The look** of each theme is a `Look` in `crates/dp/ui/brand.tsx`: for every themed place, the classes it takes
  (written out whole) and the styles a class can't express, such as gradients and colored shadows. The elevation's
  depth goes on top (`raise`). `look()` returns the current look, and `BrandCard`, `BrandBadge` and `EdgeLine` draw from
  it.
- **The theme shown** is `theme` in the same file: `picked.dark` or `picked.light` (set in Settings), for the current
  appearance. A component asks `look()` how to draw and never checks the theme itself, so a new theme reaches every
  place at once.

The shared controls in `crates/dp/ui/` apply the look, so a page built from them is themed with no extra work. To
theme another place, use those controls or ask `look()`, adding a field to `Look` if the place needs one, rather than
writing a theme into the place itself. `settings.json` keeps the picked names. A name this version doesn't know, or
one belonging to the other appearance, falls back to that appearance's Default.

### The theme menu

The title bar has a quicker way to switch themes: `ThemeMenu`, a palette icon beside full screen. Its menu starts with
the mode (Light, Dark or System) as the same segmented control Settings shows, then lists the themes for the
appearance that mode shows (Default, then the theme files, high contrast last), each beside a miniature window in its
colors (`swatch`, as on Settings' tiles), with the current one checked.

The mode there is Settings' mode: the app passes the menu its value and the same `setMode` the Appearance settings use
(`keepAppearance`'s), which saves it. Picking a mode leaves the menu open and switches the list to the new appearance
(for System, the system's appearance, named in the heading). Picking a theme picks it for its own appearance only,
saves it as Settings would, and closes the menu; under System, the other appearance keeps its pick.

The menu is drawn (nsh's `Menu`, as `PopupMenu`) because the system's menus can't show pictures. That also lets it
hold the segmented control: nsh's `Menu` accepts items other than `MenuItem`s, and pressing the control closes
nothing. The list scrolls in a window too short for it. Its trigger is `IconTrigger`, an icon button's look on a plain
view, since a drawn menu's trigger must be a plain view.

### Default

Default uses the app's own theme file, which keeps nsh's flat look and stock tokens except where the suite sets its
own colors: the chart colors (see IP Camera's [Dashboard](ipcamera/app/README.md#dashboard)), the page, cards,
readable muted text, field outlines and focus rings, and these:

| What | Dark | Light | Why |
| --- | --- | --- | --- |
| The page and the frame (the sidebar, title bar and status bar, `sidebar`) | `#171717`, where nsh's page is near-black `#0a0a0a` | `#f1f2f4`, where nsh has a white page and a `#fafafa` frame | the window is one surface around its content |
| Cards and menus | `#222326`, where nsh has `#171717` | nsh's white | they stand off the page by color alone, and a menu opened over a card is never darker than the card |
| A hovered or selected item in a list or menu (`accent`, `sidebar-accent`) | nsh's | `#e8eaed`, where nsh has `#f5f5f5` | darker than the page, so it shows on the frame and the page as well as on a card |
| A button or chip under the pointer | the text's color at 10%, 20% pressed, 15% for a button that's on | the same | it shows on any surface: in the dark, `accent` is the secondary button's own color, so hovering to it would change nothing, and in the light it's only a shade off the page |
| A segmented control's chosen segment | the page's, as in nsh | the card's | the page's gray would vanish on its muted track |
| The line on the chosen tab (`tab-border`, over a reader's file and under a page's tab) | a blue, `#0078d4` | the text's color | white glares; a dark theme file whose line would be near-white has its own accent there too |

The title bar and the panels' headers use the sidebar's color, so the window's frame is one color around the content.
A page with nothing to show has faint rings around its badge.

Default's cards have no outline: its `card-border` is transparent, and a card's border color comes from
`Look.cardEdge`, which is `card-border` in Default and `border` in a theme file, whose cards keep their outlines.
Default draws no page lines either (`edges.page` is transparent in its `Look`): none under a header's bar when content
scrolls beneath it, none over a table's pager bar, none under the PTZ panel's title. The line beside the PTZ panel
stays, since it's the panel's resize handle. A theme file keeps its page line.

### In every theme

- The title bar and status bar have no line along the edge they share with the page; their color separates them from
  it, as the sidebar's does.
- A page's own header (Dashboard, Snapshots, Videos) takes the page's color, so header and page are one surface, with
  the theme's page line under the header's bar.
- No theme has depth of its own: the elevation goes over every theme ([Elevation](#elevation)).
- Hover is neutral in Default; a theme file hovers in its own colors.
- Red means recording.
- The logo is the mark in one color: black on light, white on dark.
- IP Camera uses the system's scroll bars. Markdown Viewer uses `SCROLLBARS`, always shown and in the theme's colors.
- The system draws the window's minimize, maximize and close buttons, its own dialogs (the delete question, the folder
  picker) and the right-click menus, so those follow light and dark but not the theme. In a browser the app draws its
  menus itself, in the theme's colors.

### Theme files

Every other theme is a file in `crates/dp/ui/themes/`, in the same format as `theme.json`: shadcn's `cssVars`, which
nsh reads, with every value under `theme` so light and dark match. Each file sets every token: nsh's stock ones
(`background`, `primary`, `sidebar`, ...) and the ones `theme.json` adds for what those have no name for, such as
`title-bar`, `status-bar`, `toolbar-hover`, `option` (a button that's on), `nav-selection`, `menu-selection`, `field`,
`dropdown` and `state-outline`. `theme.json` gives those its own values (`var(--accent)`, ...), which Default never
names.

A theme file's look (`flatLook` in `crates/dp/ui/brand.tsx`) is flat, with each state in its own token:

- A toolbar button hovers and presses in `toolbar-hover` and `toolbar-active`.
- A button that's on (the PTZ panel's) is a toggled option (`option`, outlined in `option-border`).
- The selected sidebar row is `nav-selection`, and a menu's highlighted item is `menu-selection`, with its text, icons
  and pictures in `menu-selection-foreground`.
- Focus is a one-point outline in `ring`.
- The title bar (with the sidebar's top row) is `title-bar`, the status bar `status-bar`, the PTZ panel the sidebar's
  color, a page header's tabs `tab-*`, and form fields and checkboxes `field` and `checkbox`.
- The high contrast themes outline what's hovered or selected in `state-outline`, which the others leave transparent.

A theme file holds colors only; shadows and lights come from the elevation.

#### Markdown's colors

Markdown Viewer's colors for a file are tokens too, from each theme's own palette: `syntax-heading`, `syntax-link`,
`syntax-code`, `syntax-marker` (list markers, a quote's `>`), `syntax-punctuation` (Markdown's own, quieted),
`syntax-strong`, `syntax-emphasis` and `syntax-quote`. `theme.json` gives Default's for each appearance, mostly in the
hues nsh highlights code with. Headings and code use Dark+'s and Light+'s colors instead of nsh's purple and green: a
blue and a warm red in both appearances.

- Dark: Dark+'s blue (`#569cd6`) and its salmon one step lighter (`#d59282`), which keeps 4.5:1 on code's tint over
  Default's document, which is lighter than Dark+'s page.
- Light: Light+'s blue (`#0070c1`) and its red turned toward rose (`#b4234a`), distinct from the rust of strong text.

A fenced code block keeps nsh's own colors in every theme; nsh 0.5.1 has no way to change them.

#### How a theme reaches the app

nsh reads one theme file when the app is built, so the other themes are applied while the app runs: `Themes.apply` in
`crates/dp/ui/src/lib.rs` (registered in the app's native module) lays the file over `theme.json` and hands nsh the
result (`set_tokens`), and every class that names a token takes the new colors at once. `Themes.apply(null)` restores
the app's own. `Themes.colors` reads any theme's colors; the theme tiles use it so each shows its own colors whatever
the app shows.

#### The checks

- `dp-ui`'s Rust tests fail on a theme file that doesn't parse, leaves out a token, or isn't listed in `THEME_FILES`.
- `crates/ipcamera/app/Brand.test.ts` checks the colors the shared controls and Markdown use against their surfaces
  in every theme: 4.5:1 for text and 3:1 for field outlines and focus rings, including colors under translucent tokens
  and on raised surfaces. Disabled controls are exempt.

Status, error, field and Markdown colors stay within each theme's palette while standing out from the surfaces they
sit on. A destructive button keeps the theme's red but uses whichever of black or white reads best on it, and hover
or press outlines it instead of fading its label.

#### Adding a theme

`THEMES` in `crates/dp/ui/brand.tsx` lists the themes with their names, whether each is light or dark, and which are
high contrast. To add one:

1. Write its file in `crates/dp/ui/themes/`, with every token.
2. Add it to `THEME_FILES` in `crates/dp/ui/src/lib.rs`.
3. Add it to `THEMES` in `crates/dp/ui/brand.tsx`, in its appearance's section.
4. Run `cargo test` in `crates/dp/ui` and `nsh test Brand.test.ts` in `crates/ipcamera/app`.

Every product shows the same themes, so a new theme reaches Markdown Viewer too.

## Islands

A window with panes side by side (Markdown Viewer's files, file and outline; IP Camera's video and PTZ panel) sets
each pane on the frame as an island (`Island` in `crates/dp/ui/island.tsx`, inside `Islands`): a block with rounded
corners, `ISLAND_GAP` (6 points) from its neighbors and from the frame's edges, with a card's outline
(`Look.cardEdge`: none in Default, the theme's `border` in a theme file). The resize handle between two islands sits in
their gap with no line (`GRIP_HANDLE`); a grip shows there under the pointer, as along the sidebar's edge. Pages (IP
Camera's Dashboard and the rest) aren't islands: each is the theme's page. Settings and the editors are dialogs over
the window.

Most themes put their sidebar barely off their page, a contrast of 1.05 to 1.12, too little to tell panes apart
without a line, and some share one color for both. So islands stand apart in three tiers instead (`Look.tiers`,
worked out in `brand.tsx` from the theme's own colors, so every theme gets them without a token of its own):

- **The frame**, behind the islands, which the sidebar shares (`Frame`, `SidebarFrame`): the theme's sidebar.
- **A tool pane's island** (the files, the outline, the PTZ panel): halfway between the frame and the document.
- **The document's island** (the file and its tabs, the video): a theme file's page, or Default's card, which stands
  off its frame as its cards do.

The frame and the document are a contrast of 1.3 apart in a dark theme and 1.18 in a light one (`TIERS_APART`), so
each tier shows against the next (1.08 or more in every theme). Where a theme has them closer, one moves toward white
or black by the least amount that works:

- A theme file keeps its page, the color it's known by, and moves its sidebar's color away from it, or past it where
  there's no room on that side (a sidebar darker than a near-black page).
- Default keeps its frame, which matches its title bar and status bar, and moves its card off it, or moves the frame
  where the card has no room (white, in the light).
- The high contrast themes keep their own colors: the sidebar's for the frame, and the page for both islands, which
  their outlines separate.

The title bar and status bar keep the theme's own colors. Inside an island, a pane's header (`PaneHeader`, or
`PanelHeader` with `island`) is the island's color over a panel line, and the file tabs (`FileTabs`) are a strip in the
tool panes' color, so the headers of Markdown Viewer's three islands line up as one row. The front tab is a card in the
document's color with rounded top corners, joined to the breadcrumbs under the strip, which are also the document's
color, over a panel line.

IP Camera's video fills its island to the edge, in an island with no outline (`edgeless` on `Island`): the island clips
it to its rounded corners, live picture included, which Direct2D draws inside the clips of the views around it. In full
screen the video fills the window with no gap, outline or corners (`flat` on `Islands` and `Island`).

## Textures

A texture is a finish drawn over the page headers' banners (Dashboard, Snapshots, Videos), on top of the theme: the
card's color first, then the texture, inside the banner's corners. It's light and shade only, white in the dark and
black in the light at low alpha, so it sits on any theme without adding a color. `crates/dp/ui/texture.ts` names them
and their strengths:

- **None** leaves the header as the theme draws it. It's the default.
- **Haze** is a soft pool of light near the top right and a fainter one along the bottom.
- **Dots** is the same light picking out a grid of fine dots, one every 14 points, where it's strongest.

Both draw natively with the rest of the header. Each pool is a radial gradient whose stops follow a bell curve, laid
over the header's color as its `backgroundImage`, and the dots are one path on an nsh `Canvas`, filled with the glow's
falloff. Neither animates, so neither redraws until the header changes size. The light scrolls at half the page's
speed, like the masthead's other parts. `settings.json` keeps the texture's name; a name this version doesn't know
shows None.

Textures don't use an nsh `Shader`, which costs too much for a header's finish: the first shader opens a GPU device of
its own (about 145 MB for as long as the app runs), a shader that reads `nsh.time` renders and copies back every
display frame, and its frames lag a frame or two behind, so the header stretches and shakes while the window is
resized. Drawn by a shader, Dots kept 278 MB, 144 fps and 19% CPU while idle on the test picture in a debug build;
drawn natively it keeps 134 MB, no frames and 1%, the same as None.

## Elevation

The elevation is how far raised things lift off the page, the same in every theme: the theme sets the colors and
outlines, and the elevation adds depth on top (`raise` in `crates/dp/ui/brand.tsx`). Its shadows are black and its
lights white or black, at low alpha, so it adds no color. `crates/dp/ui/elevation.ts` names the levels:

- **None**, the default, is flat: colors and lines alone set things apart.
- **Soft** casts a shadow under raised things: cards, the settings groups, menus, toasts, dialogs and tooltips,
  buttons, the chosen segment of a segmented control, the selected sidebar row and a slider's thumb.
- **Raised** adds light to those shadows. The top edge of a raised thing catches it (a card's one-point edge is lit at
  the top in the dark and shaded at the bottom in the light); primary buttons, checked boxes and tooltips have light
  falling from their top edge; badges are beveled like keys; and things that take input or are on are pressed in: text
  fields, the tracks of segmented controls and sliders, and a button that's on (the PTZ panel's).

Each place's light depends on how light its own color is in the current theme, so a pale primary button gets a
visible sheen and a saturated one a soft one. The sheen on the primary button and the selected segment is reduced where
needed to keep their text readable, hover included. The high contrast themes stay flat at every elevation, since their
outlines mark every state and a shadow would blur them. `settings.json` keeps the elevation's name; one this version
doesn't know is None.

At Soft and Raised, things that stay put over a scroll also shade what scrolls under them: a page's header over its
page, and the sidebar's cameras heading over the cameras. The shade is 12 points deep under its line, darkest along it,
and fades in over the first 24 points of content passing beneath (`scrollShade`, drawn by `ScrollEdge` in
`crates/dp/ui/edge.tsx`, which can also face up, from the same stops). At both ends of the sidebar's list the shade
falls over a fade into the sidebar's color (`fadeFrom`), not onto the rows, so it lies on the same plain color at both
ends and looks the same at the top and the bottom.

Every card is the one in `crates/dp/ui/card.tsx` (`Card`, or its classes, `cardClass()`, with `look().surface` on a
pressable one). It puts the shadow on the rounded view itself, so the shadow follows the corners; on a square wrapper
the shadow would be square and its corners would show past the card's. A view that clips its children still draws its
own shadow whole, but a clipping parent cuts a child's shadow off at its edge, so a card inside one (a page header's
banner) has `SHADOW_ROOM` around it, as far as a card's shadow reaches at any elevation.

## Workarounds for nsh 0.5.1

nsh's website and gallery follow its `main` branch, which has moved on past the 0.5.1 release the suite uses, so
check what you take from them against the installed crates. Each workaround below goes once a release covers what it
stands in for:

- **The folder picker and Explorer.** `crates/dp/ui/src/folder_dialog.rs` shows the system's folder picker itself, and
  `crates/dp/ui/src/explorer.rs` starts Explorer, because 0.5.1's `Picker` picks files but not folders, and nothing
  shows a file in its folder. A release with both replaces them.
- **Stopping PTZ as the window closes.** IP Camera stops PTZ in `VideoView`'s `Drop`
  (`crates/ipcamera/app/src/video.rs`), because nsh sends the close event to JS, and a closing window never waits for
  JS. A tile that goes away for any other reason runs the same `Drop`, so the wait for the stop is at most half a
  second (`Camera::release_ptz`); a camera that stopped answering gets its retries on a thread of its own.
- **Menus in a browser.** They're drawn (`MenuOpener`, `popupMenu` and `MenuLayer` in `crates/dp/ui/menu.tsx`),
  because `nsh/desktop`'s `Menu.popup` does nothing there. A release whose `Menu.popup` works in a browser replaces the
  drawn half, and buttons that open menus then need no `MenuOpener`. A test, opened as the web, covers it. Two costs:
  `Button` doesn't say where it was pressed, so a button's menu opens under the button rather than at the pointer; and
  a dialog that covers the window places a `MenuLayer` last so its own right-click menus show inside it.
- **Picking several items in a tree.** `TreeList` in `crates/dp/ui/tree.tsx` lets its rows handle their own presses
  when the app picks several items (`marked`, `onPress`: Markdown Viewer's files with Ctrl+click and Shift+click),
  because nsh 0.5.1's `Tree` selects one item and its presses don't say which keys were held. The tree keeps its
  keyboard handling; Shift with an arrow doesn't pick a range. A release whose `Tree` selects several replaces both;
  Markdown Viewer's `App.test.tsx` covers Ctrl+click.
- **Escape from a text field in a dialog.** `EditorDialog` in `crates/dp/ui/editor.tsx` (and so every product's
  Settings) binds Escape while it shows (`EscapeCloses`), because nsh 0.5.1's `Dialog` closes on Escape only while no
  text field has the keyboard. A list open in the dialog still gets Escape first. A release whose `Dialog` closes from
  a text field replaces it; IP Camera's `App.test.tsx` covers both.
- **Split's scrolling.** Markdown Viewer's Split reads its raw view's scroll offset every 33 ms (`RAW_CHECK_EVERY` in
  `crates/mdviewer/app/views/scrollsync.ts`) while the pointer is over it or it has the keyboard, so the rendered side
  can follow, because nsh 0.5.1's `Editor` reports no scrolling. A release whose `Editor` has an `onScroll` replaces the
  timer; `ScrollSync.test.ts` covers how the two sides line up, and the following itself is for a look by hand.
- **The map's band.** For the same reason, Markdown Viewer's map beside Raw asks its editor which lines are in view
  (`visibleRange`) every 50 ms while the editor can scroll (the pointer over it or the map, or the keyboard in it) and
  every 500 ms otherwise (`MAP_VIEW_CHECK_ACTIVE` and `MAP_VIEW_CHECK_IDLE` in `crates/mdviewer/app/views/reader.tsx`),
  so its band follows the view. The same `onScroll` replaces it.
- **Settings saved by the development host.** Markdown Viewer's development host saves `settings.json` and
  `projects.json` in a `.nsh` folder beside itself (`development_folder` in `crates/dp/ui/src/lib.rs`). `nsh run` 0.5.1
  treats a change to any `.json` file in the app's folder as a TSX change and rebuilds the app; it skips `target` only
  when looking for Rust changes, and the host lives in the app's `target\debug`, so every saved window move,
  preference or tab would start a build. It never looks inside a folder named `.nsh`. A release whose `nsh run` skips
  `target` for TSX too replaces it. IP Camera's development host still saves beside itself, in the app's
  `target\debug`, and rebuilds the same way.

## Brands

`assets/brand/` has a folder for each brand, the company's and each product's, with its logos and a `brand.json` (its
name, tagline and palette).

| | Brand | Folder | Primary | Accent |
| --- | --- | --- | --- | --- |
| <img src="../assets/brand/drifting-prompts/logo/drifting-prompts-icon-color.svg" width="40" alt=""> | Drifting Prompts, the suite | `assets/brand/drifting-prompts` | `#1E3A8A` | `#0EA5E9` |
| <img src="../assets/brand/ipcamera/logo/ip-camera-icon-color.svg" width="40" alt=""> | IP Camera | `assets/brand/ipcamera` | `#7C3AED` | `#EC4899` |
| <img src="../assets/brand/mdviewer/logo/md-viewer-icon-color.svg" width="40" alt=""> | Markdown Viewer ("MD Viewer" in its logo and `brand.json`) | `assets/brand/mdviewer` | `#0F766E` | `#2DD4BF` |

Each brand's `logo/` folder has the same set, each as an SVG (its letters saved as outlines, so no font is needed)
and as a 2048-pixel PNG:

| File | What it is |
| --- | --- |
| `*-color` | The mark and the name, on a square, for a light background |
| `*-horizontal-color`, `*-stacked-color` | The same, cropped close, the name beside the mark or under it |
| `*-wordmark-color` | The name alone |
| `*-icon-color` | The mark alone |
| `*-inverse` | The mark and the name in white, on a square, for a dark background |
| `*-horizontal-inverse` | The same, cropped close, the name beside the mark |
| `*-mono` | The mark and the name in near-black, on a square |
| `*-reversed` | The mark and the name in white on the brand's primary color |

The products don't read these at run time; each ships its own copy, rendered or written out from here:

- Each app's icon (`icon.png`, `nsh.json` `icon`) is rendered from `ipcamera/ip-camera-app-icon.svg` or
  `mdviewer/md-viewer-app-icon.svg`.
- The logo an app draws is its `*-icon-color` mark, as markup in `crates/dp/ui/logos.ts`.
- The READMEs' headers use the brand's `*-horizontal-color` logo, and `*-horizontal-inverse` in dark mode: the suite's
  in the root README, each product's in its own. Both are cropped alike, with a margin of about a tenth of the
  drawing's height, so a header takes the same room in either mode.

## Screenshots

The READMEs show each app from `assets/screenshots/`, light and dark (`ipcamera-light.png` and `ipcamera-dark.png`,
and the same pairs for `ipcamera-dashboard`, `ipcamera-events`, `ipcamera-search` and `mdviewer`), in a `<picture>`
that follows the reader's light or dark mode. After a change to how an app looks, take them again from the repository
root:

```powershell
cargo screenshots            # both apps; `cargo screenshots ipcamera` or `mdviewer` for one
```

It needs `resvg` (`cargo install resvg`). It opens each app's window on this machine once, in turn, under
`nsh test --target windows`, with a test it writes to the app's `.nsh\test\` and deletes afterward. Keep the pointer
away from the windows until it finishes, or a tile shows its toolbar. Each app opens as it would the first time, in
its default appearance (System, no texture, no elevation, the Default themes), and goes through its screens, taking
each one in the current mode, then switching mode and taking the other. Everything shown is made up:

| App | What it shows | From |
| --- | --- | --- |
| IP Camera | Four cameras in a 2 × 2 view | `assets/screenshots/cameras/`, an SVG drawn for each camera and named after it, which `resvg` renders and the test pattern shows in place of its own (`IPCAMERA_SOURCE=pattern`, `IPCAMERA_PICTURES`), so nothing reaches a camera |
| IP Camera's dashboard | Two weeks of events from those cameras, each with its snapshot and video, and a few snapshots taken by hand | `crates/xtask/src/screenshots/dashboard.rs`: each camera's habits (people and parcels at the front door by day, cars in the driveway morning and evening, animals in the backyard, motion at the side gate by night), placed back from the time of the run by the local clock. A snapshot is its camera's picture; a video is an empty sparse file the size of a recording that long, since the pages read only a video's name, size and time |
| IP Camera's events | The same events as a log, the newest previewed beside it | As the dashboard's |
| IP Camera's network search | Six devices answering *Search network for devices* in the Add camera dialog: the four cameras, a camera not added yet, and one found by its video alone | `FOUND` in `crates/xtask/src/screenshots.rs`, written to `target\screenshots\found.json`, which the test pattern's search lists (`IPCAMERA_FOUND`) without asking the network |
| Markdown Viewer | Three projects, the first open on its sensor guide in Split, with every panel showing: the files, a map beside each side, and the outline | `assets/screenshots/projects/`; Markdown Viewer's `App.test.tsx` drops its Weathervane project too, so a change to those files can change what that test finds |

The apps' development builds keep their settings in their own `target\debug`, including the developer's cameras,
events and projects. The command moves those files to `target\screenshots\kept\`, puts the made-up ones in their
place, and puts the developer's back when the app closes (Ctrl+C included), then deletes the copies, which hold the
cameras' passwords. If a run is stopped before that (its console closed), the next run puts them back first.
`crates/xtask/src/screenshots.rs` has the details: the window's size, the waits, the views and the projects.
