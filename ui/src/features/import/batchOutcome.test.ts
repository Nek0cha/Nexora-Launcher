import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import i18n from "../../i18n";
import { batchOutcome } from "./batchOutcome";

function task(id: string, state: TaskSnapshot["state"], error: TaskSnapshot["error"] = null): TaskSnapshot {
  return {
    id, kind: "launcher-import", instanceId: id, state, phase: null, progress: { current: 0, total: 0 },
    revision: 1, error, createdAt: 0, updatedAt: 0,
  };
}

const failure = { code: "io.failed", params: {}, detail: null };

describe("batchOutcome", () => {
  it("is not done while any task of the batch is still running or unknown", () => {
    expect(batchOutcome(["a", "b"], { a: task("a", "succeeded"), b: task("b", "running") }).done).toBe(false);
    expect(batchOutcome(["a", "b"], { a: task("a", "succeeded") }).done).toBe(false);
  });

  it("counts the succeeded and the failed once every task has ended", () => {
    const out = batchOutcome(["a", "b", "c"], {
      a: task("a", "succeeded"),
      b: task("b", "failed", failure),
      c: task("c", "cancelled"),
    });
    expect(out).toEqual({ done: true, total: 3, succeeded: 1, failed: 2, firstError: failure });
  });
});

describe("task kind labels", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  afterAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("names the launcher import task in both languages", async () => {
    expect(i18n.exists("tasks.kind.launcher-import", { lng: "en" })).toBe(true);
    expect(i18n.exists("tasks.kind.launcher-import", { lng: "ja" })).toBe(true);
    expect(i18n.exists("launcherImport.summary", { lng: "en" })).toBe(true);
    expect(i18n.exists("launcherImport.summary", { lng: "ja" })).toBe(true);
  });
});
