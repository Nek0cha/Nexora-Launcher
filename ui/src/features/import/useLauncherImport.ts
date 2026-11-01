import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useSyncExternalStore } from "react";
import { importFromLauncher } from "../../api/commands";
import type { ImportRequest } from "../../bindings/ImportRequest";
import { useToast } from "../../components/toast/useToast";
import { INSTANCES_KEY } from "../instances/useInstances";
import { isTerminal } from "../tasks/taskTable";
import { useTasks } from "../tasks/useTasks";

export const IMPORT_TASK_KIND = "launcher-import";

// 読み込みのダイアログが開いているか。ボタンとダイアログが離れているため、React の外に置く
let opened = false;
const listeners = new Set<() => void>();

function emit() {
  for (const fn of listeners) fn();
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function openLauncherImport(): void {
  opened = true;
  emit();
}

export function closeLauncherImport(): void {
  if (!opened) return;
  opened = false;
  emit();
}

export function useLauncherImportOpen(): boolean {
  return useSyncExternalStore(subscribe, () => opened);
}

// 始めたタスク。終わりを見届けて、失敗を知らせるために覚えておく
const watched = new Set<string>();

/** 選んだインスタンスを作り、中身をコピーするタスクを始める（インスタンスごとに 1 つ）。 */
export function useImportFromLauncher() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({ requests, allowDuplicates, root }: { requests: ImportRequest[]; allowDuplicates: boolean; root: string | null }) => {
      const created = await importFromLauncher("modrinth_app", root, requests, allowDuplicates);
      for (const item of created) watched.add(item.taskId);
      return created;
    },
    onSuccess: () => client.invalidateQueries({ queryKey: INSTANCES_KEY }),
  });
}

/**
 * 取り込みのタスクが終わったら、一覧を取り直し、失敗なら理由を知らせる。
 * 失敗した取り込みはインスタンスごと消えるため、知らせないと何も起きなかったように見える。
 * アプリで 1 回だけ呼ぶ。
 */
export function useLauncherImportTaskWatch(): void {
  const tasks = useTasks();
  const client = useQueryClient();
  const toast = useToast();
  useEffect(() => {
    for (const task of Object.values(tasks)) {
      if (task.kind !== IMPORT_TASK_KIND) continue;
      if (!isTerminal(task)) {
        watched.add(task.id);
        continue;
      }
      if (!watched.delete(task.id)) continue;
      void client.invalidateQueries({ queryKey: INSTANCES_KEY });
      if (task.state === "failed" && task.error) toast.showError(task.error);
    }
  }, [tasks, client, toast]);
}
