// The sidebar's All cameras put in another order: by dragging a camera by its
// grip, from its menu, and with Alt+Down while it is open on its own. Each step
// checks the order the sidebar shows (its texts, top to bottom) and the order
// saved (`Cameras.list`, which Rust reads from the library).
//
// The run starts with one camera (`test_camera` in `src/camera/hub.rs`). This
// adds two through the bridge, as the camera form would, and deletes them at
// the end. Headless tests read a model of the views, not the screen, so how
// the row lifts and the others slide is for a look by hand.
import { expect, open, test } from "nsh/test";
import type { App } from "nsh/test";
import { Cameras } from "ipcamera/native";

const NAMES = ["Camera 1", "Porch", "Yard"];
/** From one row's top to the next one's in the sidebar: 32-point rows, 4 points apart. */
const PITCH = 36;

/** The cameras in the order saved. */
const saved = () => Cameras.list().map((camera) => camera.name);

/** The cameras in the order the sidebar shows them: All cameras comes first, before the stage. */
function shown(app: App): string[] {
  const seen: string[] = [];
  for (const text of app.texts()) if (NAMES.includes(text) && !seen.includes(text)) seen.push(text);
  return seen;
}

test("a camera is dragged to a new place by its grip, moved from its menu and with the keyboard, and stays there", async () => {
  const app = await open("./App.tsx", { platform: "windows", width: 1100, height: 760 });
  const porch = await Cameras.add({ ...Cameras.blankForm(), name: "Porch", host: "10.0.0.7" });
  const yard = await Cameras.add({ ...Cameras.blankForm(), name: "Yard", host: "10.0.0.8" });
  try {
    await app.waitFor("label:Yard");
    expect(shown(app)).toEqual(["Camera 1", "Porch", "Yard"]);

    // Open on its own, the camera is selected, and its grip shows without a pointer over it, as a touch screen needs.
    await app.press("label:Yard");
    await app.dragAndHold("id:grip:Yard", { y: -2 * PITCH });
    expect(saved()).toEqual(["Camera 1", "Porch", "Yard"]);
    await app.drop();
    await app.waitUntil(() => saved()[0] === "Yard");
    expect(saved()).toEqual(["Yard", "Camera 1", "Porch"]);
    await app.waitUntil(() => shown(app)[0] === "Yard");
    expect(shown(app)).toEqual(["Yard", "Camera 1", "Porch"]);

    // Let go less than half a row away, it settles back where it was.
    await app.drag("id:grip:Yard", { y: PITCH / 3 });
    await app.wait(300);
    expect(saved()).toEqual(["Yard", "Camera 1", "Porch"]);

    // Dragged past the end of the list, it goes to the end.
    await app.drag("id:grip:Yard", { y: 10 * PITCH });
    await app.waitUntil(() => saved()[2] === "Yard");
    expect(shown(app)).toEqual(["Camera 1", "Porch", "Yard"]);

    await app.press("label:Yard options");
    await app.chooseMenuItem("Move to top");
    await app.waitUntil(() => saved()[0] === "Yard");
    expect(shown(app)).toEqual(["Yard", "Camera 1", "Porch"]);

    // Another row's `…` shows under the pointer and opens its menu without opening that camera.
    await app.hover("label:Porch");
    await app.press("label:Porch options");
    await app.chooseMenuItem("Move to top");
    await app.waitUntil(() => saved()[0] === "Porch");
    expect(saved()).toEqual(["Porch", "Yard", "Camera 1"]);
    expect(app.texts().some((text) => text.includes("10.0.0.8"))).toBe(true);
    expect(app.texts().some((text) => text.includes("10.0.0.7"))).toBe(false);

    // The keys move the camera open on its own: still Yard.
    await app.keyboard.press("alt+down");
    await app.waitUntil(() => saved()[2] === "Yard");
    expect(saved()).toEqual(["Porch", "Camera 1", "Yard"]);
    await app.waitUntil(() => shown(app)[2] === "Yard");
  } finally {
    await Cameras.delete(porch);
    await Cameras.delete(yard);
  }
});
