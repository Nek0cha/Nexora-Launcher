import { describe, expect, it } from "vitest";
import type { Instance } from "../../bindings/Instance";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { announcement, latestInstanceTask, playStateFrom, type PlayInputs } from "./playState";

const instance: Instance = {
  id: "i1", name: "World", dirName: "World", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
};

function task(state: TaskSnapshot["state"], extra: Partial<TaskSnapshot> = {}): TaskSnapshot {
  return {
    id: "t1", kind: "launch", instanceId: "i1", state, progress: { current: 25, total: 100 },
    revision: 1, error: null, createdAt: 0, updatedAt: 1000, ...extra,
  };
}

function inputs(extra: Partial<PlayInputs> = {}): PlayInputs {
  return { instance, task: undefined, running: undefined, lastExit: undefined, ...extra };
}

describe("playStateFrom", () => {
  it("is ready for an installed instance", () => {
    expect(playStateFrom(inputs())).toEqual({ kind: "ready" });
  });
  it("asks for installation when the instance is not ready", () => {
    expect(playStateFrom(inputs({ instance: { ...instance, state: "broken" } }))).toEqual({ kind: "needsInstall" });
    expect(playStateFrom(inputs({ instance: { ...instance, state: "installing" } }))).toEqual({ kind: "needsInstall" });
  });
  it("shows install and launch progress with their phase", () => {
    expect(playStateFrom(inputs({ task: task("running", { kind: "install" }) })))
      .toEqual({ kind: "preparing", phase: "install", percent: 25, taskId: "t1" });
    expect(playStateFrom(inputs({ task: task("queued") })))
      .toEqual({ kind: "preparing", phase: "launch", percent: 25, taskId: "t1" });
    expect(playStateFrom(inputs({ task: task("cancelling") })))
      .toEqual({ kind: "cancelling", phase: "launch", percent: 25, taskId: "t1" });
  });
  it("is playing while a session is running, even after the task succeeded", () => {
    const running = { instanceId: "i1", pid: 1, launchedAt: 500 };
    expect(playStateFrom(inputs({ task: task("succeeded"), running }))).toEqual({ kind: "playing", launchedAt: 500 });
  });
  it("shows a crash after a non-zero exit until a newer task", () => {
    const lastExit = { instanceId: "i1", exitCode: 1, at: 2000 };
    expect(playStateFrom(inputs({ task: task("succeeded"), lastExit }))).toEqual({ kind: "crashed", exitCode: 1 });
    expect(playStateFrom(inputs({ task: task("running", { updatedAt: 3000 }), lastExit })).kind).toBe("preparing");
    expect(playStateFrom(inputs({ lastExit: { ...lastExit, exitCode: 0 } }))).toEqual({ kind: "ready" });
    expect(playStateFrom(inputs({ lastExit: { ...lastExit, exitCode: null } }))).toEqual({ kind: "ready" });
  });
  it("keeps a failed task until retried", () => {
    const error = { code: "net.offline", params: {}, detail: null };
    expect(playStateFrom(inputs({ task: task("failed", { error }) }))).toEqual({ kind: "failed", error });
  });
  it("returns to ready after cancellation", () => {
    expect(playStateFrom(inputs({ task: task("cancelled") }))).toEqual({ kind: "ready" });
  });
});

describe("latestInstanceTask", () => {
  it("picks the newest install or launch task of the instance", () => {
    const table = {
      a: task("succeeded", { id: "0001", kind: "install" }),
      b: task("running", { id: "0002", kind: "launch" }),
      c: task("running", { id: "0003", kind: "launch", instanceId: "other" }),
      d: task("running", { id: "0004", kind: "something-else" }),
    };
    expect(latestInstanceTask(table, "i1")?.id).toBe("0002");
    expect(latestInstanceTask(table, "nope")).toBeUndefined();
  });
});

describe("announcement", () => {
  const preparing = { kind: "preparing", phase: "launch", percent: 0, taskId: "t1" } as const;
  it("announces start, progress, playing, failure, crash and cancellation", () => {
    expect(announcement({ kind: "ready" }, preparing)).toEqual({ key: "play.announce.started" });
    expect(announcement(preparing, { ...preparing, percent: 50 })).toEqual({ key: "play.announce.progress", percent: 50 });
    expect(announcement(preparing, { kind: "playing", launchedAt: 0 })).toEqual({ key: "play.announce.playing" });
    expect(announcement(preparing, { kind: "failed", error: null })).toEqual({ key: "play.announce.failed" });
    expect(announcement({ kind: "playing", launchedAt: 0 }, { kind: "crashed", exitCode: 1 })).toEqual({ key: "play.announce.crashed" });
    expect(announcement(preparing, { kind: "ready" }, "cancelled")).toEqual({ key: "play.announce.cancelled" });
    expect(announcement(preparing, { kind: "ready" }, "succeeded")).toEqual({ key: "play.announce.done" });
    expect(announcement({ kind: "ready" }, { kind: "ready" })).toBeNull();
  });
});
