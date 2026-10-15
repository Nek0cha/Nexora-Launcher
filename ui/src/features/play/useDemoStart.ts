import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { startDemoTask } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { ErrorPayload } from "../../bindings/ErrorPayload";

/**
 * ホームとコマンドパレットで共有する開始処理。応答までの間は starting が真になる。
 * 開始の呼び出し自体が失敗したときは error に入り、次に開始に成功するまで残る。
 */
export function useDemoStart(): { start: () => void; starting: boolean; error: ErrorPayload | null } {
  const [error, setError] = useState<ErrorPayload | null>(null);
  const mutation = useMutation({
    mutationKey: ["startDemo"],
    mutationFn: () => startDemoTask(),
    onSuccess: () => setError(null),
    onError: (e) => setError(toErrorPayload(e)),
  });
  return {
    start: () => {
      if (!mutation.isPending) mutation.mutate();
    },
    starting: mutation.isPending,
    error,
  };
}
