import { computed, effect, type JSX, signal, Text, untracked, View } from "nsh";
import { Automation, Cameras, type CameraForm, type DetectedCamera, type EventVideoSettings, type FoundCamera } from "ipcamera/native";
import { Button, CheckField, CopyableNote, type EditorSection, EditorDialog, FieldRow, FormSection, SelectField, TextField } from "../../../dp/ui";
import { cameraAdded, editing } from "./actions";
import { KindChecks } from "./automation";
import { AFTER_SECONDS, BEFORE_SECONDS, beforeLabel, cameraStreams, LONGEST_MINUTES, longestLabel, numberItems, secondsLabel } from "./eventvideo";
import { FindCamerasDialog, foundTitle, withToldBrand } from "./finder";
import { refresh } from "./model";
import { streamChoices } from "./streams";

/** A part of a camera's settings. */
export type CameraSection = "connection" | "stream" | "ptz" | "features" | "events";

/** A camera's settings' parts, in order; Events only where automation is available. */
export const CAMERA_SECTIONS: readonly EditorSection<CameraSection>[] = [
  { value: "connection", title: "Connection", icon: "cctv" },
  { value: "stream", title: "Stream", icon: "video" },
  { value: "ptz", title: "PTZ", icon: "move" },
  { value: "features", title: "Features", icon: "layers" },
  ...(Automation.available() ? [{ value: "events" as const, title: "Events", icon: "bell" as const }] : []),
];

const BRANDS = Cameras.brands();

const SECTION_OF: Record<string, CameraSection> = {
  name: "connection",
  host: "connection",
  user: "connection",
  password: "connection",
  rtspPort: "stream",
  channel: "stream",
  rtspPath: "stream",
  onvifPort: "ptz",
  dvripPort: "ptz",
  onvifPath: "ptz",
  apiPort: "features",
};

/** `sections` marked where `problems` has a field, and the first part marked, if any. */
export function markProblems(sections: readonly EditorSection<CameraSection>[], problems: readonly { field: string }[]) {
  const marked = new Set(problems.map((problem) => SECTION_OF[problem.field]));
  const items = sections.map((section) => ({ ...section, flagged: marked.has(section.value) }));
  return { items, first: items.find((section) => section.flagged)?.value };
}

const FEATURES_NOTE: Record<string, string> = {
  reolink: "Motion, what its AI sees and its spotlight show while the camera is connected. They and PTZ go over Reolink's API, with this login.",
  dahua:
    "PTZ goes over DVRIP, or over its HTTP API with the switch below, which switches its light too: Amcrest's light bulb cameras have no DVRIP. Motion and what the camera tells apart can come from its ONVIF events. All with this login.",
  other: "PTZ goes over ONVIF. Motion, what the camera tells apart and a light bulb camera's light can too, with this login. Press Connect to see what it has.",
};

const STREAMS = [
  { value: "0", title: "Main stream (best quality, more bandwidth)" },
  { value: "1", title: "Sub stream (lighter, lower latency)" },
];

type TextName = { [K in keyof CameraForm]: CameraForm[K] extends string ? K : never }[keyof CameraForm];

/** Typing in one of these clears `onvifRefused`, so Connect may ask a refusing ONVIF again. */
const ONVIF_LOGIN: ReadonlySet<TextName> = new Set<TextName>(["host", "user", "password", "onvifPort", "onvifPath"]);

/** A new camera's Add button: off until asked with this address and login, then Add, or Add anyway where it didn't answer. */
export function addChoice(now: string, asked: string | null, reached: string | null): { label: string; off: boolean; anyway: boolean } {
  if (now === reached) return { label: "Add", off: false, anyway: false };
  if (now === asked) return { label: "Add anyway", off: false, anyway: true };
  return { label: "Add", off: true, anyway: false };
}

const askedAs = (form: CameraForm) => JSON.stringify([form.host.trim(), form.user.trim(), form.password]);

export function CameraDialog() {
  const form = signal(Cameras.blankForm());
  const saving = signal(false);
  const failure = signal("");
  const section = signal<CameraSection>("connection");
  const adding = () => editing()?.camera === null;
  const found = signal<DetectedCamera | null>(null);
  const notFound = signal("");
  const detecting = signal(false);
  const finding = signal(false);
  const asked = signal<string | null>(null);
  const unasked = () => adding() && askedAs(form()) !== asked();
  const reached = signal<string | null>(null);
  const add = () => addChoice(askedAs(form()), asked(), reached());
  const brand = () => BRANDS.find((each) => each.key === form().brand) ?? BRANDS[0];
  const flvNote = () =>
    form().rtspPath.trim().startsWith("flv:")
      ? `One of a Reolink's FLV streams, which its web server sends on the API port (${form().apiPort.trim() || "443"}, on Features) with this login. The RTSP port and Force RTSP over TCP don't apply to it.`
      : undefined;

  const problems = computed(() => Cameras.formProblems(form()));
  const problem = (field: string) => problems().find((found) => found.field === field)?.message;
  const sections = computed(() => {
    const marked = markProblems(CAMERA_SECTIONS, problems());
    return { ...marked, items: marked.items.map((item) => ({ ...item, disabled: unasked() && item.value !== "connection" })) };
  });

  // Each opening starts from what is saved, which is what makes closing discard the edits.
  effect(() => {
    const wanted = editing();
    if (!wanted) return;
    const opened = untracked(() => (wanted.camera === null ? Cameras.blankForm() : Cameras.form(wanted.camera)));
    form.set(opened);
    asked.set(wanted.camera === null ? null : askedAs(opened));
    reached.set(null);
    failure.set("");
    section.set("connection");
    found.set(null);
    notFound.set("");
    finding.set(false);
  });

  const changeVideo = (next: Partial<EventVideoSettings>) => form.set({ ...form(), eventVideo: { ...form().eventVideo, ...next } });

  const close = () => editing.set(null);

  /** Takes a found camera into the form; its brand is asked with the login left out, so nothing is logged in to. */
  function take(camera: FoundCamera) {
    finding.set(false);
    found.set(null);
    notFound.set("");
    const picked: CameraForm = {
      ...form(),
      host: camera.host,
      onvifPort: camera.onvifPort || form().onvifPort,
      onvifPath: camera.onvifPath || form().onvifPath,
      rtspPort: camera.rtspPort || form().rtspPort,
      name: form().name.trim() ? form().name : foundTitle(camera),
      onvifRefused: false,
    };
    form.set(picked);
    Cameras.detect("", { ...picked, user: "", password: "" })
      .then((told) => form.set(withToldBrand(form(), picked, told.form)))
      // Nothing told its brand: it stays the user's to pick.
      .catch(() => {});
  }

  /** Saves the form; `pressed` is the button, since Return never adds a camera that didn't answer. */
  const save = (pressed = false) => {
    const wanted = untracked(editing);
    if (!wanted || saving()) return;
    if (adding()) {
      if (add().off) {
        if (form().host.trim()) detect();
        return;
      }
      if (add().anyway && !pressed) return;
    }
    if (problems().length > 0) {
      const unfixed = sections().first;
      if (unfixed) section.set(unfixed);
      return;
    }
    failure.set("");

    if (wanted.camera === null) {
      saving.set(true);
      Cameras.add(form())
        .then((id) => {
          close();
          cameraAdded(id, wanted.tile);
        })
        .catch((error) => failure.set(String(error?.message ?? error)))
        .finally(() => saving.set(false));
      return;
    }

    // Giving a camera its first password connects it; one the user stopped stays stopped.
    const id = wanted.camera;
    const waitingForPassword = !Cameras.ready(id);

    saving.set(true);
    Cameras.saveForm(id, form())
      .then(() => {
        if (waitingForPassword && Cameras.ready(id)) Cameras.connect(id);
        refresh();
        close();
      })
      .catch((error) => failure.set(String(error?.message ?? error)))
      .finally(() => saving.set(false));
  };

  function detect() {
    if (detecting()) return;
    detecting.set(true);
    found.set(null);
    notFound.set("");
    const sent = form();
    Cameras.detect(untracked(editing)?.camera ?? "", sent)
      .then((detected) => {
        form.set(detected.form);
        found.set(detected);
        // The answer may change the form's ports, never its address or login.
        reached.set(askedAs(sent));
      })
      .catch((error) => notFound.set(String(error?.message ?? error)))
      .finally(() => {
        asked.set(askedAs(sent));
        detecting.set(false);
      });
  }

  const Found = () => (
    <>
      <If when={detecting()}>
        <Text class="text-xs text-muted-foreground">Asking the camera…</Text>
      </If>
      <If when={found()}>
        {(shown) => <CopyableNote text={shown().summary} detail={shown().features.length > 0 ? `Has: ${shown().features.join(", ")}` : undefined} />}
      </If>
      <If when={notFound()}>
        <CopyableNote text={notFound()} tone="error" />
      </If>
    </>
  );

  const Entry = (entry: {
    label: string;
    name: TextName;
    help?: string;
    masked?: boolean;
    numeric?: boolean;
    wide?: boolean;
    readOnly?: boolean;
    autoFocus?: boolean;
    action?: JSX.Element;
    trailing?: JSX.Element;
  }) => (
    <TextField
      label={entry.label}
      value={form()[entry.name]}
      help={entry.help}
      error={problem(entry.name)}
      masked={entry.masked}
      numeric={entry.numeric}
      autoFocus={entry.autoFocus}
      readOnly={entry.readOnly}
      action={entry.action}
      trailing={entry.trailing}
      class={entry.wide ? undefined : "min-w-0 flex-1"}
      onChange={(value) => form.set({ ...form(), [entry.name]: value, onvifRefused: form().onvifRefused && !ONVIF_LOGIN.has(entry.name) })}
      onSubmit={() => save()}
    />
  );

  return (
    <>
    <EditorDialog
      title={adding() ? "Add camera" : "Camera settings"}
      icon="video"
      open={editing() !== null}
      onClose={close}
      action={<Button kind="ghost" icon="search" label="Search network for devices" onPress={() => finding.set(true)} />}
      sections={{ items: sections().items, value: section(), onChange: (value) => section.set(value) }}
      footer={
        <>
          <If when={failure()}>
            <Text class="min-w-0 flex-1 text-xs text-destructive">{failure()}</Text>
          </If>
          <Button label={detecting() ? "Connecting…" : "Connect"} accessibilityLabel="Connect to the camera" disabled={detecting() || !form().host.trim()} onPress={detect} />
          <Button label="Cancel" onPress={close} />
          <Button kind="primary" label={saving() ? "Saving…" : adding() ? add().label : "Save"} disabled={saving() || (adding() && add().off) || problems().length > 0} onPress={() => save(true)} />
        </>
      }
    >
      <If when={section() === "connection"}>
        <FormSection title="Connection">
          <Entry label="Name" name="name" wide autoFocus={adding()} />
          <Entry
            label="Camera address / hostname"
            name="host"
            wide
          />
          <FieldRow>
            <Entry label="Username" name="user" />
            <Entry label="Password" name="password" masked help={form().passwordNote || undefined} />
          </FieldRow>
          <SelectField
            label="Brand"
            items={BRANDS.map((each) => ({ value: each.key, title: each.title }))}
            value={form().brand}
            onChange={(key) => form.set(Cameras.withBrand(form(), key))}
          />
          <If when={!found() && !detecting() && !notFound()}>
            <Text class="text-xs text-muted-foreground">
              {adding()
                ? "Connect asks the camera at the address which brand it is and, with its login, what it has and which streams it sends. The other parts open once it has been asked, filled in with what it said. A camera that doesn't answer can still be added, with Add anyway."
                : "Connect asks the camera at the address which brand it is and what it has, and fills in what it finds."}
            </Text>
          </If>
          <Found />
        </FormSection>
      </If>

      <If when={section() === "stream"}>
        <FormSection title="Stream">
          <FieldRow>
            <Entry label="RTSP port" name="rtspPort" numeric />
            <Entry label="Channel" name="channel" numeric />
          </FieldRow>
          <If
            when={streamChoices(form()).length > 0}
            else={
              <>
                <SelectField label="Stream quality" items={STREAMS} value={String(form().subStream)} onChange={(value) => form.set({ ...form(), subStream: Number(value) })} />
                <Entry label="RTSP path override" name="rtspPath" help={`Blank uses ${brand().rtspPath} for channel 1's main stream`} wide />
                <Text class="text-xs text-muted-foreground">The camera has listed no streams, so its stream is reached by channel and quality, or at a path typed here. Connect asks it for them, with its login.</Text>
              </>
            }
          >
            <SelectField label="Stream" items={streamChoices(form())} value={form().rtspPath.trim()} onChange={(rtspPath) => form.set({ ...form(), rtspPath })} />
            <Entry label="Stream path" name="rtspPath" wide readOnly help={flvNote()} />
          </If>
          <CheckField label="Force RTSP over TCP (recommended)" checked={form().useTcp} onChange={(useTcp) => form.set({ ...form(), useTcp })} />
          <CheckField label="Reconnect automatically" checked={form().autoReconnect} onChange={(autoReconnect) => form.set({ ...form(), autoReconnect })} />
        </FormSection>
      </If>

      <If when={section() === "ptz"}>
        <FormSection title="PTZ">
          <If
            when={form().brand === "dahua" && !form().httpApi}
            else={
              <>
                <Text class="text-xs text-muted-foreground">
                  {form().brand === "other" ? "PTZ goes over ONVIF, on this port and path." : `PTZ goes over ${form().brand === "reolink" ? "Reolink's" : "its HTTP"} API, on the port Features names.`}
                </Text>
                <Entry label="ONVIF port" name="onvifPort" numeric />
              </>
            }
          >
            <FieldRow>
              <Entry label="ONVIF port" name="onvifPort" numeric />
              <Entry label="DVRIP port" name="dvripPort" numeric />
            </FieldRow>
          </If>
          <Entry label="ONVIF device path" name="onvifPath" wide />
        </FormSection>
      </If>

      <If when={section() === "features"}>
        <FormSection title="Features">
          <Text class="text-xs text-muted-foreground">{FEATURES_NOTE[form().brand] ?? FEATURES_NOTE.other}</Text>
          <If when={form().brand === "dahua"}>
            <CheckField
              label="PTZ and its light over its HTTP API, not DVRIP"
              checked={form().httpApi}
              onChange={(httpApi) => form.set({ ...form(), httpApi })}
            />
          </If>
          <If when={form().brand === "reolink" || (form().brand === "dahua" && form().httpApi)}>
            <FieldRow>
              <Entry label="API port" name="apiPort" numeric help={form().brand === "reolink" ? "443 for HTTPS, 80 for HTTP" : "80 for HTTP, 443 for HTTPS"} />
            </FieldRow>
            <CheckField
              label={form().brand === "reolink" ? "The API is HTTPS (Reolink's default)" : "The API is HTTPS"}
              checked={form().apiHttps}
              onChange={(apiHttps) => form.set({ ...form(), apiHttps })}
            />
          </If>
          {/* Off until asked: a failed ONVIF login counts against the camera's account. */}
          <If when={form().brand !== "reolink"}>
            <CheckField
              label="Watch it over ONVIF: motion, and its light where it has one"
              checked={form().onvifEvents}
              onChange={(onvifEvents) => form.set({ ...form(), onvifEvents })}
            />
          </If>
          <Found />
        </FormSection>
      </If>

      <If when={section() === "events"}>
        <FormSection title="Events">
          <CheckField
            label="Log what it sees on the Events page, with alerts and what each event saves"
            checked={form().logEvents}
            onChange={(logEvents) => form.set({ ...form(), logEvents })}
          />
          <Text class="text-xs text-muted-foreground">
            Off, the app never watches it for events: nothing is logged, alerted or saved for it. Its badge still says what it sees while it streams.
          </Text>
        </FormSection>
        <If when={form().logEvents}>
          <FormSection title="Event videos">
            <Text class="text-xs text-muted-foreground">
              Settings → Automation says what each event saves, and how videos are made for every camera without settings of its own. Give this camera its own to tune it on its own.
            </Text>
            <CheckField
              label="Its own event video settings, in place of Settings → Automation's"
              checked={form().ownEventVideo}
              onChange={(ownEventVideo) => form.set({ ...form(), ownEventVideo })}
            />
            <If when={form().ownEventVideo}>
              <FieldRow>
                <SelectField
                  label="Keep recording after it last saw something"
                  items={numberItems(AFTER_SECONDS, form().eventVideo.afterSeconds, secondsLabel)}
                  value={String(form().eventVideo.afterSeconds)}
                  onChange={(value) => changeVideo({ afterSeconds: Number(value) })}
                />
                <SelectField
                  label="Start before it saw something"
                  items={numberItems(BEFORE_SECONDS, form().eventVideo.beforeSeconds, beforeLabel)}
                  value={String(form().eventVideo.beforeSeconds)}
                  onChange={(value) => changeVideo({ beforeSeconds: Number(value) })}
                />
              </FieldRow>
              <FieldRow>
                <SelectField
                  label="Longest video"
                  items={numberItems(LONGEST_MINUTES, form().eventVideo.longestMinutes, longestLabel)}
                  value={String(form().eventVideo.longestMinutes)}
                  onChange={(value) => changeVideo({ longestMinutes: Number(value) })}
                />
                <SelectField label="Stream to record" items={cameraStreams(form())} value={form().eventVideo.stream} onChange={(stream) => changeVideo({ stream })} />
              </FieldRow>
              <CheckField
                label="Motion keeps a video going between its AI's detections"
                checked={form().eventVideo.motionContinues}
                onChange={(motionContinues) => changeVideo({ motionContinues })}
              />
              <Text class="text-sm font-medium text-foreground">Save videos of</Text>
              <KindChecks video={form().eventVideo} onChange={(eventVideo) => form.set({ ...form(), eventVideo })} />
            </If>
          </FormSection>
        </If>
      </If>
    </EditorDialog>
    {/* After the form, so it opens over it. */}
    <FindCamerasDialog open={finding()} onClose={() => finding.set(false)} onPick={take} />
    </>
  );
}
