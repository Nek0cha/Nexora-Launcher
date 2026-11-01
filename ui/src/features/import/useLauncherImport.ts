import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useSyncExternalStore } from "react";
import i18n from "../../i18n";
import { importFromLauncher } from "../../api/commands";
import type { ImportRequest } from "../../bindings/ImportRequest";
import type { ImportSourceKind } from "../../bindings/ImportSourceKind";
import { useToast } from "../../components/toast/useToast";
import { INSTANCES_KEY } from "../instances/useInstances";
import { isTerminal } from "../tasks/taskTable";
import { useTasks } from "../tasks/useTasks";
import { batchOutcome } from "./batchOutcome";

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

// 始めたタスク（取り込み 1 回につき 1 つの束）。全部終わったら 1 回だけ結果を知らせるために覚えておく
const batches: string[][] = [];
const watched = new Set<string>();

/** 選んだインスタンスを作り、中身をコピーするタスクを始める（インスタンスごとに 1 つ）。 */
export function useImportFromLauncher() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({
      source,
      requests,
      allowDuplicates,
      root,
    }: {
      source: ImportSourceKind;
      requests: ImportRequest[];
      allowDuplicates: boolean;
      root: string | null;
    }) => {
      const created = await importFromLauncher(source, root, requests, allowDuplicates);
      const ids = created.map((item) => item.taskId);
      for (const id of ids) watched.add(id);
      batches.push(ids);
      return created;
    },
    onSuccess: () => client.invalidateQueries({ queryKey: INSTANCES_KEY }),
  });
}

/**
 * 取り込みのタスクが終わったら一覧を取り直し、1 回の取り込みのタスクがすべて終わった時点で結果を 1 回だけ知らせる。
 * 1 件だけで失敗したときは、その理由をそのまま出す。失敗した取り込みはインスタンスごと消えるため、
 * 知らせないと何も起きなかったように見える。アプリで 1 回だけ呼ぶ。
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
    }
    for (let i = batches.length - 1; i >= 0; i--) {
      const outcome = batchOutcome(batches[i] ?? [], tasks);
      if (!outcome.done) continue;
      batches.splice(i, 1);
      if (outcome.failed === 0) {
        toast.show({ kind: "success", message: i18n.t("launcherImport.done", { count: outcome.succeeded }) });
      } else if (outcome.total === 1 && outcome.firstError) {
        toast.showError(outcome.firstError);
      } else {
        toast.show({
          kind: "error",
          message: i18n.t("launcherImport.summary", { succeeded: outcome.succeeded, failed: outcome.failed }),
        });
      }
    }
  }, [tasks, client, toast]);
}
