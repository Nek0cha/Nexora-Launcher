import { type QueryClient, skipToken, useQuery, useQueryClient } from "@tanstack/react-query";

export const CRASH_KEY = ["crashQueue"] as const;

/** 診断を待っている異常終了。`launchedAt` は終わった起動の時刻（ゲームのログの名前と同じ） */
export type CrashEntry = { instanceId: string; launchedAt: number };

/** backend から取り直せない状態なので、見ている部品がなくても消さず、取り直しもしない */
const OPTIONS = {
  gcTime: Number.POSITIVE_INFINITY,
  staleTime: Number.POSITIVE_INFINITY,
} as const;

/** 異常終了をキューに積む。同じ起動は 2 度積まない。 */
export function pushCrash(client: QueryClient, entry: CrashEntry): void {
  client.setQueryDefaults(CRASH_KEY, OPTIONS);
  client.setQueryData<CrashEntry[]>(CRASH_KEY, (prev = []) =>
    prev.some((e) => e.instanceId === entry.instanceId && e.launchedAt === entry.launchedAt) ? prev : [...prev, entry],
  );
}

/** 先頭がダイアログに出ているもの。閉じると次に進む。 */
export function useCrashQueue(): { head: CrashEntry | null; dismiss: () => void } {
  const client = useQueryClient();
  const { data } = useQuery<CrashEntry[]>({
    queryKey: CRASH_KEY,
    queryFn: skipToken,
    enabled: false,
    initialData: [],
    ...OPTIONS,
  });
  return {
    head: data?.[0] ?? null,
    dismiss: () => client.setQueryData<CrashEntry[]>(CRASH_KEY, (prev = []) => prev.slice(1)),
  };
}
