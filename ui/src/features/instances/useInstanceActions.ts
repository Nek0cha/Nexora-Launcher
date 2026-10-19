import { useMutation, useQueryClient } from "@tanstack/react-query";
import { deleteInstance, duplicateInstance, updateInstance } from "../../api/commands";
import type { InstanceSettings } from "../../bindings/InstanceSettings";
import { INSTANCES_KEY } from "./useInstances";

/** 設定を保存する。成功したら一覧を取り直す。 */
export function useUpdateInstance() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ instanceId, settings }: { instanceId: string; settings: InstanceSettings }) =>
      updateInstance(instanceId, settings),
    onSuccess: () => client.invalidateQueries({ queryKey: INSTANCES_KEY }),
  });
}

/** インスタンスを削除する。成功したら一覧を取り直す。 */
export function useDeleteInstance() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (instanceId: string) => deleteInstance(instanceId),
    onSuccess: () => client.invalidateQueries({ queryKey: INSTANCES_KEY }),
  });
}

/** インスタンスを複製する。タスク ID を返す。成功したら一覧を取り直す。 */
export function useDuplicateInstance() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (instanceId: string) => duplicateInstance(instanceId),
    onSuccess: () => client.invalidateQueries({ queryKey: INSTANCES_KEY }),
  });
}
