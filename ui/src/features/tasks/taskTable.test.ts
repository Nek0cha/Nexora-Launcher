import { describe, expect, it } from "vitest";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { activeCount, mergeList, mergeSnapshot, phaseLabel, progressView, removeTask, sortedTasks } from "./taskTable";

function snap(id: string, revision: number, state: TaskSnapshot["state"] = "running"): TaskSnapshot {
  return {
    id,
    kind: "install",
    instanceId: null,
    state,
    phase: null,
    progress: { current: revision, total: 100 },
    revision,
    error: null,
    createdAt: 1,
    updatedAt: revision,
  };
}

describe("taskTable", () => {
  it("adds unknown tasks", () => {
    expect(mergeSnapshot({}, snap("a", 1))).toEqual({ a: snap("a", 1) });
  });

  it("ignores older or equal revisions (out-of-order responses)", () => {
    const table = mergeSnapshot({}, snap("a", 5));
    expect(mergeSnapshot(table, snap("a", 4))).toBe(table);
    expect(mergeSnapshot(table, snap("a", 5))).toBe(table);
    expect(mergeSnapshot(table, snap("a", 6)).a?.revision).toBe(6);
  });

  it("mergeList drops tasks missing from the list but keeps newer local revisions", () => {
    const table = { a: snap("a", 9), b: snap("b", 2) };
    const merged = mergeList(table, [snap("a", 3), snap("c", 1)]);
    expect(Object.keys(merged).sort()).toEqual(["a", "c"]);
    expect(merged.a?.revision).toBe(9);
  });

  it("removes tasks", () => {
    expect(removeTask({ a: snap("a", 1) }, "a")).toEqual({});
  });

  it("counts only unfinished tasks", () => {
    const table = { a: snap("a", 1, "running"), b: snap("b", 1, "succeeded"), c: snap("c", 1, "cancelling") };
    expect(activeCount(table)).toBe(2);
  });

  it("sorts newest first by id (UUID v7)", () => {
    const table = { "0001": snap("0001", 1), "0003": snap("0003", 1), "0002": snap("0002", 1) };
    expect(sortedTasks(table).map((t) => t.id)).toEqual(["0003", "0002", "0001"]);
  });
});

it("mergeList preserves only explicitly protected missing ids without mutating inputs", () => {
  const table = { a: snap("a", 9), b: snap("b", 2), c: snap("c", 3) };
  const list = [snap("a", 1)];
  const preserve = new Set(["b", "unknown"]);
  expect(mergeList(table, list, preserve)).toEqual({ a: snap("a", 9), b: snap("b", 2) });
  expect(Object.keys(table)).toEqual(["a", "b", "c"]);
  expect(list).toEqual([snap("a", 1)]);
  expect([...preserve]).toEqual(["b", "unknown"]);
});

describe("progressView", () => {
  const at = (phase: TaskSnapshot["phase"], current: number, total: number): TaskSnapshot =>
    ({ ...snap("a", 1), phase, progress: { current, total } });

  it("gives a percent for verifying and downloading only", () => {
    expect(progressView(at("verifying", 42, 100))).toEqual({ phase: "verifying", percent: 42, count: null });
    expect(progressView(at("downloading", 2, 3))).toEqual({ phase: "downloading", percent: 66, count: null });
    expect(progressView(at("downloading", 0, 0))).toEqual({ phase: "downloading", percent: null, count: null });
  });

  it("counts processors while building the loader, without a percent", () => {
    expect(progressView(at("building_loader", 2, 5))).toEqual({ phase: "building_loader", percent: null, count: { done: 2, total: 5 } });
    expect(progressView(at("building_loader", 0, 0))).toEqual({ phase: "building_loader", percent: null, count: null });
  });

  it("has no determinate progress while resolving, finishing or launching", () => {
    for (const phase of ["resolving", "finishing", "launching"] as const) {
      expect(progressView(at(phase, 5, 10))).toEqual({ phase, percent: null, count: null });
    }
  });

  it("keeps the plain percent for tasks without a phase", () => {
    expect(progressView(at(null, 40, 100))).toEqual({ phase: null, percent: 40, count: null });
    expect(progressView(at(null, 0, 0))).toEqual({ phase: null, percent: null, count: null });
    expect(progressView(at(null, 150, 100)).percent).toBe(100);
  });
});

describe("phaseLabel", () => {
  it("names each phase with its number when there is one", () => {
    expect(phaseLabel({ phase: "verifying", percent: 42, count: null }))
      .toEqual({ key: "play.step.verifyingPercent", values: { percent: 42 } });
    expect(phaseLabel({ phase: "verifying", percent: null, count: null }))
      .toEqual({ key: "play.step.verifying", values: {} });
    expect(phaseLabel({ phase: "downloading", percent: 63, count: null }))
      .toEqual({ key: "play.step.downloadingPercent", values: { percent: 63 } });
    expect(phaseLabel({ phase: "building_loader", percent: null, count: { done: 2, total: 5 } }))
      .toEqual({ key: "play.step.buildingLoaderCount", values: { done: 2, total: 5 } });
    expect(phaseLabel({ phase: "building_loader", percent: null, count: null }))
      .toEqual({ key: "play.step.buildingLoader", values: {} });
    expect(phaseLabel({ phase: "resolving", percent: null, count: null })).toEqual({ key: "play.step.resolving", values: {} });
    expect(phaseLabel({ phase: "finishing", percent: null, count: null })).toEqual({ key: "play.step.finishing", values: {} });
    expect(phaseLabel({ phase: "launching", percent: null, count: null })).toEqual({ key: "play.step.launching", values: {} });
  });

  it("has no label without a phase", () => {
    expect(phaseLabel({ phase: null, percent: 40, count: null })).toBeNull();
  });
});
