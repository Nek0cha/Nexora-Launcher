import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { onGameExited, onSyncApplyFinished } from "../../api/events";
import { toErrorPayload } from "../../api/ipc";
import type { Instance } from "../../bindings/Instance";
import { pushToast } from "../../components/toast/useToast";
import i18n from "../../i18n";
import { INSTANCES_KEY } from "../instances/useInstances";
import { SYNC_KEY, instanceSyncKey } from "./useInstanceSync";
import { reasonKey } from "./syncText";

/**
 * アプリで 1 回だけ呼ぶ。起動前の共通設定の適用が終わったら、そのインスタンスの状態を取り直す。
 * 失敗したときだけ、理由を付けてトーストで知らせる（起動は止めない）。
 * ゲームが終わったら、終了時に集めた提案を出すために、共通設定の状態をまとめて取り直す。
 */
export function useSyncEvents(): void {
  const client = useQueryClient();

  useEffect(() => {
    let disposed = false;
    const stops: Array<() => void> = [];
    const keep = (unlisten: () => void) => {
      if (disposed) unlisten();
      else stops.push(unlisten);
    };
    const warn = (error: unknown) => console.warn(toErrorPayload(error).code);
    onGameExited(() => {
      // 提案は終了の通知より前に記録される。件数はほかのメンバーのカードにも出るので全部を取り直す
      void client.invalidateQueries({ queryKey: SYNC_KEY });
    }).then(keep, warn);
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
    }).then(keep, warn);
    return () => {
      disposed = true;
      for (const stop of stops) stop();
    };
  }, [client]);
}
