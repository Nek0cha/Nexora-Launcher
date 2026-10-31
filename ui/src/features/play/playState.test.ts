import { describe, expect, it } from "vitest";
import type { Instance } from "../../bindings/Instance";
import type { TaskPhase } from "../../bindings/TaskPhase";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { announcement, latestInstanceTask, playStateFrom, type PlayInputs } from "./playState";

const instance: Instance = {
  id: "i1", name: "World", dirName: "World", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
};

function task(state: TaskSnapshot["state"], extra: Partial<TaskSnapshot> = {}): TaskSnapshot {
  return {
    id: "t1", kind: "launch", instanceId: "i1", state, phase: null, progress: { current: 25, total: 100 },
    revision: 1, error: null, createdAt: 0, updatedAt: 1000, ...extra,
  };
}

function inputs(extra: Partial<PlayInputs> = {}): PlayInputs {
  return { instance, task: undefined, running: undefined, runningLoaded: true, lastExit: undefined, ...extra };
}

describe("playStateFrom", () => {
  it("is checking until the running list has loaded", () => {
    expect(playStateFrom(inputs({ runningLoaded: false }))).toEqual({ kind: "checking" });
    // 実行中のタスクと起動中のセッションは、一覧の受信前でも優先して見せる
    expect(playStateFrom(inputs({ runningLoaded: false, task: task("running") })).kind).toBe("preparing");
    const running = { instanceId: "i1", pid: 1, launchedAt: 500 };
    expect(playStateFrom(inputs({ runningLoaded: false, running })).kind).toBe("playing");
  });
  it("is ready for an installed instance", () => {
    expect(playStateFrom(inputs())).toEqual({ kind: "ready" });
  });
  it("asks for installation when the instance is not ready", () => {
    expect(playStateFrom(inputs({ instance: { ...instance, state: "broken" } }))).toEqual({ kind: "needsInstall" });
    expect(playStateFrom(inputs({ instance: { ...instance, state: "installing" } }))).toEqual({ kind: "needsInstall" });
  });
  it("shows install and launch progress with their phase", () => {
    const progress = { phase: null, percent: 25, count: null };
    expect(playStateFrom(inputs({ task: task("running", { kind: "install" }) })))
      .toEqual({ kind: "preparing", phase: "install", progress, taskId: "t1" });
    expect(playStateFrom(inputs({ task: task("queued") })))
      .toEqual({ kind: "preparing", phase: "launch", progress, taskId: "t1" });
    expect(playStateFrom(inputs({ task: task("cancelling") })))
      .toEqual({ kind: "cancelling", phase: "launch", progress, taskId: "t1" });
  });
  it("carries the task phase and its own progress", () => {
    expect(playStateFrom(inputs({ task: task("running", { phase: "downloading", progress: { current: 63, total: 100 } }) })))
      .toMatchObject({ kind: "preparing", progress: { phase: "downloading", percent: 63, count: null } });
    expect(playStateFrom(inputs({ task: task("running", { phase: "building_loader", progress: { current: 2, total: 5 } }) })))
      .toMatchObject({ kind: "preparing", progress: { phase: "building_loader", percent: null, count: { done: 2, total: 5 } } });
    expect(playStateFrom(inputs({ task: task("running", { phase: "resolving", progress: { current: 0, total: 0 } }) })))
      .toMatchObject({ kind: "preparing", progress: { phase: "resolving", percent: null, count: null } });
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

describe("a modpack import", () => {
  it("shows the instance as preparing (install) while the import task runs", () => {
    const state = playStateFrom(inputs({ task: task("running", { kind: "mrpack-import" }) }));
    expect(state.kind).toBe("preparing");
    expect(state.kind === "preparing" && state.phase).toBe("install");
  });

  it("is the newest task of the instance for the play button", () => {
    const table = { a: task("running", { id: "0005", kind: "mrpack-import" }) };
    expect(latestInstanceTask(table, "i1")?.id).toBe("0005");
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
  const preparing = {
    kind: "preparing", phase: "launch", progress: { phase: null, percent: 0, count: null }, taskId: "t1",
  } as const;
  const at = (phase: TaskPhase, percent: number | null) =>
    ({ ...preparing, progress: { phase, percent, count: null } });
  it("announces start, progress, playing, failure, crash and cancellation", () => {
    expect(announcement({ kind: "ready" }, preparing)).toEqual({ key: "play.announce.started" });
    expect(announcement(preparing, { ...preparing, progress: { ...preparing.progress, percent: 50 } }))
      .toEqual({ key: "play.announce.progress", percent: 50 });
    expect(announcement(preparing, { kind: "playing", launchedAt: 0 })).toEqual({ key: "play.announce.playing" });
    expect(announcement(preparing, { kind: "failed", error: null })).toEqual({ key: "play.announce.failed" });
    expect(announcement({ kind: "playing", launchedAt: 0 }, { kind: "crashed", exitCode: 1 })).toEqual({ key: "play.announce.crashed" });
    expect(announcement(preparing, { kind: "ready" }, "cancelled")).toEqual({ key: "play.announce.cancelled" });
    expect(announcement(preparing, { kind: "ready" }, "succeeded")).toEqual({ key: "play.announce.done" });
    expect(announcement({ kind: "ready" }, { kind: "ready" })).toBeNull();
  });
  it("announces a new phase once, and quarter steps only within the same phase", () => {
    expect(announcement(at("verifying", 80), at("downloading", 0))).toEqual({ key: "play.announce.phase", phase: "downloading" });
    // 段階が変わって数え直した直後の 25% は、前の段階の続きではない
    expect(announcement(at("downloading", 0), at("downloading", 30))).toEqual({ key: "play.announce.progress", percent: 25 });
    expect(announcement(at("downloading", 30), at("downloading", 40))).toBeNull();
    expect(announcement(at("downloading", 90), at("building_loader", null))).toEqual({ key: "play.announce.phase", phase: "building_loader" });
    expect(announcement(at("building_loader", null), at("building_loader", null))).toBeNull();
    // 始めた知らせが先（段階の知らせは重ねない）
    expect(announcement({ kind: "ready" }, at("resolving", null))).toEqual({ key: "play.announce.started" });
  });
});
