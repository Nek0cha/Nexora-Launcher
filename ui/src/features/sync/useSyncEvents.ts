import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { onSyncApplyFinished } from "../../api/events";
import { toErrorPayload } from "../../api/ipc";
import type { Instance } from "../../bindings/Instance";
import { pushToast } from "../../components/toast/useToast";
import i18n from "../../i18n";
import { INSTANCES_KEY } from "../instances/useInstances";
import { instanceSyncKey } from "./useInstanceSync";
import { reasonKey } from "./syncText";

/**
 * アプリで 1 回だけ呼ぶ。起動前の共通設定の適用が終わったら、そのインスタンスの状態を取り直す。
 * 失敗したときだけ、理由を付けてトーストで知らせる（起動は止めない）。
 */
export function useSyncEvents(): void {
  const client = useQueryClient();

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    onSyncApplyFinished((event) => {
      void client.invalidateQueries({ queryKey: instanceSyncKey(event.instanceId) });
      if (event.outcome !== "failed") return;
      const name = client.getQueryData<Instance[]>(INSTANCES_KEY)?.find((i) => i.id === event.instanceId)?.name;
      const reason = i18n.t(reasonKey(event.reason));
      pushToast({
        kind: "error",
        message:
          name === undefined
            ? i18n.t("sync.toast.failedNoName", { reason })
            : i18n.t("sync.toast.failed", { name, reason }),
      });
    }).then(
      (unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      },
      (error: unknown) => console.warn(toErrorPayload(error).code),
    );
    return () => {
      disposed = true;
      stop?.();
    };
  }, [client]);
}
