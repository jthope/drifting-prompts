import { Button, EmptyState } from "../../../dp/ui";
import { chooseFolder } from "./actions";

/** New: what the window shows with no project open. */
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
