import { describe, expect, it } from "vitest";
import type { Instance } from "../../bindings/Instance";
import { hasMemoryFlag, isDirty, memoryError, toForm, toSettings, type SettingsForm } from "./settingsForm";

const instance = (patch: Partial<Instance> = {}): Instance => ({
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
  ...patch,
});

const form = (patch: Partial<SettingsForm> = {}): SettingsForm => ({ ...toForm(instance()), ...patch });

describe("toForm and toSettings", () => {
  it("round_trips_manual_values", () => {
    const i = instance({ memoryMode: "manual", memoryMb: 6144, javaMode: "manual", javaPath: "C:\\java.exe", jvmArgs: "-Xss1M" });
    expect(toSettings(toForm(i))).toEqual({
      name: "Survival", memoryMode: "manual", memoryMb: 6144, javaMode: "manual", javaPath: "C:\\java.exe", jvmArgs: "-Xss1M",
    });
  });

  it("auto_modes_send_null_manual_values", () => {
    const s = toSettings(form({ memoryMb: 8192, javaPath: "C:\\java.exe" }));
    expect(s.memoryMb).toBeNull();
    expect(s.javaPath).toBeNull();
  });

  it("trims_text_and_strips_quotes_around_java_path", () => {
    const s = toSettings(form({ name: "  New  ", javaMode: "manual", javaPath: '  "C:\\Program Files\\java.exe"  ', jvmArgs: " -Xss1M " }));
    expect(s.name).toBe("New");
    expect(s.javaPath).toBe("C:\\Program Files\\java.exe");
    expect(s.jvmArgs).toBe("-Xss1M");
  });

  it("keeps_a_lone_quote_in_java_path", () => {
    expect(toSettings(form({ javaMode: "manual", javaPath: '"C:\\java.exe' })).javaPath).toBe('"C:\\java.exe');
  });
});

describe("isDirty", () => {
  it("is_false_for_the_same_form", () => {
    expect(isDirty(form(), form())).toBe(false);
  });

  it("is_true_when_a_field_changes", () => {
    expect(isDirty(form(), form({ name: "Other" }))).toBe(true);
    expect(isDirty(form(), form({ memoryMode: "manual" }))).toBe(true);
    expect(isDirty(form(), form({ jvmArgs: "-Xss1M" }))).toBe(true);
  });

  it("ignores_manual_values_while_auto", () => {
    expect(isDirty(form(), form({ memoryMb: 1024, javaPath: "x" }))).toBe(false);
  });
});

describe("hasMemoryFlag", () => {
  it("detects_xmx", () => {
    expect(hasMemoryFlag("-Xmx2G")).toBe(true);
    expect(hasMemoryFlag("-Xss1M -Xmx2G")).toBe(true);
    expect(hasMemoryFlag('"-Xmx8G"')).toBe(true);
  });

  it("ignores_other_flags", () => {
    expect(hasMemoryFlag("-Xms1G")).toBe(false);
    expect(hasMemoryFlag("-Dxmx=1")).toBe(false);
    expect(hasMemoryFlag('-Dfoo="a b -Xmx4G"')).toBe(false);
    expect(hasMemoryFlag("")).toBe(false);
  });
});

describe("memoryError", () => {
  it("accepts_the_bounds", () => {
    expect(memoryError(form({ memoryMode: "manual", memoryMb: 512 }))).toBeNull();
    expect(memoryError(form({ memoryMode: "manual", memoryMb: 65536 }))).toBeNull();
  });

  it("rejects_out_of_range_when_manual", () => {
    expect(memoryError(form({ memoryMode: "manual", memoryMb: 511 }))).toBe("range");
    expect(memoryError(form({ memoryMode: "manual", memoryMb: 65537 }))).toBe("range");
  });

  it("ignores_the_value_when_auto", () => {
    expect(memoryError(form({ memoryMode: "auto", memoryMb: 1 }))).toBeNull();
  });
});
