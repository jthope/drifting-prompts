// A message worth passing on, with a button that copies it: what a camera
// answered, or why it didn't. nsh 0.5.1's text can't be selected, so the
// button is how such a message leaves the app.
import { Clipboard, Text, View } from "nsh";
import { IconButton } from "./button";
import { notify } from "./toast";

/**
 * `text`, with a quieter `detail` line under it, and a button that puts both
 * on the clipboard. `tone` "error" draws the text in the destructive color.
 */
export function CopyableNote(props: { text: string; detail?: string; tone?: "error" | "normal" }) {
  const copy = () => {
    const text = props.detail ? `${props.text}\n${props.detail}` : props.text;
    Clipboard.writeText(text)
      .then(() => notify("Copied."))
      .catch((error) => notify(`Couldn't copy: ${String(error?.message ?? error)}`, "warn"));
  };
  return (
    <View class="flex-row items-start gap-2">
      <View class="min-w-0 flex-1 gap-0.5 pt-1">
        <Text class={`text-xs ${props.tone === "error" ? "text-destructive" : "text-foreground"}`}>{props.text}</Text>
        <If when={props.detail}>{(detail) => <Text class="text-xs text-muted-foreground">{detail()}</Text>}</If>
      </View>
      <IconButton icon="copy" label="Copy" size="small" onPress={copy} />
    </View>
  );
}
