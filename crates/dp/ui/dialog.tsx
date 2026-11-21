import { signal, Text, View } from "nsh";
import { AlertDialog, Dialog } from "nsh/base";
import { Button } from "./button";
import { TextField } from "./field";
import { popupSurface } from "./menu";

type Question = { title: string; label: string; action: string; resolve: (name: string | null) => void };

const question = signal<Question | null>(null);
const typed = signal("");

/** Asks for a name, starting from `value`; resolves with it trimmed, or `null` when canceled. */
export function askName(options: { title: string; label: string; action: string; value?: string }): Promise<string | null> {
  question()?.resolve(null);
  typed.set(options.value ?? "");
  return new Promise((resolve) => question.set({ title: options.title, label: options.label, action: options.action, resolve }));
}

function answer(name: string | null) {
  const asked = question();
  question.set(null);
  asked?.resolve(name);
}

/** Where `askName` asks: a small panel in the middle of the window. */
export function NameDialog() {
  const accept = () => {
    const name = typed().trim();
    if (name !== "") answer(name);
  };
  return (
    <Dialog
      open={question() !== null}
      animation="fade"
      class="flex-1 items-center justify-center p-6"
      style={{ pointerEvents: "box-none" }}
      backdropClass="bg-black/40"
      onOpenChange={(event) => !event.open && answer(null)}
    >
      <If when={question()}>
        {(asked) => (
          <View class={popupSurface("w-[360px] max-w-full gap-4 p-4").class} style={popupSurface().style}>
            <Text role="header" class="text-sm font-semibold text-popover-foreground">
              {asked().title}
            </Text>
            <TextField label={asked().label} value={typed()} autoFocus onChange={(value) => typed.set(value)} onSubmit={accept} />
            <View class="flex-row justify-end gap-2">
              <Button label="Cancel" onPress={() => answer(null)} />
              <Button kind="primary" label={asked().action} disabled={typed().trim() === ""} onPress={accept} />
            </View>
          </View>
        )}
      </If>
    </Dialog>
  );
}

/** Asks before something is deleted, in the system's dialog. Resolves true to go ahead. */
export async function confirmDelete(options: { title: string; description: string; action: string }): Promise<boolean> {
  const result = await AlertDialog.show({
    title: options.title,
    description: options.description,
    actions: [
      { id: "cancel", label: "Cancel", role: "cancel" },
      { id: "delete", label: options.action, role: "destructive" },
    ],
  });
  return result.action === "delete";
}
