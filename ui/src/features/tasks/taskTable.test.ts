import { describe, expect, it } from "vitest";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { activeCount, mergeList, mergeSnapshot, removeTask, sortedTasks } from "./taskTable";

function snap(id: string, revision: number, state: TaskSnapshot["state"] = "running"): TaskSnapshot {
  return {
    id,
    kind: "demo",
    state,
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
