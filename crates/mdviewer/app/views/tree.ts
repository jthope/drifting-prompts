import type { TreeItem } from "nsh/base";
import type { FileEntry, FilesView } from "mdviewer/native";

/** A folder's id: its path with a trailing slash, so it never clashes with a file's. */
export const folderId = (path: string) => `${path}/`;

/** The project's own folder's id. */
export const ROOT = folderId("");

/** Whether tree item `id` is a folder's. */
export const isFolder = (id: string) => id.endsWith("/");

/** The tree of `files`, already in the panel's order, under the project's folder named `root`. */
export function fileTree(files: FileEntry[], root: string): TreeItem[] {
  if (files.length === 0) return [];
  const top: TreeItem[] = [];
  const folders = new Map<string, TreeItem[]>([["", top]]);
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

/** A group for each folder that holds files, named by its whole path. */
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

// nsh has no Intl.Collator.
const byText = (a: string, b: string) => {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  return x < y ? -1 : x > y ? 1 : 0;
};

/** Every file, by name, then by folder. */
export function fileList(files: FileEntry[]): TreeItem[] {
  return files
    .slice()
    .sort((a, b) => byText(a.name, b.name) || byText(a.path, b.path))
    .map((file) => ({ id: file.path, label: file.name }));
}

/** The items `view` shows `files` as. */
export function itemsFor(view: FilesView, files: FileEntry[], root: string): TreeItem[] {
  if (view === "folder") return folderGroups(files, root);
  if (view === "list") return fileList(files);
  return fileTree(files, root);
}

/** The folders to expand to show file `path` in `view`, outermost first. */
export function foldersAbove(path: string, view: FilesView = "tree"): string[] {
  if (view === "list" || path === "") return [];
  const parts = path.split("/").slice(0, -1);
  if (view === "folder") return [folderId(parts.join("/"))];
  return [ROOT, ...parts.map((_, index) => folderId(parts.slice(0, index + 1).join("/")))];
}

/** Every folder of `items`, at any depth. */
export function allFolders(items: TreeItem[]): string[] {
  return items.flatMap((item) => (item.children ? [item.id, ...allFolders(item.children)] : []));
}

/** The folders of `items` not in `closed`: every folder starts open. */
export function openFolders(items: TreeItem[], closed: string[]): string[] {
  return allFolders(items).filter((id) => !closed.includes(id));
}

/** The files `items` show, top to bottom, with the folders in `expanded` open. */
export function shownFiles(items: TreeItem[], expanded: string[]): string[] {
  return items.flatMap((item) => (item.children ? (expanded.includes(item.id) ? shownFiles(item.children, expanded) : []) : [item.id]));
}

/** The files from `from` to `to` in `order`, both included; `to` alone when `from` isn't shown. */
export function run(order: string[], from: string, to: string): string[] {
  const a = order.indexOf(from);
  const b = order.indexOf(to);
  if (b < 0) return [];
  if (a < 0) return [to];
  return order.slice(Math.min(a, b), Math.max(a, b) + 1);
}

/** The files picked after a press on `id`: Ctrl toggles it, Shift picks the run from `anchor`. */
export function pickedFiles(order: string[], marked: string[], anchor: string, id: string, keys: { control: boolean; shift: boolean }): string[] {
  if (keys.shift) return run(order, anchor === "" ? id : anchor, id);
  if (keys.control) return marked.includes(id) ? marked.filter((each) => each !== id) : [...marked, id];
  return [id];
}

/** The file a project with no tabs opens: its top README, else its first; `""` for none. */
export function fileToOpen(files: FileEntry[]): string {
  const readme = files.find((file) => file.folder === "" && /^readme\.(md|markdown|mdx)$/i.test(file.name));
  return readme?.path ?? files[0]?.path ?? "";
}

/** The files whose path holds `query`, in any case. */
export function matching(files: FileEntry[], query: string): FileEntry[] {
  const wanted = query.trim().toLowerCase();
  return wanted === "" ? files : files.filter((file) => file.path.toLowerCase().includes(wanted));
}
