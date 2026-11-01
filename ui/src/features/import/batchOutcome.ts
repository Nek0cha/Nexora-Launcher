import type { ErrorPayload } from "../../bindings/ErrorPayload";
import { isTerminal, type TaskTable } from "../tasks/taskTable";

export type BatchOutcome = {
  /** バッチのタスクがすべて終わったか。一覧に無いタスクがあれば、まだ終わっていない。 */
  done: boolean;
  total: number;
  succeeded: number;
  failed: number;
  /** 最初に失敗したタスクの理由。 */
  firstError: ErrorPayload | null;
};

/** 1 回の取り込みで始めたタスクの結果。成功以外（失敗と取り消し）は失敗に数える。 */
export function batchOutcome(taskIds: readonly string[], tasks: TaskTable): BatchOutcome {
  let succeeded = 0;
  let failed = 0;
  let firstError: ErrorPayload | null = null;
  let done = true;
  for (const id of taskIds) {
    const task = tasks[id];
    if (!task || !isTerminal(task)) {
      done = false;
      continue;
    }
    if (task.state === "succeeded") succeeded += 1;
    else {
      failed += 1;
      firstError ??= task.error;
    }
  }
  return { done, total: taskIds.length, succeeded, failed, firstError };
}
