// A camera's settings, in a dialog over the window: its parts (Connection,
// Stream, PTZ, Features, and on the desktop Events) down the dialog's left
// side, the chosen part's fields beside them. `editing` says which camera, or a
// new one. Save writes them and a running stream reconnects with them; Add
// saves a new camera. Cancel, Escape and the backdrop discard the edits. A part
// with a field to fix is marked, and Return there shows it. The editor for
// several cameras at once (`bulk.tsx`) takes the same parts but Features and
// Events, which are each camera's own.
//
// Events gives the camera event video settings of its own, in place of
// Settings → Automation's as a whole (`eventvideo.ts`); until then it shows
// the app's, to start from.
//
// Connect, before Cancel, asks the camera at the address what it is, with its
// login. A new camera's other parts open once it has been asked, so they start
// from what the camera said rather than from blanks; one that doesn't answer
// opens them too, since a port there may be what it takes to reach it. Add
// stays off until the camera has been asked at the address and login the form
// holds. One that answered is added with Add; one that didn't, with Add
// anyway, for a camera that has video and nothing Connect can ask (RTSP
// alone), or that is off for now (`addChoice`). A saved camera
// was reached when it was added, so its parts are open and Save is on whatever
// changes; Connect is there to ask it again.
//
// Stream offers the streams the camera listed, and nothing else, where it
// listed any: a Reolink's FLV streams among them, which go over its API port;
// only a camera that lists none is reached by channel and quality or by a path
// typed by hand.
//
// The brand decides the RTSP path a blank override stands for and how PTZ,
// motion and a spotlight are reached; changing it brings that brand's ports
// where the old brand's still stand. A Dahua's PTZ goes over DVRIP, or over
// its HTTP API with `httpApi` on, which switches its light too. Connect asks
// the camera at the address which brand it is (without a login), and with the
// password what it is and has (`Cameras.detect`): the form takes what it
// finds. Connection reads top to bottom as a new camera is entered: its name,
// its address, the login Connect needs, the brand Connect fills in, then what
// the camera said, with a button that copies it.
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

/** A camera's settings' parts, in order: Events only where the app does anything by itself, the desktop. */
export const CAMERA_SECTIONS: readonly EditorSection<CameraSection>[] = [
  { value: "connection", title: "Connection", icon: "cctv" },
  { value: "stream", title: "Stream", icon: "video" },
  { value: "ptz", title: "PTZ", icon: "move" },
  { value: "features", title: "Features", icon: "layers" },
  ...(Automation.available() ? [{ value: "events" as const, title: "Events", icon: "bell" as const }] : []),
];

/** The brands the form offers, with the RTSP path a blank override stands for. */
const BRANDS = Cameras.brands();

/** The part each field that can have a problem is in, by the name a problem gives it. */
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

/** What the Features part says each brand gives, by its key. */
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

/** The fields of the form that hold text. */
type TextName = { [K in keyof CameraForm]: CameraForm[K] extends string ? K : never }[keyof CameraForm];

/** The fields a camera's ONVIF login is made of: typing in one has Connect ask an ONVIF that refused it again. */
const ONVIF_LOGIN: ReadonlySet<TextName> = new Set<TextName>(["host", "user", "password", "onvifPort", "onvifPath"]);

/**
 * What a new camera's Add button is, by the address and login the form holds now (`now`, an
 * `askedAs`), those Connect last asked with (`asked`) and those the camera last answered at
 * (`reached`): off until it has been asked with these; `Add` once it answered; `Add anyway`
 * when it was asked and didn't, which is how a camera Connect can't ask is still added.
 */
export function addChoice(now: string, asked: string | null, reached: string | null): { label: string; off: boolean; anyway: boolean } {
  if (now === reached) return { label: "Add", off: false, anyway: false };
  if (now === asked) return { label: "Add anyway", off: false, anyway: true };
  return { label: "Add", off: true, anyway: false };
}

/** Whom Connect asks, and as whom: a new camera's other parts open once it has been asked with these. */
const askedAs = (form: CameraForm) => JSON.stringify([form.host.trim(), form.user.trim(), form.password]);

export function CameraDialog() {
  const form = signal(Cameras.blankForm());
  const saving = signal(false);
  /** Why the last save failed. It stays until the next try, rather than closing as if it had worked. */
  const failure = signal("");
  const section = signal<CameraSection>("connection");
  const adding = () => editing()?.camera === null;
  /** What the camera said when Connect last asked it, until the form opens again. */
  const found = signal<DetectedCamera | null>(null);
  /** Why Connect reached nothing, until it is pressed again. */
  const notFound = signal("");
  const detecting = signal(false);
  /** Whether the window that lists the cameras on the network is open, over the form. */
  const finding = signal(false);
  /** The address and login the camera was last asked with (`askedAs`): a saved camera's own, and none yet for a new one. */
  const asked = signal<string | null>(null);
  /** Whether a new camera is still to be asked with this address and login, which keeps its other parts closed. */
  const unasked = () => adding() && askedAs(form()) !== asked();
  /** The address and login the camera last answered at (`askedAs`); none yet for a new one. */
  const reached = signal<string | null>(null);
  /** A new camera's Add button, as things stand. */
  const add = () => addChoice(askedAs(form()), asked(), reached());
  const brand = () => BRANDS.find((each) => each.key === form().brand) ?? BRANDS[0];
  /** What the stream path says of an FLV stream, which goes another way than the RTSP settings around it. */
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

  // Each opening starts from what is saved, or from a blank form for a new camera, which is what
  // makes closing discard the edits, and on the first part.
  effect(() => {
    const wanted = editing();
    if (!wanted) return;
    const opened = untracked(() => (wanted.camera === null ? Cameras.blankForm() : Cameras.form(wanted.camera)));
    form.set(opened);
    // A saved camera was asked when it was set up; a new one is still to be.
    asked.set(wanted.camera === null ? null : askedAs(opened));
    reached.set(null);
    failure.set("");
    section.set("connection");
    found.set(null);
    notFound.set("");
    finding.set(false);
  });

  /** Changes the camera's own event video settings. */
  const changeVideo = (next: Partial<EventVideoSettings>) => form.set({ ...form(), eventVideo: { ...form().eventVideo, ...next } });

  const close = () => editing.set(null);

  /**
   * Takes a camera found on the network into the form: its address and where its ONVIF service
   * is, and its name for a form that has none yet. Its brand follows a moment later: Connect asks
   * the camera which it is with the login left out, so nothing is logged in to, and the Brand
   * dropdown fills in when it answers (`withToldBrand`). It is still to be asked what it is, with
   * its login, which is Connect's.
   */
  function take(camera: FoundCamera) {
    finding.set(false);
    found.set(null);
    notFound.set("");
    const picked: CameraForm = {
      ...form(),
      host: camera.host,
      // What the search learned of it; the rest stays as the form has it.
      onvifPort: camera.onvifPort || form().onvifPort,
      onvifPath: camera.onvifPath || form().onvifPath,
      rtspPort: camera.rtspPort || form().rtspPort,
      name: form().name.trim() ? form().name : foundTitle(camera),
      onvifRefused: false,
    };
    form.set(picked);
    Cameras.detect("", { ...picked, user: "", password: "" })
      .then((told) => form.set(withToldBrand(form(), picked, told.form)))
      // Only its video answered, or nothing did: the brand stays the user's to pick.
      .catch(() => {});
  }

  /** Saves the form. `pressed` is the button itself: Return in a field never adds a camera that didn't answer. */
  const save = (pressed = false) => {
    const wanted = untracked(editing);
    if (!wanted || saving()) return;
    if (adding()) {
      // Return in a new camera still to be asked asks it, since Add is off until it has been.
      if (add().off) {
        if (form().host.trim()) detect();
        return;
      }
      if (add().anyway && !pressed) return;
    }
    if (problems().length > 0) {
      // Return in a field while a part has something to fix shows the first such part.
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

    // A password is what keeps a camera from connecting by itself, so giving it one connects now.
    // A stream the user stopped stays stopped.
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

  /**
   * Asks the camera at the address what it is, and takes what it finds into the form. Asked, a
   * new camera's other parts open, whatever it said; only one that answered can be added.
   */
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
        // What it found may have changed the form's ports, never its address or login.
        reached.set(askedAs(sent));
      })
      .catch((error) => notFound.set(String(error?.message ?? error)))
      .finally(() => {
        asked.set(askedAs(sent));
        detecting.set(false);
      });
  }

  /** What the camera said last, on Connection and on Features, each with a button that copies it. */
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

  /** A text field of the form. */
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
          {/* A new camera starts with its name. */}
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
          {/* The camera's own list where it gave one, with the chosen stream's path under it to read; only without one, the path built from the channel and quality or typed by hand. */}
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
            {/* The chosen stream's path, to read: the list above is what sets it. */}
            <Entry label="Stream path" name="rtspPath" wide readOnly help={flvNote()} />
          </If>
          <CheckField label="Force RTSP over TCP (recommended)" checked={form().useTcp} onChange={(useTcp) => form.set({ ...form(), useTcp })} />
          <CheckField label="Reconnect automatically" checked={form().autoReconnect} onChange={(autoReconnect) => form.set({ ...form(), autoReconnect })} />
        </FormSection>
      </If>

      <If when={section() === "ptz"}>
        <FormSection title="PTZ">
          {/* A Dahua moves over DVRIP or its HTTP API, a Reolink over its own API (both APIs' ports are on Features), any other camera over ONVIF. */}
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
          {/* Off until asked: not every camera has ONVIF on, and a failed ONVIF login counts against the account. */}
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
        {/* With its events off, nothing would make a video. */}
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
