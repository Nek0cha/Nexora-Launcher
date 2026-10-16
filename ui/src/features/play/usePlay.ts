import { useIsMutating, useMutation, useMutationState, useQueryClient } from "@tanstack/react-query";
import { launchInstance } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import { isTerminal, type TaskTable } from "../tasks/taskTable";
import { TASKS_KEY } from "../tasks/useTasks";
import { clearLastExit } from "./useGameSync";
import { INSTALL_KIND, LAUNCH_KIND } from "./playState";

/**
 * ホームとコマンドパレットで共有する「プレイ」の処理。インスタンスごとに開始中かどうかを共有する。
 * 実行中のインストールか起動のタスクがあれば、新しく呼ばない（backend も同じタスクの ID を返す）。
 */
export function usePlay(instanceId: string): { press: () => void; starting: boolean; error: ErrorPayload | null } {
  const client = useQueryClient();
  const key = ["launchInstance", instanceId] as const;
  const mutation = useMutation({ mutationKey: key, mutationFn: () => launchInstance(instanceId) });
  const starting = useIsMutating({ mutationKey: key }) > 0;
  const settled = useMutationState({
    filters: { mutationKey: key },
    select: (m) => ({ status: m.state.status, error: m.state.error }),
  }).filter((m) => m.status === "error" || m.status === "success");
  const last = settled[settled.length - 1];
  const error = last?.status === "error" ? toErrorPayload(last.error) : null;
  return {
    press: () => {
      if (client.isMutating({ mutationKey: key }) > 0) return;
      const table = client.getQueryData<TaskTable>(TASKS_KEY) ?? {};
      const busy = Object.values(table).some(
        (t) => t.instanceId === instanceId && (t.kind === INSTALL_KIND || t.kind === LAUNCH_KIND) && !isTerminal(t),
      );
      if (busy) return;
      clearLastExit(client, instanceId);
      mutation.mutate();
    },
    starting,
    error,
  };
}
