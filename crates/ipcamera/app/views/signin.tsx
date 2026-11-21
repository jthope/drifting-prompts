import { signal, Text, View } from "nsh";
import { Server } from "ipcamera/native";
import { Button, TextField } from "../../../dp/ui";

/** Asks for the server's password, and signs in with it. */
export function SignInPage() {
  const password = signal("");
  const problem = signal("");
  const busy = signal(false);

  const submit = () => {
    if (busy() || password() === "") return;
    busy.set(true);
    problem.set("");
    Server.signIn(password())
      .then(() => password.set(""))
      .catch((error) => problem.set(String((error as { message?: string })?.message ?? error)))
      .finally(() => busy.set(false));
  };

  return (
    <View testID="sign-in" class="flex-1 items-center justify-center bg-background p-6">
      <View class="w-full max-w-80 gap-4">
        <View class="gap-1">
          <Text role="header" class="text-base font-semibold text-foreground">
            Sign in to the server
          </Text>
          <Text class="text-sm text-muted-foreground">This server has a password. Enter it to see its cameras.</Text>
        </View>
        <TextField
          label="Password"
          value={password()}
          masked
          autoFocus
          error={problem() || undefined}
          onChange={(value) => password.set(value)}
          onSubmit={submit}
        />
        <Button kind="primary" label={busy() ? "Signing in…" : "Sign in"} disabled={busy() || password() === ""} onPress={submit} />
      </View>
    </View>
  );
}
