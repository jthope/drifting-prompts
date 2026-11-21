import { expect, test } from "nsh/test";
import { addChoice } from "./views/settings";

const HERE = "10.0.0.7 as admin";
const ELSEWHERE = "10.0.0.8 as admin";

test("Add is off until the camera has been asked at the address and login the form holds", () => {
  expect(addChoice(HERE, null, null)).toEqual({ label: "Add", off: true, anyway: false });
  expect(addChoice(HERE, ELSEWHERE, ELSEWHERE).off).toBe(true);
});

test("a camera that answered is added with Add", () => {
  expect(addChoice(HERE, HERE, HERE)).toEqual({ label: "Add", off: false, anyway: false });
});

test("one that was asked and didn't answer can still be added, with Add anyway", () => {
  expect(addChoice(HERE, HERE, null)).toEqual({ label: "Add anyway", off: false, anyway: true });
  expect(addChoice(HERE, HERE, ELSEWHERE).anyway).toBe(true);
});
