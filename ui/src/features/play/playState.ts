import type { ErrorPayload } from "../../bindings/ErrorPayload";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { sortedTasks, type TaskTable } from "../tasks/taskTable";

export const DEMO_KIND = "demo";
export const DONE_DISPLAY_MS = 3000;

export type PlayState =
  | { kind: "ready" }
  | { kind: "preparing"; percent: number; taskId: string }
  | { kind: "cancelling"; percent: number; taskId: string }
  | { kind: "done"; startedAt: number }
  | { kind: "failed"; error: ErrorPayload | null };

export type Announcement =
  | { key: "play.announce.started" }
  | { key: "play.announce.progress"; percent: number }
  | { key: "play.announce.done" }
  | { key: "play.announce.failed" }
  | { key: "play.announce.cancelled" };

export function latestDemo(table: TaskTable): TaskSnapshot | undefined {
  return sortedTasks(table).find((t) => t.kind === DEMO_KIND);
}

function percentOf(snapshot: TaskSnapshot): number {
  const { current, total } = snapshot.progress;
  if (total <= 0) return 0;
  return Math.min(100, Math.floor((current * 100) / total));
}

export function playStateFrom(snapshot: TaskSnapshot | undefined, nowMs: number): PlayState {
  if (!snapshot) return { kind: "ready" };
  switch (snapshot.state) {
    case "queued":
    case "running":
      return { kind: "preparing", percent: percentOf(snapshot), taskId: snapshot.id };
    case "cancelling":
      return { kind: "cancelling", percent: percentOf(snapshot), taskId: snapshot.id };
    case "succeeded":
      return nowMs - snapshot.updatedAt < DONE_DISPLAY_MS
        ? { kind: "done", startedAt: snapshot.updatedAt }
        : { kind: "ready" };
    case "failed":
      return { kind: "failed", error: snapshot.error };
    case "cancelled":
      return { kind: "ready" };
  }
}

export function announcement(prev: PlayState, next: PlayState): Announcement | null {
  const wasActive = prev.kind === "preparing" || prev.kind === "cancelling";
  if (next.kind === "preparing" && !wasActive) return { key: "play.announce.started" };
  if (next.kind === "preparing" && prev.kind === "preparing") {
    const before = Math.floor(prev.percent / 25);
    const after = Math.floor(next.percent / 25);
    if (after > before && after < 4) return { key: "play.announce.progress", percent: after * 25 };
    return null;
  }
  if (next.kind === "done" && prev.kind !== "done") return { key: "play.announce.done" };
  if (next.kind === "failed" && prev.kind !== "failed") return { key: "play.announce.failed" };
  if (next.kind === "ready" && prev.kind === "cancelling") return { key: "play.announce.cancelled" };
  return null;
}
