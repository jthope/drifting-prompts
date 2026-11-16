import { Button, EmptyState } from "../../../dp/ui";
import { chooseFolder } from "./actions";

/**
 * New: what the window shows with no project open. A folder chosen here, or
 * dropped anywhere on the window, becomes a project: it joins the sidebar's
 * Projects, and its Markdown files list on the right.
 */
export function NewProject() {
  return (
    <EmptyState
      icon="folder-open"
      title="Open a folder"
      text="Choose a folder of Markdown files, or drop one on the window. It joins Projects, and its files list on the right."
      action={<Button kind="primary" icon="folder-open" label="Choose folder" onPress={chooseFolder} />}
    />
  );
}
