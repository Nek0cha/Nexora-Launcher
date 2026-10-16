import type { ErrorPayload } from "../../bindings/ErrorPayload";
import type { Instance } from "../../bindings/Instance";
import type { RunningSession } from "../../bindings/RunningSession";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import type { TaskState } from "../../bindings/TaskState";
import { sortedTasks, type TaskTable } from "../tasks/taskTable";
import type { LastExit } from "./useGameSync";

export const INSTALL_KIND = "install";
export const LAUNCH_KIND = "launch";

export type Phase = "install" | "launch";

export type PlayState =
  | { kind: "ready" }
  | { kind: "needsInstall" }
  | { kind: "preparing"; phase: Phase; percent: number; taskId: string }
  | { kind: "cancelling"; phase: Phase; percent: number; taskId: string }
  | { kind: "playing"; launchedAt: number }
  | { kind: "crashed"; exitCode: number }
  | { kind: "failed"; error: ErrorPayload | null };

export type PlayInputs = {
  instance: Instance;
  task: TaskSnapshot | undefined;
  running: RunningSession | undefined;
  lastExit: LastExit | undefined;
};

export type Announcement =
  | { key: "play.announce.started" }
  | { key: "play.announce.progress"; percent: number }
  | { key: "play.announce.done" }
  | { key: "play.announce.playing" }
  | { key: "play.announce.failed" }
  | { key: "play.announce.crashed" }
  | { key: "play.announce.cancelled" };

/** インスタンスの最新のインストールまたは起動のタスク。 */
export function latestInstanceTask(table: TaskTable, instanceId: string): TaskSnapshot | undefined {
  return sortedTasks(table).find(
    (t) => t.instanceId === instanceId && (t.kind === INSTALL_KIND || t.kind === LAUNCH_KIND),
  );
}

function percentOf(snapshot: TaskSnapshot): number {
  const { current, total } = snapshot.progress;
  if (total <= 0) return 0;
  return Math.min(100, Math.floor((current * 100) / total));
}

/** 異常終了を UI のエラーとして表す。文言は errors:game.crashed。 */
export function crashPayload(exitCode: number): ErrorPayload {
  return { code: "game.crashed", params: { exitCode }, detail: null };
}

export function playStateFrom({ instance, task, running, lastExit }: PlayInputs): PlayState {
  if (running) return { kind: "playing", launchedAt: running.launchedAt };
  if (task) {
    const phase: Phase = task.kind === INSTALL_KIND ? "install" : "launch";
    if (task.state === "queued" || task.state === "running") {
      return { kind: "preparing", phase, percent: percentOf(task), taskId: task.id };
    }
    if (task.state === "cancelling") {
      return { kind: "cancelling", phase, percent: percentOf(task), taskId: task.id };
    }
  }
  const exitAt = lastExit?.at ?? Number.NEGATIVE_INFINITY;
  if (task?.state === "failed" && task.updatedAt >= exitAt) return { kind: "failed", error: task.error };
  if (lastExit && lastExit.exitCode !== null && lastExit.exitCode !== 0
    && exitAt > (task?.updatedAt ?? Number.NEGATIVE_INFINITY)) {
    return { kind: "crashed", exitCode: lastExit.exitCode };
  }
  if (instance.state !== "ready") return { kind: "needsInstall" };
  return { kind: "ready" };
}

const isActive = (s: PlayState) => s.kind === "preparing" || s.kind === "cancelling";

/** 状態の変化から読み上げる文を決める。latest は next を作ったタスクの状態。 */
export function announcement(prev: PlayState, next: PlayState, latest?: TaskState): Announcement | null {
  if (next.kind === "preparing" && !isActive(prev)) return { key: "play.announce.started" };
  if (next.kind === "preparing" && prev.kind === "preparing") {
    const before = Math.floor(prev.percent / 25);
    const after = Math.floor(next.percent / 25);
    if (after > before && after < 4) return { key: "play.announce.progress", percent: after * 25 };
    return null;
  }
  if (next.kind === "playing" && prev.kind !== "playing") return { key: "play.announce.playing" };
  if (next.kind === "failed" && prev.kind !== "failed") return { key: "play.announce.failed" };
  if (next.kind === "crashed" && prev.kind !== "crashed") return { key: "play.announce.crashed" };
  if ((next.kind === "ready" || next.kind === "needsInstall") && isActive(prev)) {
    if (latest === "succeeded") return { key: "play.announce.done" };
    if (latest === "cancelled" || prev.kind === "cancelling") return { key: "play.announce.cancelled" };
  }
  return null;
}
