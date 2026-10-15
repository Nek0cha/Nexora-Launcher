import { useMutation } from "@tanstack/react-query";
import { startDemoTask } from "../../api/commands";

/** ホームとコマンドパレットで共有する開始処理。応答までの間は starting が真になる。 */
export function useDemoStart() {
  const mutation = useMutation({ mutationKey: ["startDemo"], mutationFn: () => startDemoTask() });
  return {
    start: () => {
      if (!mutation.isPending) mutation.mutate();
    },
    starting: mutation.isPending,
  };
}
