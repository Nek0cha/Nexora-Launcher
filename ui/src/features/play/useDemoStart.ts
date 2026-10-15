import { useIsMutating, useMutation, useMutationState, useQueryClient } from "@tanstack/react-query";
import { startDemoTask } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import { DEMO_KIND } from "./playState";
import { isTerminal } from "../tasks/taskTable";
import type { TaskTable } from "../tasks/taskTable";
import { TASKS_KEY } from "../tasks/useTasks";

const START_KEY = ["startDemo"] as const;

/**
 * ホームとコマンドパレットで共有する開始処理。応答までの間は starting が真になる。
 * 開始中、または実行中のデモタスクがあるときは、どの呼び出し元からも新しく開始しない。
 * これは多重開始に対する二重の備えであり、バックエンドの spawn_unique が実行中のタスクの id を返すため、
 * フロント側の判定が抜けても 2 件目は作られない。
 * 開始の呼び出し自体が失敗したときは、どの呼び出し元の error にも入る（mutation の履歴から求める）。
 * 次に開始に成功すると消える。
 */
export function useDemoStart(): { start: () => void; starting: boolean; error: ErrorPayload | null } {
  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationKey: START_KEY,
    mutationFn: () => startDemoTask(),
  });
  // 利用者が開始を押した別の場所の分も含めて数える（フックごとの isPending では共有できない）
  const starting = useIsMutating({ mutationKey: START_KEY }) > 0;
  // 最後に終わった開始の結果。失敗ならそのエラー、成功なら null。呼び出し元のフックをまたいで共有される。
  const settled = useMutationState({
    filters: { mutationKey: START_KEY },
    select: (m) => ({ status: m.state.status, error: m.state.error }),
  }).filter((m) => m.status === "error" || m.status === "success");
  const last = settled[settled.length - 1];
  const error = last?.status === "error" ? toErrorPayload(last.error) : null;
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
