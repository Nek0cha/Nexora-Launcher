import { useIsMutating, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { startDemoTask } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import { DEMO_KIND } from "./playState";
import { isTerminal } from "../tasks/taskTable";
import { TASKS_KEY } from "../tasks/useTasks";
import type { TaskTable } from "../tasks/taskTable";

const START_KEY = ["startDemo"] as const;

/**
 * ホームとコマンドパレットで共有する開始処理。応答までの間は starting が真になる。
 * 開始中、または実行中のデモタスクがあるときは、どの呼び出し元からも新しく開始しない。
 * 開始の呼び出し自体が失敗したときは error に入り、次に開始に成功するまで残る。
 */
export function useDemoStart(): { start: () => void; starting: boolean; error: ErrorPayload | null } {
  const queryClient = useQueryClient();
  const [error, setError] = useState<ErrorPayload | null>(null);
  const mutation = useMutation({
    mutationKey: START_KEY,
    mutationFn: () => startDemoTask(),
    onSuccess: () => setError(null),
    onError: (e) => setError(toErrorPayload(e)),
  });
  // 利用者が開始を押した別の場所の分も含めて数える（フックごとの isPending では共有できない）
  const starting = useIsMutating({ mutationKey: START_KEY }) > 0;
  return {
    start: () => {
      // 描画を待たずに、その時点の状態で判断する
      if (queryClient.isMutating({ mutationKey: START_KEY }) > 0) return;
      const table = queryClient.getQueryData<TaskTable>(TASKS_KEY) ?? {};
      if (Object.values(table).some((task) => task.kind === DEMO_KIND && !isTerminal(task))) return;
      mutation.mutate();
    },
    starting,
    error,
  };
}
