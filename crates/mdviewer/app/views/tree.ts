// The files panel's three views, from the flat list Rust gives
// (`Files.onChange`), each as items of nsh's `Tree`:
//
// - Tree: the project's own folder at the top, holding its files, then its
//   folders, each folder holding its own the same way, as the list is already
//   ordered.
// - Folder: an item for the project's own folder holding the files at its top,
//   then one for each folder under it that holds files, named by its whole path
//   (`docs/api`), with its files in it.
// - List: every file, by name, with its folder said beside it.
//
// The project's folder is an item of its own, named as the folder is, so a
// project with no folders in it still shows the tree and the folder view as
// such. A file's id is its path in the project (`docs/setup.md`); a folder's is
// its path with a slash at the end (`docs/`), and the project's own folder's
// is a slash alone (`ROOT`), so none clash.
import type { TreeItem } from "nsh/base";
import type { FileEntry, FilesView } from "mdviewer/native";

/** A folder's id in the tree: its path with a slash after it; the project's own folder's for `""`. */
export const folderId = (path: string) => `${path}/`;

/** The project's own folder's id: the folder at the top of the tree and the folder view. */
export const ROOT = folderId("");

/** Whether tree item `id` is a folder's. */
export const isFolder = (id: string) => id.endsWith("/");

/** The tree of `files`, which come in the files panel's order, under the project's folder, named `root`. */
export function fileTree(files: FileEntry[], root: string): TreeItem[] {
  if (files.length === 0) return [];
  const top: TreeItem[] = [];
  const folders = new Map<string, TreeItem[]>([["", top]]);
  /** The items of folder `path`, making it and the folders above it as needed. */
  const itemsOf = (path: string): TreeItem[] => {
    const made = folders.get(path);
    if (made) return made;
    const at = path.lastIndexOf("/");
    const items: TreeItem[] = [];
    itemsOf(at < 0 ? "" : path.slice(0, at)).push({ id: folderId(path), label: path.slice(at + 1), children: items });
    folders.set(path, items);
    return items;
  };
  for (const file of files) itemsOf(file.folder).push({ id: file.path, label: file.name });
  return [{ id: ROOT, label: root, children: top }];
}

/**
 * A folder for each folder that holds files, in the list's order: the
 * project's own, named `root`, with the files at its top, then each under it,
 * by its whole path.
 */
export function folderGroups(files: FileEntry[], root: string): TreeItem[] {
  const groups = new Map<string, TreeItem[]>();
  for (const file of files) {
    const item = { id: file.path, label: file.name };
    const group = groups.get(file.folder);
    if (group) group.push(item);
    else groups.set(file.folder, [item]);
  }
  return [...groups].map(([folder, children]) => ({ id: folderId(folder), label: folder === "" ? root : folder, children }));
}

/** `a` against `b`, whatever their case, by code point: the engine has no collator to do better. */
const byText = (a: string, b: string) => {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  return x < y ? -1 : x > y ? 1 : 0;
};

/** Every file, by name whatever its case, then by folder. */
export function fileList(files: FileEntry[]): TreeItem[] {
  return files
    .slice()
    .sort((a, b) => byText(a.name, b.name) || byText(a.path, b.path))
    .map((file) => ({ id: file.path, label: file.name }));
}

/** The items `view` shows `files` as, the project's folder named `root`. */
export function itemsFor(view: FilesView, files: FileEntry[], root: string): TreeItem[] {
  if (view === "folder") return folderGroups(files, root);
  if (view === "list") return fileList(files);
  return fileTree(files, root);
}

/** The folders `view` has file `path` in, as tree ids, outermost first: what to expand to show it. */
export function foldersAbove(path: string, view: FilesView = "tree"): string[] {
  if (view === "list" || path === "") return [];
  const parts = path.split("/").slice(0, -1);
  if (view === "folder") return [folderId(parts.join("/"))];
  return [ROOT, ...parts.map((_, index) => folderId(parts.slice(0, index + 1).join("/")))];
}

/** Every folder of `items`, at any depth: all a view's folders, open. */
export function allFolders(items: TreeItem[]): string[] {
  return items.flatMap((item) => (item.children ? [item.id, ...allFolders(item.children)] : []));
}

/** The folders of `items` that are open: all but those in `closed`, since every folder starts open. */
export function openFolders(items: TreeItem[], closed: string[]): string[] {
  return allFolders(items).filter((id) => !closed.includes(id));
}

/** The files `items` show, top to bottom, with the folders in `expanded` open: the order Shift picks a run of them in. */
export function shownFiles(items: TreeItem[], expanded: string[]): string[] {
  return items.flatMap((item) => (item.children ? (expanded.includes(item.id) ? shownFiles(item.children, expanded) : []) : [item.id]));
}

/** The files from `from` to `to` in `order`, both included, whichever comes first; `to` alone when `from` isn't shown. */
export function run(order: string[], from: string, to: string): string[] {
  const a = order.indexOf(from);
  const b = order.indexOf(to);
  if (b < 0) return [];
  if (a < 0) return [to];
  return order.slice(Math.min(a, b), Math.max(a, b) + 1);
}

/** The files picked after a press on file `id` with `keys` held: Ctrl adds or takes it away, Shift picks the run from `anchor`, else it alone. */
export function pickedFiles(order: string[], marked: string[], anchor: string, id: string, keys: { control: boolean; shift: boolean }): string[] {
  if (keys.shift) return run(order, anchor === "" ? id : anchor, id);
  if (keys.control) return marked.includes(id) ? marked.filter((each) => each !== id) : [...marked, id];
  return [id];
}

/**
 * The file to open in a project with no tabs whose files are `files`: its
 * README at the top, else its first. `""` for a project with none.
 */
export function fileToOpen(files: FileEntry[]): string {
  const readme = files.find((file) => file.folder === "" && /^readme\.(md|markdown|mdx)$/i.test(file.name));
  return readme?.path ?? files[0]?.path ?? "";
}

/** The files whose path holds `query`, whatever the case, for the panel's filter. */
export function matching(files: FileEntry[], query: string): FileEntry[] {
  const wanted = query.trim().toLowerCase();
  return wanted === "" ? files : files.filter((file) => file.path.toLowerCase().includes(wanted));
}
