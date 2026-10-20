import type { ErrorPayload } from "../../bindings/ErrorPayload";
import type { Instance } from "../../bindings/Instance";
import type { RunningSession } from "../../bindings/RunningSession";
import type { TaskPhase } from "../../bindings/TaskPhase";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import type { TaskState } from "../../bindings/TaskState";
import { progressView, sortedTasks, type ProgressView, type TaskTable } from "../tasks/taskTable";
import type { LastExit } from "./useGameSync";

export const INSTALL_KIND = "install";
export const LAUNCH_KIND = "launch";

export type Phase = "install" | "launch";

export type PlayState =
  | { kind: "ready" }
  | { kind: "needsInstall" }
  /** 起動中のセッションの一覧がまだ届いていない。押しても何もしない。 */
  | { kind: "checking" }
  /** progress はタスクのいまの段階と、その段階の進み具合。 */
  | { kind: "preparing"; phase: Phase; progress: ProgressView; taskId: string }
  | { kind: "cancelling"; phase: Phase; progress: ProgressView; taskId: string }
  | { kind: "playing"; launchedAt: number }
  | { kind: "crashed"; exitCode: number }
  | { kind: "failed"; error: ErrorPayload | null };

export type PlayInputs = {
  instance: Instance;
  task: TaskSnapshot | undefined;
  running: RunningSession | undefined;
  /** 起動中の一覧を受け取り済みか。未受信の間は、遊んでいるかどうか分からない。 */
  runningLoaded: boolean;
  lastExit: LastExit | undefined;
};

export type Announcement =
  | { key: "play.announce.started" }
  | { key: "play.announce.progress"; percent: number }
  | { key: "play.announce.phase"; phase: TaskPhase }
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

/** 異常終了を UI のエラーとして表す。文言は errors:game.crashed。 */
export function crashPayload(exitCode: number): ErrorPayload {
  return { code: "game.crashed", params: { exitCode }, detail: null };
}

export function playStateFrom({ instance, task, running, runningLoaded, lastExit }: PlayInputs): PlayState {
  if (running) return { kind: "playing", launchedAt: running.launchedAt };
  if (task) {
    const phase: Phase = task.kind === INSTALL_KIND ? "install" : "launch";
    if (task.state === "queued" || task.state === "running") {
      return { kind: "preparing", phase, progress: progressView(task), taskId: task.id };
    }
    if (task.state === "cancelling") {
      return { kind: "cancelling", phase, progress: progressView(task), taskId: task.id };
    }
  }
  if (!runningLoaded) return { kind: "checking" };
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
    // 段階が変わったら、その名前だけを 1 回知らせる（進み具合は段階ごとに数え直すため、前の段階とは比べない）
    if (next.progress.phase !== prev.progress.phase) {
      return next.progress.phase === null ? null : { key: "play.announce.phase", phase: next.progress.phase };
    }
    const before = Math.floor((prev.progress.percent ?? 0) / 25);
    const after = Math.floor((next.progress.percent ?? 0) / 25);
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
