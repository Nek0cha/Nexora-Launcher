import { skipToken, useIsMutating, useMutation, useMutationState, useQuery, useQueryClient } from "@tanstack/react-query";
import { launchInstance } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import { loginPrompt, type LoginPrompt } from "../login/loginState";
import { isTerminal, type TaskTable } from "../tasks/taskTable";
import { TASKS_KEY, useTasks } from "../tasks/useTasks";
import { clearLastExit } from "./useGameSync";
import { INSTALL_KIND, LAUNCH_KIND, MRPACK_KIND } from "./playState";

export type Play = {
  press: () => void;
  starting: boolean;
  /** 開始の失敗。ログインのダイアログで扱う失敗（prompt の元）は含めない。 */
  error: ErrorPayload | null;
  /** 開始の失敗ごとに変わる印。同じ失敗を二度知らせないために使う。error が null なら null。 */
  errorId: string | null;
  /** ログインのダイアログを出す理由。閉じたら null。 */
  prompt: LoginPrompt | null;
  dismissPrompt: () => void;
  /** ダイアログを閉じ、オフラインで起動する。 */
  playOffline: () => void;
  /** ダイアログを閉じ、もう一度起動する。 */
  retry: () => void;
};

const dismissedKey = (instanceId: string) => ["loginPromptDismissed", instanceId] as const;

/**
 * ホームとコマンドパレットで共有する「プレイ」の処理。インスタンスごとに開始中かどうかを共有する。
 * 実行中のインストールか起動のタスクがあれば、新しく呼ばない（backend も同じタスクの ID を返す）。
 */
export function usePlay(instanceId: string): Play {
  const client = useQueryClient();
  const key = ["launchInstance", instanceId] as const;
  const mutation = useMutation({
    mutationKey: key,
    mutationFn: (offline: boolean) => launchInstance(instanceId, offline),
  });
  const starting = useIsMutating({ mutationKey: key }) > 0;
  const settled = useMutationState({
    filters: { mutationKey: key },
    select: (m) => ({ status: m.state.status, error: m.state.error, data: m.state.data, at: m.state.submittedAt }),
  }).filter((m) => m.status === "error" || m.status === "success");
  const last = settled[settled.length - 1];
  const tasks = useTasks();
  // 閉じたダイアログの元。同じ失敗では開き直さない（呼び出し元の間で共有する）
  const { data: dismissed } = useQuery<string | null>({
    queryKey: dismissedKey(instanceId),
    queryFn: skipToken,
    enabled: false,
    initialData: null,
    staleTime: Number.POSITIVE_INFINITY,
    // 起動の記録（mutation）より先に消えると、閉じたダイアログが開き直すため、消さない
    gcTime: Number.POSITIVE_INFINITY,
  });

  const callError = last?.status === "error" ? toErrorPayload(last.error) : null;
  // 起動のタスクは、セッションの用意で auth.* として失敗する。この押下で始めたタスクの失敗だけを見る
  const startedTask = last?.status === "success" && typeof last.data === "string" ? tasks[last.data] : undefined;
  const source: { id: string; error: ErrorPayload | null } | null = callError
    ? { id: `call:${last?.at}`, error: callError }
    : startedTask?.state === "failed"
      ? { id: `task:${startedTask.id}`, error: startedTask.error }
      : null;
  const sourcePrompt = loginPrompt(source?.error ?? null);
  const prompt = source && sourcePrompt && source.id !== dismissed ? sourcePrompt : null;
  const error = sourcePrompt ? null : callError;

  const dismissPrompt = () => {
    if (source) client.setQueryData(dismissedKey(instanceId), source.id);
  };

  const launch = (offline: boolean) => {
    if (client.isMutating({ mutationKey: key }) > 0) return;
    const table = client.getQueryData<TaskTable>(TASKS_KEY) ?? {};
    const busy = Object.values(table).some(
      (t) => t.instanceId === instanceId && (t.kind === INSTALL_KIND || t.kind === LAUNCH_KIND || t.kind === MRPACK_KIND) && !isTerminal(t),
    );
    if (busy) return;
    clearLastExit(client, instanceId);
    mutation.mutate(offline);
  };

  return {
    press: () => launch(false),
    starting,
    error,
    errorId: error ? `call:${last?.at}` : null,
    prompt,
    dismissPrompt,
    playOffline: () => {
      dismissPrompt();
      launch(true);
    },
    retry: () => {
      dismissPrompt();
      launch(false);
    },
  };
}
