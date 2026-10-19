import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { isTerminal, sortedTasks, type TaskTable } from "../tasks/taskTable";
import { useTasks } from "../tasks/useTasks";
import { INSTANCES_KEY } from "./useInstances";

export const DUPLICATE_KIND = "duplicate";

/** このインスタンスを複製元として動いている複製のタスク。なければ undefined。 */
export function activeDuplicate(table: TaskTable, instanceId: string): TaskSnapshot | undefined {
  return sortedTasks(table).find(
    (t) => t.kind === DUPLICATE_KIND && t.instanceId === instanceId && !isTerminal(t),
  );
}

export function duplicatePercent(task: TaskSnapshot): number {
  const { current, total } = task.progress;
  if (total <= 0) return 0;
  return Math.min(100, Math.floor((current * 100) / total));
}

/** 複製のタスクの状態だけを並べた文字列。進捗の変化では変わらない。 */
export function duplicateSignature(table: TaskTable): string {
  return sortedTasks(table)
    .filter((t) => t.kind === DUPLICATE_KIND)
    .map((t) => `${t.id}:${t.state}`)
    .join(",");
}

/**
 * 複製のタスクの状態が変わったら、インスタンスの一覧を取り直す。
 * 新しい行はタスクの中で（ロックを取った後に）作られるため、複製のコマンドが返った直後の取り直しでは間に合わない。
 * 始まったときと終わったとき（成功、失敗、キャンセル）に取り直し、作られた行と、失敗で消えた行を一覧に反映する。
 */
export function useDuplicateRefresh(): void {
  const client = useQueryClient();
  const signature = duplicateSignature(useTasks());
  // 最初の描画の時点の状態は、一覧の最初の取得に含まれるため、取り直さない
  const previous = useRef(signature);
  useEffect(() => {
    if (previous.current === signature) return;
    previous.current = signature;
    void client.invalidateQueries({ queryKey: INSTANCES_KEY });
  }, [signature, client]);
}
