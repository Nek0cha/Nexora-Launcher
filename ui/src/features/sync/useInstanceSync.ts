import { useIsMutating, useQuery } from "@tanstack/react-query";
import { getInstanceSync, listSyncFields, listSyncProfiles } from "../../api/commands";
import type { InstanceSync } from "../../bindings/InstanceSync";

/** 共通設定に関わるキャッシュとミューテーションの共通の先頭。変更の後はこれでまとめて取り直す */
export const SYNC_KEY = ["sync"] as const;
export const SYNC_PROFILES_KEY = ["sync", "profiles"] as const;
export const SYNC_FIELDS_KEY = ["sync", "fields"] as const;
export const instanceSyncKey = (instanceId: string) => ["sync", "instance", instanceId] as const;

/** インスタンスの共通設定の状態。busy は、取り直しか共通設定の変更が進んでいる間 true。 */
export function useInstanceSync(instanceId: string): {
  data: InstanceSync | undefined;
  error: Error | null;
  refresh: () => void;
  busy: boolean;
} {
  const query = useQuery({ queryKey: instanceSyncKey(instanceId), queryFn: () => getInstanceSync(instanceId) });
  const mutating = useIsMutating({ mutationKey: SYNC_KEY });
  return {
    data: query.data,
    error: query.error,
    refresh: () => {
      void query.refetch();
    },
    busy: query.isFetching || mutating > 0,
  };
}

export function useSyncProfiles() {
  return useQuery({ queryKey: SYNC_PROFILES_KEY, queryFn: listSyncProfiles });
}

/** 登録表の項目。アプリの実行中は変わらない */
export function useSyncFields() {
  return useQuery({ queryKey: SYNC_FIELDS_KEY, queryFn: listSyncFields, staleTime: Number.POSITIVE_INFINITY });
}
