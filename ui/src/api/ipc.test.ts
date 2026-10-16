import { describe, expect, it } from "vitest";
import { IpcError, toErrorPayload } from "./ipc";

describe("toErrorPayload", () => {
  it("preserves valid payloads and wrapped errors", () => {
    const payload = { code: "net.http_status", params: { step: 1 }, detail: null };
    expect(toErrorPayload(payload)).toBe(payload);
    expect(toErrorPayload(new IpcError(payload))).toBe(payload);
    for (const detail of [undefined, null, "diagnostic"]) {
      const value = { code: "net.http_status", params: {}, detail };
      expect(toErrorPayload(value)).toBe(value);
    }
  });

  it.each([
    null, undefined, "failure", new Error("failure"),
    { code: 1, params: {}, detail: null },
    { code: "bad", params: null },
    { code: "bad", params: [] },
    { code: "bad", params: {}, detail: 42 },
    { code: "bad", params: {}, detail: {} },
  ])("normalizes invalid payloads: %s", (value) => {
    expect(toErrorPayload(value)).toEqual({ code: "app.unexpected", params: {}, detail: String(value) });
  });
});
