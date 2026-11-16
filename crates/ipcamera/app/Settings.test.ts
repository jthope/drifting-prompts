// A new camera's Add button, by what Connect has asked and heard: `addChoice`
// in `views/settings.tsx`. A test run reaches no camera and Connect always
// answers there, so the button after a Connect that failed is checked here.
import { expect, test } from "nsh/test";
import { addChoice } from "./views/settings";

const HERE = "10.0.0.7 as admin";
const ELSEWHERE = "10.0.0.8 as admin";

test("Add is off until the camera has been asked at the address and login the form holds", () => {
  expect(addChoice(HERE, null, null)).toEqual({ label: "Add", off: true, anyway: false });
  // Asked, and even reached, at another address: this one is still to be asked.
  expect(addChoice(HERE, ELSEWHERE, ELSEWHERE).off).toBe(true);
});

test("a camera that answered is added with Add", () => {
  expect(addChoice(HERE, HERE, HERE)).toEqual({ label: "Add", off: false, anyway: false });
});

test("one that was asked and didn't answer can still be added, with Add anyway", () => {
  expect(addChoice(HERE, HERE, null)).toEqual({ label: "Add anyway", off: false, anyway: true });
  // It answered at another address before, which says nothing of this one.
  expect(addChoice(HERE, HERE, ELSEWHERE).anyway).toBe(true);
});
