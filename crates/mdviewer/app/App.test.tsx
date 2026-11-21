// The tests of a run share the saved files and the window, so each removes its project and empties what it typed.
import { expect, open, test } from "nsh/test";
import { Preferences, Projects } from "mdviewer/native";

// The headless runner names modules by their bare path (`d:/...`).
const HERE = import.meta.url.startsWith("file:") ? import.meta.url : `file:///${import.meta.url}`;

const WEATHERVANE = new URL("../../../assets/screenshots/projects/Weathervane/", HERE).href;

const tabs = () => Projects.list().projects[0]?.tabs ?? [];
// Pinned ones with `^`, the preview with `?`.
const tabFiles = () => tabs().map((tab) => `${tab.pinned ? "^" : ""}${tab.file}${tab.preview ? "?" : ""}`);

test("a dropped folder opens on its README, and its files, links and outline lead around it", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1200, height: 800 });

  await app.waitFor("Open a folder");
  expect(Projects.list().projects.length).toBe(0);

  await app.dropFiles("id:window", [WEATHERVANE]);
  await app.waitUntil(() => Projects.list().projects.length === 1);
  expect(Projects.list().projects[0].name).toBe("Weathervane");

  await app.waitFor("What it records");
  await app.waitUntil(() => tabFiles().join() === "README.md?");
  expect(app.shows("label:What it records")).toBe(true);
  expect(app.shows("label:Documentation")).toBe(true);
  await app.waitFor("text:sensors.md");
  expect(app.texts()).toContain("docs");

  // The README links Sensors twice, in its text and its table.
  await app.press("Sensors >> nth=0");
  await app.waitFor("What's supported");
  await app.waitFor("id:crumbs >> text:sensors.md");
  await app.waitUntil(() => Projects.list().projects[0].lastFile === "docs/sensors.md");
  expect(tabFiles()).toEqual(["docs/sensors.md?"]);
  // Saving the tabs must start nothing over.
  await app.wait(600);
  expect(app.texts()).toContain("configuration.md");
  expect(app.texts()).toContain("What's supported");

  await app.press("what each sensor records");
  await app.waitFor("What it records");

  await app.press("label:Rendered. Change how the file shows");
  await app.chooseMenuItem("Raw");
  await app.waitUntil(() => tabs()[0]?.mode === "raw");
  await app.press("label:Raw. Change how the file shows");
  await app.chooseMenuItem("Rendered");
  await app.waitUntil(() => tabs()[0]?.mode === "rendered");

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
  await app.keyboard.press("escape");
  await app.waitFor("id:section:files", { state: "gone" });
  await app.waitFor("What it records");
  await app.press("label:New");
  await app.waitFor("Open a folder");
  expect(Projects.list().selected).toBe("");

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
  // Its foot names the keys.
  const dropDown = () => app.texts().includes("go there");

  await app.press("id:search");
  await app.waitUntil(dropDown);
  await app.type("label:Search the projects", "Raspberry Pi");
  await app.waitFor("text:2 matches in 2 files");
  await app.waitFor("text:2 matches · 2 files");
  await app.waitFor("label:README.md, preview, 1 match");
  expect(app.shows("id:results")).toBe(true);

  await app.keyboard.press("escape");
  await app.waitUntil(() => !dropDown());
  expect(app.texts()).toContain("2 matches · 2 files");

  await app.press("label:Rendered. Change how the file shows");
  await app.chooseMenuItem("Raw");
  await app.keyboard.press("f3");
  await app.waitFor("text:Match 1 of 2 · 2 files");
  await app.waitFor("text:Ln 3, Col 60");

  await app.keyboard.press("mod+k");
  await app.waitUntil(dropDown);
  await app.keyboard.press("down");
  await app.keyboard.press("enter");
  await app.waitUntil(() => !dropDown());
  await app.waitUntil(() => tabFiles().join() === "docs/getting-started.md?");
  await app.waitFor("text:Match 2 of 2 · 2 files");
  await app.waitFor("What you need");

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
  // A second project with no README, so its first file opens.
  await app.dropFiles("id:window", [new URL("docs/", WEATHERVANE).href]);
  await app.waitUntil(() => Projects.list().projects.length === 2);
  await app.waitFor("Storage");

  await app.press("id:search");
  await app.press("label:All projects");
  await app.type("label:Search the projects", "Raspberry Pi");
  await app.waitFor("text:3 matches in 3 files");
  await app.waitFor("text:1 match · 1 file");

  // The open project's match is listed first, so the second is Weathervane's README.
  await app.keyboard.press("down");
  await app.keyboard.press("enter");
  await app.waitUntil(() => Projects.list().projects.find((each) => each.path === Projects.list().selected)?.name === "Weathervane");
  await app.waitFor("What it records");
  await app.waitFor("text:Match 1 of 2 · 2 files");

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

  await app.press("label:Hide outline");
  await app.waitUntil(() => app.shows("id:rail"));
  await app.hover("id:rail");
  await app.waitUntil(() => app.shows("id:rail-headings"));
  await app.press("id:rail-headings >> label:Documentation");
  await app.waitUntil(() => !app.shows("id:rail-headings"));

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

  expect(app.shows("id:page-map")).toBe(true);
  await app.hover("id:page-map");
  await app.waitUntil(() => app.shows("id:map-preview"));

  await app.press("label:Rendered. Change how the file shows");
  await app.chooseMenuItem("Raw");
  await app.waitUntil(() => app.shows("id:raw-map"));
  await app.press("label:Raw. Change how the file shows");
  await app.chooseMenuItem("Split");
  await app.waitUntil(() => app.shows("id:raw-map") && app.shows("id:page-map"));

  await app.press("label:Hide the map");
  await app.waitUntil(() => !app.shows("id:raw-map") && !app.shows("id:page-map"));
  await app.press("label:Show the map");
  await app.waitUntil(() => app.shows("id:raw-map"));

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

  await app.waitUntil(() => app.shows("id:results-tab"));
  await app.waitFor("label:Search: Raspberry Pi, 2 matches");
  await app.waitFor("In these results");
  await app.waitFor("label:Line 3");

  // getting-started.md shows a line 3 too, its heading.
  await app.press("label:Line 3 >> nth=0");
  await app.waitUntil(() => !app.shows("id:results-tab"));
  await app.waitFor("text:Match 1 of 2 · 2 files");
  expect(app.shows("label:Search: Raspberry Pi, 2 matches")).toBe(true);

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

  await app.waitFor("text:dashboard.md");
  await app.press("text:dashboard.md");
  await app.waitFor("Exports");
  await app.waitUntil(() => tabFiles().join() === "docs/dashboard.md?");

  await app.keyboard.down("ctrl");
  await app.press("text:configuration.md");
  await app.keyboard.up("ctrl");
  await app.waitFor("Open 2 in tabs");
  expect(tabFiles()).toEqual(["docs/dashboard.md?"]);
  await app.press("Open 2 in tabs");
  await app.waitUntil(() => tabFiles().join() === "docs/dashboard.md,docs/configuration.md");
  await app.waitFor("Storage");

  await app.keyboard.press("mod+w");
  await app.waitUntil(() => tabFiles().join() === "docs/dashboard.md");
  await app.waitFor("Exports");
  await app.keyboard.press("mod+shift+t");
  await app.waitUntil(() => tabFiles().join() === "docs/dashboard.md,docs/configuration.md");

  await app.press("label:Pin configuration.md");
  await app.waitUntil(() => tabFiles().join() === "^docs/configuration.md,docs/dashboard.md");
  await app.press("label:Unpin configuration.md");
  await app.waitUntil(() => tabFiles().join() === "docs/configuration.md,docs/dashboard.md");

  await app.rightClick("id:tab:docs/configuration.md");
  await app.chooseMenuItem("Pin");
  await app.waitUntil(() => tabFiles().join() === "^docs/configuration.md,docs/dashboard.md");
  await app.rightClick("id:tab:docs/dashboard.md");
  await app.chooseMenuItem("Close all but pinned");
  await app.waitUntil(() => tabFiles().join() === "^docs/configuration.md");
  await app.waitFor("Storage");

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
  await app.waitFor("id:crumbs >> text:README.md");
  expect(app.shows("label:Files in Weathervane")).toBe(true);

  await app.press("text:field-log.md");
  await app.waitFor("November");
  await app.waitFor("id:crumbs >> text:field-log.md");
  for (const folder of ["Weathervane", "notes"]) expect(app.shows(`label:Files in ${folder}`)).toBe(true);

  await app.press("label:Files in notes");
  expect(app.menu()?.map((entry) => `${entry.label}${entry.checked ? " ✓" : ""}`)).toEqual(["field-log.md ✓", "ideas.md"]);
  await app.chooseMenuItem("ideas.md");
  await app.waitFor("text:Ideas");
  await app.waitUntil(() => tabFiles().join() === "notes/ideas.md?");

  await app.press("label:Files in Weathervane");
  expect(app.menu()?.map((entry) => entry.label)).toEqual(["CHANGELOG.md", "README.md"]);
  await app.chooseMenuItem("CHANGELOG.md");
  await app.waitFor("0.4.0");
  await app.waitUntil(() => !app.shows("label:Files in notes"));

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

  await app.waitFor("text:sensors.md");
  await app.press("text:docs");
  await app.waitUntil(() => !app.texts().includes("sensors.md"));
  await app.press("text:docs");
  await app.waitFor("text:sensors.md");
  await app.press("label:Collapse all folders");
  await app.waitUntil(() => !app.texts().includes("sensors.md"));
  expect(app.texts()).toContain("docs");

  await app.press("label:View as Tree. Change how the files are listed");
  await app.chooseMenuItem("View as List");
  await app.waitUntil(() => Preferences.load().filesView === "list");
  await app.waitFor("text:sensors.md");

  await app.press("label:View as List. Change how the files are listed");
  await app.chooseMenuItem("View as Folder");
  await app.waitUntil(() => Preferences.load().filesView === "folder");
  await app.waitFor("text:sensors.md");
  await app.press("label:Collapse all folders");
  await app.waitUntil(() => !app.texts().includes("sensors.md"));

  await app.press("label:Hide files");
  await app.waitUntil(() => !Preferences.load().filesOpen);
  await app.press("label:Show files");
  await app.waitUntil(() => Preferences.load().filesOpen);

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

  // A fresh install follows the system, which `open` forces light.
  await app.press("label:Theme");
  await app.waitFor("text:Light Modern");
  const texts = app.texts();
  expect(texts.indexOf("Mode")).toBeGreaterThanOrEqual(0);
  expect(texts.indexOf("Mode")).toBeLessThan(texts.indexOf("Light themes, while the system is light"));
  for (const mode of ["Light", "Dark", "System"]) expect(app.count(`label:${mode}`)).toBe(1);
  expect(texts).not.toContain("Dark Modern");
  await app.press("text:Light Modern");
  await app.waitUntil(() => Preferences.load().appearance.lightTheme === "light-modern");

  await app.press("label:Theme");
  await app.press("label:Dark");
  await app.waitFor("text:Dark themes");
  await app.waitUntil(() => Preferences.load().appearance.dark && !Preferences.load().appearance.followSystem);
  expect(app.texts()).toContain("Dark Modern");
  expect(app.texts()).not.toContain("Light Modern");
  await app.press("text:Monokai");
  await app.waitUntil(() => Preferences.load().appearance.darkTheme === "monokai");
  expect(Preferences.load().appearance.lightTheme).toBe("light-modern");

  await app.press("label:Theme");
  await app.press("label:Light");
  await app.waitFor("text:Light themes");
  await app.waitUntil(() => !Preferences.load().appearance.dark && !Preferences.load().appearance.followSystem);
  await app.press("label:System");
  await app.waitFor("text:Light themes, while the system is light");
  await app.waitUntil(() => Preferences.load().appearance.followSystem);

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
