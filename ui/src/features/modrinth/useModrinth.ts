import { type QueryClient, useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import {
  changeContentVersion,
  checkContentUpdates,
  contentTaskReport,
  getTask,
  installContent,
  listContentVersions,
  previewContentInstall,
  searchModrinth,
  updateContent,
} from "../../api/commands";
import type { ContentTaskReport } from "../../bindings/ContentTaskReport";
import type { ItemFailure } from "../../bindings/ItemFailure";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import type { SearchQuery } from "../../bindings/SearchQuery";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { updatesKey } from "../content/useContent";
import { isTerminal, type TaskTable } from "../tasks/taskTable";
import { TASKS_KEY } from "../tasks/useTasks";

/** 検索の 1 ページの件数（backend の既定と同じ） */
export const SEARCH_PAGE_SIZE = 20;

const searchKey = (instanceId: string | null, query: SearchQuery) => ["modrinth-search", instanceId, query] as const;

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

/** タスクを直接読み直す間隔（ミリ秒） */
export const WAIT_POLL_MS = 2000;
/** 続けてこの回数だけ読めなければ、待つのをやめてその失敗で終える */
export const WAIT_MAX_FAILURES = 3;

/**
 * タスクが終わるのを待つ。タスクの表（useTaskSync が更新する）を見つつ、表の通知を取りこぼしたときや
 * 表に無い（すでに終わって消えた）ときに備えて、一定の間隔で直接も読む。
 * 続けて読めなければ（タスクが見つからないなど）、いつまでも待たずにその失敗で終える。
 */
export function waitForTask(client: QueryClient, taskId: string): Promise<TaskSnapshot> {
  return new Promise((resolve, reject) => {
    let done = false;
    let failures = 0;
    const cache = client.getQueryCache();
    const finish = (fn: () => void) => {
      if (done) return;
      done = true;
      unsubscribe();
      clearInterval(timer);
      fn();
    };
    const check = () => {
      const task = client.getQueryData<TaskTable>(TASKS_KEY)?.[taskId];
      if (task && isTerminal(task)) finish(() => resolve(task));
    };
    const poll = () => {
      getTask(taskId).then(
        (task) => {
          failures = 0;
          if (isTerminal(task)) finish(() => resolve(task));
        },
        (error: unknown) => {
          failures += 1;
          if (failures >= WAIT_MAX_FAILURES) finish(() => reject(error));
        },
      );
    };
    const unsubscribe = cache.subscribe(check);
    const timer = setInterval(poll, WAIT_POLL_MS);
    check();
    if (done) return;
    poll();
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

const versionsKey = (instanceId: string) => ["content-versions", instanceId] as const;

/**
 * MOD の行の、切り替えられる版の一覧。版の一覧を開いたとき（`enabled`）だけ読む。
 * 切り替えたあとは useChangeContentVersion が読み直させる。
 */
export function useContentVersions(instanceId: string, relPath: string, enabled: boolean) {
  return useQuery({
    queryKey: [...versionsKey(instanceId), relPath],
    queryFn: () => listContentVersions(instanceId, relPath),
    staleTime: 5 * 60_000,
    retry: false,
    enabled,
  });
}

/**
 * MOD を選んだ版に入れ替え、終わるまで待ってレポートを返す。
 * 完了後は一覧、更新の確認、版の一覧を取り直す（入れ替えでファイル名と固定が変わる）。
 */
export function useChangeContentVersion(instanceId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({ relPath, versionId }: { relPath: string; versionId: string }) => {
      const taskId = await changeContentVersion(instanceId, relPath, versionId);
      return runContentTask(client, taskId);
    },
    onSettled: () => {
      void client.invalidateQueries({ queryKey: ["content", instanceId] });
      void client.invalidateQueries({ queryKey: updatesKey(instanceId) });
      void client.invalidateQueries({ queryKey: versionsKey(instanceId) });
    },
  });
}

const failuresKey = (instanceId: string) => ["content-update-failures", instanceId] as const;

/**
 * 直前の「すべて更新」で失敗した MOD。画面の状態に持ち（すぐに描き直す）、同じものを問い合わせの
 * キャッシュにも置いて（通信はしない）、タブを閉じて開き直しても残す。
 */
export function useUpdateFailures(instanceId: string) {
  const client = useQueryClient();
  const [failures, setLocal] = useState<ItemFailure[] | null>(
    () => client.getQueryData<ItemFailure[] | null>(failuresKey(instanceId)) ?? null,
  );
  const set = useCallback(
    (next: ItemFailure[] | null) => {
      setLocal(next);
      // 見ている部品が無くても消えないようにする
      client.setQueryDefaults(failuresKey(instanceId), { gcTime: Infinity });
      client.setQueryData(failuresKey(instanceId), next);
    },
    [client, instanceId],
  );
  return [failures, set] as const;
}
