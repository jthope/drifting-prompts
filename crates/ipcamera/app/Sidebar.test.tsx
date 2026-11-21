import { expect, open, test } from "nsh/test";
import type { App } from "nsh/test";
import { Cameras } from "ipcamera/native";

const NAMES = ["Camera 1", "Porch", "Yard"];
/** From one row's top to the next one's in the sidebar: 32-point rows, 4 points apart. */
const PITCH = 36;

const saved = () => Cameras.list().map((camera) => camera.name);

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

    await app.press("label:Yard");
    await app.dragAndHold("id:grip:Yard", { y: -2 * PITCH });
    expect(saved()).toEqual(["Camera 1", "Porch", "Yard"]);
    await app.drop();
    await app.waitUntil(() => saved()[0] === "Yard");
    expect(saved()).toEqual(["Yard", "Camera 1", "Porch"]);
    await app.waitUntil(() => shown(app)[0] === "Yard");
    expect(shown(app)).toEqual(["Yard", "Camera 1", "Porch"]);

    await app.drag("id:grip:Yard", { y: PITCH / 3 });
    await app.wait(300);
    expect(saved()).toEqual(["Yard", "Camera 1", "Porch"]);

    await app.drag("id:grip:Yard", { y: 10 * PITCH });
    await app.waitUntil(() => saved()[2] === "Yard");
    expect(shown(app)).toEqual(["Camera 1", "Porch", "Yard"]);

    await app.press("label:Yard options");
    await app.chooseMenuItem("Move to top");
    await app.waitUntil(() => saved()[0] === "Yard");
    expect(shown(app)).toEqual(["Yard", "Camera 1", "Porch"]);

    await app.hover("label:Porch");
    await app.press("label:Porch options");
    await app.chooseMenuItem("Move to top");
    await app.waitUntil(() => saved()[0] === "Porch");
    expect(saved()).toEqual(["Porch", "Yard", "Camera 1"]);
    expect(app.texts().some((text) => text.includes("10.0.0.8"))).toBe(true);
    expect(app.texts().some((text) => text.includes("10.0.0.7"))).toBe(false);

    await app.keyboard.press("alt+down");
    await app.waitUntil(() => saved()[2] === "Yard");
    expect(saved()).toEqual(["Porch", "Camera 1", "Yard"]);
    await app.waitUntil(() => shown(app)[2] === "Yard");
  } finally {
    await Cameras.delete(porch);
    await Cameras.delete(yard);
  }
});
