import { describe, expect, it } from "vitest";
import type { PendingView } from "../../bindings/PendingView";
import { bulkCalls, conflictCalls, runSerially, scopeProposals, singleCalls } from "./proposals";

const view: PendingView = {
  single: [
    { instanceIds: ["a"], fieldId: "fov", value: "0.9" },
    { instanceIds: ["a", "b"], fieldId: "gamma", value: "1.0" },
    { instanceIds: ["c"], fieldId: "autoJump", value: "false" },
  ],
  conflicts: [
    {
      fieldId: "mouseSensitivity",
      entries: [
        { instanceId: "a", value: "0.7" },
        { instanceId: "b", value: "0.3" },
        { instanceId: "c", value: "0.7" },
      ],
    },
    { fieldId: "chatScale", entries: [{ instanceId: "b", value: "0.5" }, { instanceId: "c", value: "0.9" }] },
  ],
};

describe("scopeProposals", () => {
  it("keeps everything for the shared settings", () => {
    expect(scopeProposals(view, null)).toEqual(view);
  });

  it("keeps only what involves the instance", () => {
    const scoped = scopeProposals(view, "a");
    expect(scoped.single.map((s) => s.fieldId)).toEqual(["fov", "gamma"]);
    expect(scoped.conflicts.map((c) => c.fieldId)).toEqual(["mouseSensitivity"]);
  });
});

describe("singleCalls", () => {
  const gamma = view.single[1]!;

  it("applies for every instance that proposed the value", () => {
    expect(singleCalls(gamma, "applyToProfile", "a")).toEqual([
      { instanceId: "a", fieldId: "gamma", resolution: "applyToProfile" },
      { instanceId: "b", fieldId: "gamma", resolution: "applyToProfile" },
    ]);
  });

  it("keeps or ignores only for the instance on its card", () => {
    expect(singleCalls(gamma, "keepForInstance", "b")).toEqual([
      { instanceId: "b", fieldId: "gamma", resolution: "keepForInstance" },
    ]);
    expect(singleCalls(gamma, "ignore", "a")).toEqual([{ instanceId: "a", fieldId: "gamma", resolution: "ignore" }]);
  });

  it("keeps or ignores for every listed instance on the shared settings screen", () => {
    expect(singleCalls(gamma, "ignore", null)).toEqual([
      { instanceId: "a", fieldId: "gamma", resolution: "ignore" },
      { instanceId: "b", fieldId: "gamma", resolution: "ignore" },
    ]);
  });
});

describe("conflictCalls", () => {
  const conflict = view.conflicts[0]!;

  it("applies the chosen value first, with chosenValue, then settles the others", () => {
    expect(conflictCalls(conflict, { choose: "0.3" }, "a")).toEqual([
      { instanceId: "b", fieldId: "mouseSensitivity", resolution: "applyToProfile", chosenValue: "0.3" },
      { instanceId: "a", fieldId: "mouseSensitivity", resolution: "ignore" },
      { instanceId: "c", fieldId: "mouseSensitivity", resolution: "ignore" },
    ]);
  });

  it("ignores only the card's instance, or every entry on the screen", () => {
    expect(conflictCalls(conflict, "ignore", "c")).toEqual([
      { instanceId: "c", fieldId: "mouseSensitivity", resolution: "ignore" },
    ]);
    expect(conflictCalls(conflict, "ignore", null).map((c) => c.instanceId)).toEqual(["a", "b", "c"]);
  });

  it("does nothing for a value nobody proposed", () => {
    expect(conflictCalls(conflict, { choose: "0.1" }, null)).toEqual([]);
  });
});

describe("bulkCalls", () => {
  it("applies every single proposal and leaves the conflicts out", () => {
    const calls = bulkCalls(scopeProposals(view, "a"), "a");
    expect(calls).toEqual([
      { instanceId: "a", fieldId: "fov", resolution: "applyToProfile" },
      { instanceId: "a", fieldId: "gamma", resolution: "applyToProfile" },
      { instanceId: "b", fieldId: "gamma", resolution: "applyToProfile" },
    ]);
    expect(calls.some((c) => c.fieldId === "mouseSensitivity" || c.fieldId === "chatScale")).toBe(false);
  });
});

describe("runSerially", () => {
  it("starts each call only after the previous one finished", async () => {
    const log: string[] = [];
    const resolvers: Array<() => void> = [];
    const run = runSerially(
      [
        { instanceId: "a", fieldId: "fov", resolution: "applyToProfile" },
        { instanceId: "b", fieldId: "fov", resolution: "ignore" },
      ],
      (call) => {
        log.push(`start ${call.instanceId}`);
        return new Promise<void>((resolve) => {
          resolvers.push(() => {
            log.push(`end ${call.instanceId}`);
            resolve();
          });
        });
      },
    );
    await Promise.resolve();
    expect(log).toEqual(["start a"]);
    resolvers[0]!();
    await new Promise((r) => setTimeout(r, 0));
    expect(log).toEqual(["start a", "end a", "start b"]);
    resolvers[1]!();
    await run;
    expect(log).toEqual(["start a", "end a", "start b", "end b"]);
  });

  it("stops at the first failure", async () => {
    const seen: string[] = [];
    await expect(
      runSerially(
        [
          { instanceId: "a", fieldId: "fov", resolution: "applyToProfile" },
          { instanceId: "b", fieldId: "fov", resolution: "applyToProfile" },
        ],
        async (call) => {
          seen.push(call.instanceId);
          throw new Error("boom");
        },
      ),
    ).rejects.toThrow("boom");
    expect(seen).toEqual(["a"]);
  });
});
