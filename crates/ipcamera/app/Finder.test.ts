// What a camera found on the network is listed as: `views/finder.tsx`.
import { expect, test } from "nsh/test";
import { Cameras, type FoundCamera } from "ipcamera/native";
import { foundDetail, foundTitle, withToldBrand } from "./views/finder";

const found = (name: string, model: string, details: string[] = []): FoundCamera => ({ host: "10.0.0.5", name, model, onvifPort: "80", onvifPath: "/onvif/device_service", rtspPort: "", details });

test("a found camera is listed by its model, with its name and address under it", () => {
  const camera = found("Front Door", "IPC-123");
  expect(foundTitle(camera)).toBe("IPC-123");
  expect(foundDetail(camera)).toBe("Front Door · 10.0.0.5");
});

test("what else it says of itself follows its address, and a name that is its model isn't said twice", () => {
  const camera = found("IPC-123", "IPC-123", ["Serial: 42", "MAC: aa:bb"]);
  expect(foundTitle(camera)).toBe("IPC-123");
  expect(foundDetail(camera)).toBe("10.0.0.5 · Serial: 42 · MAC: aa:bb");
  // One that says no model is listed by its name.
  expect(foundTitle(found("Front Door", ""))).toBe("Front Door");
  expect(foundDetail(found("Front Door", ""))).toBe("10.0.0.5");
});

test("a device found by its video alone is listed by its address, with how it was found beneath", () => {
  const plain = found("", "", ["Video over RTSP"]);
  expect(foundTitle(plain)).toBe("10.0.0.5");
  expect(foundDetail(plain)).toBe("Video over RTSP");
});

test("one with no name is listed by its model, and one that says neither by its address alone", () => {
  expect(foundTitle(found("", "IPC-123"))).toBe("IPC-123");
  expect(foundDetail(found("", "IPC-123"))).toBe("10.0.0.5");

  expect(foundTitle(found("", ""))).toBe("10.0.0.5");
  expect(foundDetail(found("", ""))).toBe("");
});

test("a picked camera takes the brand Connect told, with its ports, where the user changed nothing since", () => {
  const picked = { ...Cameras.blankForm(), host: "10.0.0.5", brand: "dahua", onvifPort: "8000" };
  const told = { ...Cameras.withBrand(picked, "reolink"), apiPort: "80", apiHttps: false };

  const filled = withToldBrand({ ...picked, user: "admin" }, picked, told);
  expect(filled.brand).toBe("reolink");
  expect([filled.apiPort, filled.apiHttps]).toEqual(["80", false]);
  expect(filled.onvifPort).toBe("8000");
  // What was typed meanwhile stays.
  expect(filled.user).toBe("admin");

  // A port typed since the pick stays the user's.
  expect(withToldBrand({ ...picked, apiPort: "8443" }, picked, told).apiPort).toBe("8443");
  // A brand picked by hand, or another address, and the answer comes too late to matter.
  expect(withToldBrand({ ...picked, brand: "other" }, picked, told).brand).toBe("other");
  expect(withToldBrand({ ...picked, host: "10.0.0.9" }, picked, told).brand).toBe("dahua");
});
