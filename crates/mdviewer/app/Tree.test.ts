// The files panel's views from Rust's flat list (`views/tree.ts`), picking
// several files, the file a project opens on, and the filter.
import { expect, test } from "nsh/test";
import type { FileEntry } from "mdviewer/native";
import { allFolders, fileList, fileToOpen, fileTree, folderGroups, foldersAbove, matching, openFolders, pickedFiles, ROOT, run, shownFiles } from "./views/tree";

const file = (path: string): FileEntry => {
  const at = path.lastIndexOf("/");
  return { path, name: path.slice(at + 1), folder: at < 0 ? "" : path.slice(0, at) };
};

/** As Rust lists them: in each folder its files, then its folders. */
const FILES = ["CHANGELOG.md", "README.md", "docs/setup.md", "docs/api/intro.md", "notes/todo.md"].map(file);

test("folders hold their files, then their folders, in the list's order, under the project's own", () => {
  expect(fileTree(FILES, "project")).toEqual([
    {
      id: "/",
      label: "project",
      children: [
        { id: "CHANGELOG.md", label: "CHANGELOG.md" },
        { id: "README.md", label: "README.md" },
        {
          id: "docs/",
          label: "docs",
          children: [
            { id: "docs/setup.md", label: "setup.md" },
            { id: "docs/api/", label: "api", children: [{ id: "docs/api/intro.md", label: "intro.md" }] },
          ],
        },
        { id: "notes/", label: "notes", children: [{ id: "notes/todo.md", label: "todo.md" }] },
      ],
    },
  ]);
  // A project with no folders in it is still a tree: its own folder, holding its files.
  expect(fileTree([file("a.md"), file("b.md")], "flat")).toEqual([
    { id: ROOT, label: "flat", children: [{ id: "a.md", label: "a.md" }, { id: "b.md", label: "b.md" }] },
  ]);
  expect(fileTree([], "empty")).toEqual([]);
});

test("the folder view heads each folder with its whole path, the project's own first, by its name", () => {
  expect(folderGroups(FILES, "project")).toEqual([
    { id: "/", label: "project", children: [{ id: "CHANGELOG.md", label: "CHANGELOG.md" }, { id: "README.md", label: "README.md" }] },
    { id: "docs/", label: "docs", children: [{ id: "docs/setup.md", label: "setup.md" }] },
    { id: "docs/api/", label: "docs/api", children: [{ id: "docs/api/intro.md", label: "intro.md" }] },
    { id: "notes/", label: "notes", children: [{ id: "notes/todo.md", label: "todo.md" }] },
  ]);
  // No files at the top, no heading for them.
  expect(folderGroups([file("docs/setup.md")], "project").map((item) => item.id)).toEqual(["docs/"]);
});

test("the list view has every file by name, then by folder", () => {
  expect(fileList([...FILES, file("docs/README.md")]).map((item) => item.id)).toEqual([
    "CHANGELOG.md",
    "docs/api/intro.md",
    "docs/README.md",
    "README.md",
    "docs/setup.md",
    "notes/todo.md",
  ]);
});

test("a file's folders open to show it, outermost first, as each view has them", () => {
  expect(foldersAbove("docs/api/intro.md")).toEqual(["/", "docs/", "docs/api/"]);
  expect(foldersAbove("docs/api/intro.md", "folder")).toEqual(["docs/api/"]);
  expect(foldersAbove("docs/api/intro.md", "list")).toEqual([]);
  expect(foldersAbove("README.md")).toEqual(["/"]);
  expect(foldersAbove("README.md", "folder")).toEqual(["/"]);
  expect(foldersAbove("")).toEqual([]);
  expect(allFolders(fileTree(FILES, "project"))).toEqual(["/", "docs/", "docs/api/", "notes/"]);
});

test("every folder is open but those closed, and the files shown leave out those in closed folders", () => {
  const tree = fileTree(FILES, "project");
  expect(openFolders(tree, [])).toEqual(["/", "docs/", "docs/api/", "notes/"]);
  expect(openFolders(tree, ["docs/", "notes/"])).toEqual(["/", "docs/api/"]);
  expect(shownFiles(tree, openFolders(tree, []))).toEqual(["CHANGELOG.md", "README.md", "docs/setup.md", "docs/api/intro.md", "notes/todo.md"]);
  expect(shownFiles(tree, ["/"])).toEqual(["CHANGELOG.md", "README.md"]);
  // A folder inside a closed one stays hidden, open or not.
  expect(shownFiles(tree, ["/", "docs/api/"])).toEqual(["CHANGELOG.md", "README.md"]);
  expect(shownFiles(tree, ["docs/"])).toEqual([]);
});

test("a press picks one file, Ctrl adds or takes away, and Shift picks a run", () => {
  const order = ["a.md", "b.md", "c.md", "d.md"];
  const none = { control: false, shift: false };
  const ctrl = { control: true, shift: false };
  const shift = { control: false, shift: true };
  expect(pickedFiles(order, ["a.md", "b.md"], "a.md", "c.md", none)).toEqual(["c.md"]);
  expect(pickedFiles(order, ["a.md"], "a.md", "c.md", ctrl)).toEqual(["a.md", "c.md"]);
  expect(pickedFiles(order, ["a.md", "c.md"], "c.md", "a.md", ctrl)).toEqual(["c.md"]);
  expect(pickedFiles(order, ["b.md"], "b.md", "d.md", shift)).toEqual(["b.md", "c.md", "d.md"]);
  expect(pickedFiles(order, ["c.md"], "c.md", "a.md", shift)).toEqual(["a.md", "b.md", "c.md"]);
  expect(run(order, "gone.md", "b.md")).toEqual(["b.md"]);
  expect(run(order, "a.md", "gone.md")).toEqual([]);
});

test("a project with no tabs opens its README, else its first file", () => {
  expect(fileToOpen(FILES)).toBe("README.md");
  expect(fileToOpen([file("b.md"), file("docs/README.md")])).toBe("b.md");
  expect(fileToOpen([])).toBe("");
});

test("the filter keeps the files whose path holds what is typed, whatever its case", () => {
  expect(matching(FILES, "API").map((each) => each.path)).toEqual(["docs/api/intro.md"]);
  expect(matching(FILES, "  ").length).toBe(FILES.length);
});
