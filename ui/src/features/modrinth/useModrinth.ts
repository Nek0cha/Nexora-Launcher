import { type QueryClient, useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  checkContentUpdates,
  contentTaskReport,
  getTask,
  installContent,
  previewContentInstall,
  searchModrinth,
  updateContent,
} from "../../api/commands";
import type { ContentTaskReport } from "../../bindings/ContentTaskReport";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import type { SearchQuery } from "../../bindings/SearchQuery";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { isTerminal, type TaskTable } from "../tasks/taskTable";
import { TASKS_KEY } from "../tasks/useTasks";

/** 検索の 1 ページの件数（backend の既定と同じ） */
export const SEARCH_PAGE_SIZE = 20;

const searchKey = (instanceId: string | null, query: SearchQuery) => ["modrinth-search", instanceId, query] as const;
const updatesKey = (instanceId: string) => ["content-updates", instanceId] as const;

/** 検索。条件が変わると別のキャッシュになり、古い結果は新しい入力に混ざらない（placeholderData は使わない）。 */
export function useModrinthSearch(instanceId: string | null, query: SearchQuery) {
  return useInfiniteQuery({
    queryKey: searchKey(instanceId, query),
    queryFn: ({ pageParam }) => searchModrinth(instanceId, query, pageParam),
    initialPageParam: 0,
    getNextPageParam: (last) => {
      const next = last.offset + SEARCH_PAGE_SIZE;
      return next < last.total && last.hits.length > 0 ? next : undefined;
    },
  });
}

/** タスクが終わるのを待つ。タスクの表（useTaskSync が更新する）を見て、表に無ければ一度だけ取得する。 */
export function waitForTask(client: QueryClient, taskId: string): Promise<TaskSnapshot> {
  return new Promise((resolve, reject) => {
    let done = false;
    const cache = client.getQueryCache();
    const finish = (fn: () => void) => {
      if (done) return;
      done = true;
      unsubscribe();
      fn();
    };
    const check = () => {
      const task = client.getQueryData<TaskTable>(TASKS_KEY)?.[taskId];
      if (task && isTerminal(task)) finish(() => resolve(task));
    };
    const unsubscribe = cache.subscribe(check);
    check();
    if (done) return;
    // 表に無い（すでに終わって消えた）場合に備え、一度だけ直接取得する
    getTask(taskId).then(
      (task) => {
        if (isTerminal(task)) finish(() => resolve(task));
      },
      () => {},
    );
  });
}

export type ContentTaskOutcome = {
  taskId: string;
  state: TaskSnapshot["state"];
  /** 失敗したときのタスクのエラー */
  error: ErrorPayload | null;
  /** 覚えていなければ null */
  report: ContentTaskReport | null;
};

async function runContentTask(client: QueryClient, taskId: string): Promise<ContentTaskOutcome> {
  const task = await waitForTask(client, taskId);
  const report = await contentTaskReport(taskId).catch(() => null);
  return { taskId, state: task.state, error: task.error, report };
}

/** 導入の前に見せる内容を調べる。 */
export function usePreviewInstall() {
  return useMutation({
    mutationFn: ({ instanceId, projectId }: { instanceId: string; projectId: string }) =>
      previewContentInstall(instanceId, projectId),
  });
}

/**
 * 導入。タスクを始め、終わるまで待ち、レポートを返す（プレビューと確認は呼び出し側が先に行う）。
 * 完了後は導入済みの一覧と更新の確認を取り直す。
 */
export function useInstallContent() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (v: { instanceId: string; projectId: string; acceptIncompatible: boolean }) => {
      const taskId = await installContent(v.instanceId, v.projectId, v.acceptIncompatible);
      return runContentTask(client, taskId);
    },
    onSettled: (_data, _error, v) => {
      void client.invalidateQueries({ queryKey: ["content", v.instanceId] });
      void client.invalidateQueries({ queryKey: updatesKey(v.instanceId) });
    },
  });
}

/**
 * 更新の確認（キャッシュを使う）。`enabled` が偽の間は確認しない。
 * オフラインや 429 で失敗しても繰り返さない（画面は「あとで確認します」にとどめる）。
 */
export function useContentUpdates(instanceId: string, enabled = true) {
  return useQuery({
    queryKey: updatesKey(instanceId),
    queryFn: () => checkContentUpdates(instanceId, false),
    staleTime: 5 * 60_000,
    retry: false,
    enabled,
  });
}

/** 更新をキャッシュを使わずに確認し直す。結果は useContentUpdates に反映する。 */
export function useRecheckUpdates(instanceId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => checkContentUpdates(instanceId, true),
    onSuccess: (data) => client.setQueryData(updatesKey(instanceId), data),
  });
}

/** 選んだ MOD を更新し、終わるまで待ってレポートを返す。 */
export function useUpdateContent(instanceId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (relPaths: string[]) => {
      const taskId = await updateContent(instanceId, relPaths);
      return runContentTask(client, taskId);
    },
    onSettled: () => {
      void client.invalidateQueries({ queryKey: ["content", instanceId] });
      void client.invalidateQueries({ queryKey: updatesKey(instanceId) });
    },
  });
}
