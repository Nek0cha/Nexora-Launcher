import { describe, expect, it } from "vitest";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { announcement, DONE_DISPLAY_MS, latestDemo, playStateFrom, type PlayState } from "./playState";

function snap(state: TaskSnapshot["state"], current = 0, extra: Partial<TaskSnapshot> = {}): TaskSnapshot {
  return {
    id: "t1",
    kind: "demo",
    state,
    progress: { current, total: 100 },
    revision: 1,
    error: null,
    createdAt: 0,
    updatedAt: 1000,
    ...extra,
  };
}

describe("playStateFrom", () => {
  it("is ready without a task", () => {
    expect(playStateFrom(undefined, 0)).toEqual({ kind: "ready" });
  });
  it("maps queued and running to preparing with percent", () => {
    expect(playStateFrom(snap("queued"), 0)).toEqual({ kind: "preparing", percent: 0, taskId: "t1" });
    expect(playStateFrom(snap("running", 37), 0)).toEqual({ kind: "preparing", percent: 37, taskId: "t1" });
  });
  it("handles a zero total", () => {
    const s = snap("running", 5, { progress: { current: 5, total: 0 } });
    expect(playStateFrom(s, 0)).toEqual({ kind: "preparing", percent: 0, taskId: "t1" });
  });
  it("maps cancelling", () => {
    expect(playStateFrom(snap("cancelling", 40), 0)).toEqual({ kind: "cancelling", percent: 40, taskId: "t1" });
  });
  it("shows done for a short time, then ready", () => {
    expect(playStateFrom(snap("succeeded", 100), 1000 + DONE_DISPLAY_MS - 1)).toEqual({ kind: "done", startedAt: 1000 });
    expect(playStateFrom(snap("succeeded", 100), 1000 + DONE_DISPLAY_MS)).toEqual({ kind: "ready" });
  });
  it("keeps failed until retried", () => {
    const error = { code: "demo.failed", params: {}, detail: null };
    expect(playStateFrom(snap("failed", 30, { error }), 999_999)).toEqual({ kind: "failed", error });
  });
  it("maps cancelled to ready", () => {
    expect(playStateFrom(snap("cancelled", 50), 0)).toEqual({ kind: "ready" });
  });
});

describe("latestDemo", () => {
  it("picks the newest demo task", () => {
    const table = {
      a: snap("succeeded", 100, { id: "0001" }),
      b: snap("running", 10, { id: "0002" }),
      c: snap("running", 10, { id: "0003", kind: "other" }),
    };
    expect(latestDemo(table)?.id).toBe("0002");
  });
});

describe("announcement", () => {
  const ready: PlayState = { kind: "ready" };
  const prep = (percent: number): PlayState => ({ kind: "preparing", percent, taskId: "t1" });

  it("announces start, quarter milestones, done, failed and cancelled", () => {
    expect(announcement(ready, prep(0))).toEqual({ key: "play.announce.started" });
    expect(announcement(prep(20), prep(26))).toEqual({ key: "play.announce.progress", percent: 25 });
    expect(announcement(prep(26), prep(30))).toBeNull();
    expect(announcement(prep(99), { kind: "done", startedAt: 0 })).toEqual({ key: "play.announce.done" });
    expect(announcement(prep(10), { kind: "failed", error: null })).toEqual({ key: "play.announce.failed" });
    expect(announcement({ kind: "cancelling", percent: 10, taskId: "t1" }, ready)).toEqual({ key: "play.announce.cancelled" });
  });

  it("announces cancel when the cancelled snapshot arrives without a cancelling one", () => {
    expect(announcement(prep(40), ready, "cancelled")).toEqual({ key: "play.announce.cancelled" });
  });

  it("announces done when the success arrives after the done window", () => {
    expect(announcement(prep(90), ready, "succeeded")).toEqual({ key: "play.announce.done" });
  });

  it("does not announce 100% as a milestone", () => {
    expect(announcement(prep(80), prep(100))).toBeNull();
  });

  it("announces start again when retrying after a failure", () => {
    expect(announcement({ kind: "failed", error: null }, prep(0))).toEqual({ key: "play.announce.started" });
  });

  it("announces only the highest milestone crossed in one jump", () => {
    expect(announcement(prep(10), prep(80))).toEqual({ key: "play.announce.progress", percent: 75 });
  });

  it("stays quiet when an expired success reloads as ready", () => {
    expect(announcement(ready, ready, "succeeded")).toBeNull();
  });
});
