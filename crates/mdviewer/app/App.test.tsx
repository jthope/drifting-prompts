// The window from New to reading: a folder dropped on New becomes a project,
// its README opens rendered with its outline and its files, a link opens
// another file of the project and another brings it back, a tab's mode is
// saved, Settings opens in a dialog over it, and New shows in its place;
// Ctrl+click on a link opens
// its file in a tab of its own; the title bar's search counts what it finds,
// and F3 steps through it from file to file. Then the tabs: a single press
// opens the preview, Ctrl+click picks several files to open together, and the
// tabs close, come back, pin and unpin from their own pin and close around the
// pinned one; the breadcrumbs under them and the files their folders list; the
// files' three views; the panes' own buttons; and the title bar's theme menu, its
// mode first, then that mode's themes (`dp/ui`'s, which IP Camera has too and
// leaves to this test).
//
// The headless runner keeps its settings in a throwaway folder of its own
// (`STORE` in `src/bridge.rs`), so every run starts with no projects. The tests
// of a run share it, and the window's state, so each removes its project and
// empties what it typed before it ends. The
// project is Weathervane, from `assets/screenshots/projects`, the one the READMEs'
// screenshots show. Headless tests read a model of
// the views, not the screen: how the page, the editor and the tabs look, the
// scroll as the outline goes to a heading, and a tab dragged along the strip,
// are for a look by hand.
import { expect, open, test } from "nsh/test";
import { Preferences, Projects } from "mdviewer/native";

/** This file's folder as a `file:` URL, as a drop gives one: the headless runner names modules by their bare path (`d:/...`). */
const HERE = import.meta.url.startsWith("file:") ? import.meta.url : `file:///${import.meta.url}`;

/** The project the tests drop, and the screenshots show. */
const WEATHERVANE = new URL("../../../assets/screenshots/projects/Weathervane/", HERE).href;

/** The open project's tabs, as saved. */
const tabs = () => Projects.list().projects[0]?.tabs ?? [];
/** The files of the tabs, in order: pinned ones with `^`, the preview with `?`. */
const tabFiles = () => tabs().map((tab) => `${tab.pinned ? "^" : ""}${tab.file}${tab.preview ? "?" : ""}`);

test("a dropped folder opens on its README, and its files, links and outline lead around it", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });

  await app.waitFor("Open a folder");
  expect(Projects.list().projects.length).toBe(0);

  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitUntil(() => Projects.list().projects.length === 1);
  expect(Projects.list().projects[0].name).toBe("Weathervane");

  // Its README, rendered, as the preview, with its headings in the outline and its files in the panel.
  await app.waitFor("What it records");
  await app.waitUntil(() => tabFiles().join() === "README.md?");
  expect(app.shows("label:What it records")).toBe(true);
  expect(app.shows("label:Documentation")).toBe(true);
  // Every folder starts open, under the project's own: `docs` holds `sensors.md`.
  await app.waitFor("text:sensors.md");
  expect(app.texts()).toContain("docs");

  // A link to another file of the project opens it in the preview's place, and the project remembers it in
  // front. The README links Sensors twice, in its text and its table: the first.
  await app.press("Sensors >> nth=0");
  await app.waitFor("What's supported");
  await app.waitFor("id:crumbs >> text:sensors.md");
  await app.waitUntil(() => Projects.list().projects[0].lastFile === "docs/sensors.md");
  expect(tabFiles()).toEqual(["docs/sensors.md?"]);
  // Saving the tabs starts nothing over: the tree stays open on the file, and the page stays as it is.
  await app.wait(600);
  expect(app.texts()).toContain("configuration.md");
  expect(app.texts()).toContain("What's supported");

  // A link back to a heading of the README opens the README there.
  await app.press("what each sensor records");
  await app.waitFor("What it records");

  // Raw shows the text as written, and the tab keeps it. The header's button says how the file shows, and its menu
  // changes it.
  await app.press("label:Rendered. Change how the file shows");
  await app.chooseMenuItem("Raw");
  await app.waitUntil(() => tabs()[0]?.mode === "raw");
  await app.press("label:Raw. Change how the file shows");
  await app.chooseMenuItem("Rendered");
  await app.waitUntil(() => tabs()[0]?.mode === "rendered");

  // Settings opens in a dialog over the project, its sections down its side: Files picks how a new tab shows,
  // saved, and its Tabs setting wraps the tabs onto more rows, and back to one that scrolls, saved each time;
  // Appearance is the suite's.
  await app.press("label:Settings");
  await app.waitFor("id:section:files");
  await app.press("id:section:files");
  await app.waitFor("text:Open files as");
  await app.press("label:Split");
  await app.waitUntil(() => Preferences.load().mode === "split");
  await app.press("label:Rendered");
  await app.waitUntil(() => Preferences.load().mode === "rendered");
  expect(app.texts()).toContain("Tabs that don't fit");
  await app.press("label:Wrap");
  await app.waitUntil(() => Preferences.load().tabOverflow === "wrap");
  await app.press("label:Scroll");
  await app.waitUntil(() => Preferences.load().tabOverflow === "scroll");
  await app.press("id:section:appearance");
  await app.waitFor("text:Mode");
  expect(app.texts()).not.toContain("Open files as");
  // Escape closes it, and the project is as it was behind it; New shows in its place.
  await app.keyboard.press("escape");
  await app.waitFor("id:section:files", { state: "gone" });
  await app.waitFor("What it records");
  await app.press("label:New");
  await app.waitFor("Open a folder");
  expect(Projects.list().selected).toBe("");

  // The project stays listed, and opens again on the tab it had.
  await app.press("id:sidebar-projects >> label:Weathervane");
  await app.waitFor("What it records");

  await Projects.remove(Projects.list().projects[0].path);
  await app.waitFor("Open a folder");
});

test("Ctrl+click on a link opens its file in a tab of its own, beside the preview", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");
  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitFor("What it records");
  await app.waitUntil(() => tabFiles().join() === "README.md?");

  await app.keyboard.down("ctrl");
  await app.press("Sensors >> nth=0");
  await app.keyboard.up("ctrl");
  await app.waitFor("What's supported");
  await app.waitUntil(() => tabFiles().join() === "README.md?,docs/sensors.md");

  await Projects.remove(Projects.list().projects[0].path);
  await app.waitFor("Open a folder");
});

test("the title bar's search finds text in every file, lists it, and F3 and its drop-down go through it", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");
  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitFor("What it records");
  await app.waitUntil(() => tabFiles().join() === "README.md?");
  /** Whether the drop-down shows: its foot names the keys. */
  const dropDown = () => app.texts().includes("go there");

  // The box opens the drop-down. "Raspberry Pi" is in the README and in docs/getting-started.md, once each: the
  // drop-down, the status bar and the README's tab say so, and the files panel lists them.
  await app.press("id:search");
  await app.waitUntil(dropDown);
  await app.type("label:Search the projects", "Raspberry Pi");
  await app.waitFor("text:2 matches in 2 files");
  await app.waitFor("text:2 matches · 2 files");
  await app.waitFor("label:README.md, preview, 1 match");
  expect(app.shows("id:results")).toBe(true);

  // Escape closes the drop-down and keeps the search.
  await app.keyboard.press("escape");
  await app.waitUntil(() => !dropDown());
  expect(app.texts()).toContain("2 matches · 2 files");

  // In Raw, F3 selects the README's match: the caret ends after it.
  await app.press("label:Rendered. Change how the file shows");
  await app.chooseMenuItem("Raw");
  await app.keyboard.press("f3");
  await app.waitFor("text:Match 1 of 2 · 2 files");
  await app.waitFor("text:Ln 3, Col 60");

  // In the drop-down, Down and Return go to the next match, in getting-started.md, which opens as the preview.
  await app.keyboard.press("mod+k");
  await app.waitUntil(dropDown);
  await app.keyboard.press("down");
  await app.keyboard.press("enter");
  await app.waitUntil(() => !dropDown());
  await app.waitUntil(() => tabFiles().join() === "docs/getting-started.md?");
  await app.waitFor("text:Match 2 of 2 · 2 files");
  await app.waitFor("What you need");

  // The files panel's button shows every file again, and back; so does Ctrl+Return in the drop-down.
  await app.press("label:Show every file");
  await app.waitUntil(() => app.shows("id:files"));
  await app.press("label:Show what the search found");
  await app.waitUntil(() => app.shows("id:results"));
  await app.press("label:Show every file");
  await app.waitUntil(() => app.shows("id:files"));
  await app.keyboard.press("mod+k");
  await app.waitUntil(dropDown);
  await app.keyboard.press("mod+enter");
  await app.waitUntil(() => !dropDown() && app.shows("id:results"));

  // Ending the search takes its marks away, and the files come back.
  await app.press("label:Clear search");
  await app.waitUntil(() => !app.texts().some((text) => text.includes("Match ")));
  await app.waitUntil(() => app.shows("id:files"));
  await Projects.remove(Projects.list().projects[0].path);
  await app.waitFor("Open a folder");
});

test("All projects finds text in every project, and a match in another project opens it there", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");
  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitFor("What it records");
  // Weathervane's own docs folder, as a second project, open in front: it has no README, so its first file opens.
  await app.dropFiles("id:window", [new URL("docs/", WEATHERVANE).href]);
  await app.waitUntil(() => Projects.list().projects.length === 2);
  await app.waitFor("Storage");

  // Every project: the docs project's getting-started.md first, as it is the one open, then Weathervane's README and
  // getting-started.md.
  await app.press("id:search");
  await app.press("label:All projects");
  await app.type("label:Search the projects", "Raspberry Pi");
  await app.waitFor("text:3 matches in 3 files");
  // The marks and F3 go by the open project's.
  await app.waitFor("text:1 match · 1 file");

  // The second is Weathervane's README: Weathervane opens on it.
  await app.keyboard.press("down");
  await app.keyboard.press("enter");
  await app.waitUntil(() => Projects.list().projects.find((each) => each.path === Projects.list().selected)?.name === "Weathervane");
  await app.waitFor("What it records");
  await app.waitFor("text:Match 1 of 2 · 2 files");

  // Back as it was, for the next test: the project scope, no search, no projects.
  await app.keyboard.press("mod+k");
  await app.press("label:Project");
  await app.keyboard.press("escape");
  await app.press("label:Clear search");
  for (const each of Projects.list().projects) await Projects.remove(each.path);
  await app.waitFor("Open a folder");
});

test("the page's scroll bar carries its marks, and with the outline hidden the heading rail stands in for it", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");
  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitFor("What it records");
  expect(app.shows("id:page-marks")).toBe(true);
  expect(app.shows("id:rail")).toBe(false);

  // Hiding the outline brings the rail; resting the pointer on it opens the headings, and a press goes to one.
  await app.press("label:Hide outline");
  await app.waitUntil(() => app.shows("id:rail"));
  await app.hover("id:rail");
  await app.waitUntil(() => app.shows("id:rail-headings"));
  await app.press("id:rail-headings >> label:Documentation");
  await app.waitUntil(() => !app.shows("id:rail-headings"));

  // The outline back, and the rail goes.
  await app.press("label:Show outline");
  await app.waitUntil(() => !app.shows("id:rail"));
  await Projects.remove(Projects.list().projects[0].path);
  await app.waitFor("Open a folder");
});

test("each view has a map of the file beside it, which previews where the pointer rests and hides from the header", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");
  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitFor("What it records");

  // Rendered's map, and the part of the file there, larger, where the pointer rests.
  expect(app.shows("id:page-map")).toBe(true);
  await app.hover("id:page-map");
  await app.waitUntil(() => app.shows("id:map-preview"));

  // Raw's, and Split's two.
  await app.press("label:Rendered. Change how the file shows");
  await app.chooseMenuItem("Raw");
  await app.waitUntil(() => app.shows("id:raw-map"));
  await app.press("label:Raw. Change how the file shows");
  await app.chooseMenuItem("Split");
  await app.waitUntil(() => app.shows("id:raw-map") && app.shows("id:page-map"));

  // The header's button hides both, and shows them again.
  await app.press("label:Hide the map");
  await app.waitUntil(() => !app.shows("id:raw-map") && !app.shows("id:page-map"));
  await app.press("label:Show the map");
  await app.waitUntil(() => app.shows("id:raw-map"));

  // Back as it was, for the next test.
  await app.press("label:Split. Change how the file shows");
  await app.chooseMenuItem("Rendered");
  await Projects.remove(Projects.list().projects[0].path);
  await app.waitFor("Open a folder");
});

test("the search's results open in a tab of their own, each match in place, and a line goes to it", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");
  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitFor("What it records");
  await app.waitUntil(() => tabFiles().join() === "README.md?");

  await app.press("id:search");
  await app.type("label:Search the projects", "Raspberry Pi");
  await app.waitFor("text:2 matches in 2 files");
  await app.press("Open in a tab");

  // The tab, after the file's, with the matches in place and the files beside them.
  await app.waitUntil(() => app.shows("id:results-tab"));
  await app.waitFor("label:Search: Raspberry Pi, 2 matches");
  await app.waitFor("In these results");
  await app.waitFor("label:Line 3");

  // A press on the line goes to its match, in the README, the first file listed (getting-started.md shows a line 3
  // too, its heading); the results' tab stays.
  await app.press("label:Line 3 >> nth=0");
  await app.waitUntil(() => !app.shows("id:results-tab"));
  await app.waitFor("text:Match 1 of 2 · 2 files");
  expect(app.shows("label:Search: Raspberry Pi, 2 matches")).toBe(true);

  // Ctrl+W closes the results' tab while it is in front.
  await app.press("label:Search: Raspberry Pi, 2 matches");
  await app.waitUntil(() => app.shows("id:results-tab"));
  await app.keyboard.press("mod+w");
  await app.waitUntil(() => !app.shows("id:results-tab") && !app.shows("label:Search: Raspberry Pi, 2 matches"));
  expect(tabFiles()).toEqual(["README.md?"]);

  await app.press("label:Clear search");
  await Projects.remove(Projects.list().projects[0].path);
  await app.waitFor("Open a folder");
});

test("files open in tabs, several at once, and the tabs close, come back and pin", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");
  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitFor("What it records");
  await app.waitUntil(() => tabFiles().join() === "README.md?");

  // A press opens a file as the preview, in the README's place.
  await app.waitFor("text:dashboard.md");
  await app.press("text:dashboard.md");
  await app.waitFor("Exports");
  await app.waitUntil(() => tabFiles().join() === "docs/dashboard.md?");

  // Ctrl+click picks another with it; both open, kept, the first shown in front.
  await app.keyboard.down("ctrl");
  await app.press("text:configuration.md");
  await app.keyboard.up("ctrl");
  await app.waitFor("Open 2 in tabs");
  expect(tabFiles()).toEqual(["docs/dashboard.md?"]);
  await app.press("Open 2 in tabs");
  await app.waitUntil(() => tabFiles().join() === "docs/dashboard.md,docs/configuration.md");
  await app.waitFor("Storage");

  // Ctrl+W closes the tab in front, and the one beside it comes forward; Ctrl+Shift+T brings it back.
  await app.keyboard.press("mod+w");
  await app.waitUntil(() => tabFiles().join() === "docs/dashboard.md");
  await app.waitFor("Exports");
  await app.keyboard.press("mod+shift+t");
  await app.waitUntil(() => tabFiles().join() === "docs/dashboard.md,docs/configuration.md");

  // The pin beside the tab's close button pins it, first; pressed again, it unpins it, first of the others.
  await app.press("label:Pin configuration.md");
  await app.waitUntil(() => tabFiles().join() === "^docs/configuration.md,docs/dashboard.md");
  await app.press("label:Unpin configuration.md");
  await app.waitUntil(() => tabFiles().join() === "docs/configuration.md,docs/dashboard.md");

  // A tab's menu pins it too; closing all but pinned leaves it alone, in front.
  await app.rightClick("id:tab:docs/configuration.md");
  await app.chooseMenuItem("Pin");
  await app.waitUntil(() => tabFiles().join() === "^docs/configuration.md,docs/dashboard.md");
  await app.rightClick("id:tab:docs/dashboard.md");
  await app.chooseMenuItem("Close all but pinned");
  await app.waitUntil(() => tabFiles().join() === "^docs/configuration.md");
  await app.waitFor("Storage");

  // Ctrl+W closes a pinned tab too, and the project has none left.
  await app.press("id:tab:docs/configuration.md");
  await app.keyboard.press("mod+w");
  await app.waitFor("No file open");
  expect(tabs().length).toBe(0);

  await Projects.remove(Projects.list().projects[0].path);
  await app.waitFor("Open a folder");
});

test("the breadcrumbs under the tabs lead from the project down to the file, and a folder's list its files", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");
  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitFor("What it records");
  await app.waitUntil(() => tabFiles().join() === "README.md?");
  // The README is at the project's top: the project, then the file.
  await app.waitFor("id:crumbs >> text:README.md");
  expect(app.shows("label:Files in Weathervane")).toBe(true);

  // A file in a folder: the project and the folder each hold files of their own, which they list.
  await app.press("text:field-log.md");
  await app.waitFor("November");
  await app.waitFor("id:crumbs >> text:field-log.md");
  for (const folder of ["Weathervane", "notes"]) expect(app.shows(`label:Files in ${folder}`)).toBe(true);

  // A folder's breadcrumb lists its files, the one in front checked; one of them opens as the preview.
  await app.press("label:Files in notes");
  expect(app.menu()?.map((entry) => `${entry.label}${entry.checked ? " ✓" : ""}`)).toEqual(["field-log.md ✓", "ideas.md"]);
  await app.chooseMenuItem("ideas.md");
  await app.waitFor("text:Ideas");
  await app.waitUntil(() => tabFiles().join() === "notes/ideas.md?");

  // The project's own lists its files, not those of the folders under it.
  await app.press("label:Files in Weathervane");
  expect(app.menu()?.map((entry) => entry.label)).toEqual(["CHANGELOG.md", "README.md"]);
  await app.chooseMenuItem("CHANGELOG.md");
  await app.waitFor("0.4.0");
  await app.waitUntil(() => !app.shows("label:Files in notes"));

  // The results' tab in front: the project, then Search.
  await app.press("id:search");
  await app.type("label:Search the projects", "Raspberry Pi");
  await app.waitFor("text:2 matches in 2 files");
  await app.press("Open in a tab");
  await app.waitUntil(() => app.shows("id:results-tab"));
  await app.waitFor("id:crumbs >> text:Search");
  expect(app.shows("id:crumbs >> text:CHANGELOG.md")).toBe(false);

  await app.press("label:Clear search");
  await Projects.remove(Projects.list().projects[0].path);
  await app.waitFor("Open a folder");
});

test("the files list as a tree, by folder or in one list, and hide from their own header", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");
  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitFor("What it records");

  // The tree starts with every folder open; a press closes one and opens it again, and "Collapse all"
  // closes all but the project's own.
  await app.waitFor("text:sensors.md");
  await app.press("text:docs");
  await app.waitUntil(() => !app.texts().includes("sensors.md"));
  await app.press("text:docs");
  await app.waitFor("text:sensors.md");
  await app.press("label:Collapse all folders");
  await app.waitUntil(() => !app.texts().includes("sensors.md"));
  expect(app.texts()).toContain("docs");

  // The list has every file, its folder beside it.
  await app.press("label:View as Tree. Change how the files are listed");
  await app.chooseMenuItem("View as List");
  await app.waitUntil(() => Preferences.load().filesView === "list");
  await app.waitFor("text:sensors.md");

  // The folder view heads each folder with its path, open.
  await app.press("label:View as List. Change how the files are listed");
  await app.chooseMenuItem("View as Folder");
  await app.waitUntil(() => Preferences.load().filesView === "folder");
  await app.waitFor("text:sensors.md");
  await app.press("label:Collapse all folders");
  await app.waitUntil(() => !app.texts().includes("sensors.md"));

  // The files hide from their header, and come back from the tabs' end.
  await app.press("label:Hide files");
  await app.waitUntil(() => !Preferences.load().filesOpen);
  await app.press("label:Show files");
  await app.waitUntil(() => Preferences.load().filesOpen);

  // Back as it was, for the next run.
  await app.press("label:View as Folder. Change how the files are listed");
  await app.chooseMenuItem("View as Tree");
  await app.waitUntil(() => Preferences.load().filesView === "tree");
  await Projects.remove(Projects.list().projects[0].path);
  await app.waitFor("Open a folder");
});

test("a drop of something that isn't a folder says what to drop", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");
  await app.dropFiles("id:window", [new URL("README.md", WEATHERVANE).href]);
  await app.waitFor("Drop a folder of Markdown files to open it as a project.");
  expect(Projects.list().projects.length).toBe(0);
});

test("the title bar's theme menu picks the mode first, then offers that mode's themes, and picks both as Settings does", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });
  await app.waitFor("Open a folder");

  // A fresh install follows the system, which `open` shows light: the mode comes first, then the light themes, each by
  // name, and the dark ones aren't there.
  await app.press("label:Theme");
  await app.waitFor("text:Light Modern");
  const texts = app.texts();
  expect(texts.indexOf("Mode")).toBeGreaterThanOrEqual(0);
  expect(texts.indexOf("Mode")).toBeLessThan(texts.indexOf("Light themes, while the system is light"));
  for (const mode of ["Light", "Dark", "System"]) expect(app.count(`label:${mode}`)).toBe(1);
  expect(texts).not.toContain("Dark Modern");
  await app.press("text:Light Modern");
  await app.waitUntil(() => Preferences.load().appearance.lightTheme === "light-modern");

  // Dark there is Settings' Dark, saved: the menu stays open on the dark themes, and the light ones go.
  await app.press("label:Theme");
  await app.press("label:Dark");
  await app.waitFor("text:Dark themes");
  await app.waitUntil(() => Preferences.load().appearance.dark && !Preferences.load().appearance.followSystem);
  expect(app.texts()).toContain("Dark Modern");
  expect(app.texts()).not.toContain("Light Modern");
  await app.press("text:Monokai");
  await app.waitUntil(() => Preferences.load().appearance.darkTheme === "monokai");
  // A mode never gives a pick up.
  expect(Preferences.load().appearance.lightTheme).toBe("light-modern");

  // Light, then System, which follows the system back to its light themes.
  await app.press("label:Theme");
  await app.press("label:Light");
  await app.waitFor("text:Light themes");
  await app.waitUntil(() => !Preferences.load().appearance.dark && !Preferences.load().appearance.followSystem);
  await app.press("label:System");
  await app.waitFor("text:Light themes, while the system is light");
  await app.waitUntil(() => Preferences.load().appearance.followSystem);

  // Back as it was, for the next run: Default for both appearances, following the system.
  await app.press("text:Default");
  await app.waitUntil(() => Preferences.load().appearance.lightTheme === "default-light");
  await app.press("label:Theme");
  await app.press("label:Dark");
  await app.waitFor("text:Dark themes");
  await app.press("text:Default");
  await app.waitUntil(() => Preferences.load().appearance.darkTheme === "default-dark");
  await app.press("label:Theme");
  await app.press("label:System");
  await app.waitUntil(() => Preferences.load().appearance.followSystem);
});
