import { type QueryClient, skipToken, useQuery } from "@tanstack/react-query";
import { listInstances } from "../../api/commands";
import type { Instance } from "../../bindings/Instance";

export const INSTANCES_KEY = ["instances"] as const;
/** ホームに出すインスタンスの選択（ID）。アプリを再起動すると消える。 */
export const HOME_SELECTION_KEY = ["homeSelection"] as const;

export function useInstances() {
  return useQuery({ queryKey: INSTANCES_KEY, queryFn: listInstances });
}

function useHomeSelection(): string | null {
  const { data } = useQuery<string | null>({
    queryKey: HOME_SELECTION_KEY,
    queryFn: skipToken,
    enabled: false,
    initialData: null,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
  });
  return data ?? null;
}

/** ホームに出すインスタンスを選ぶ。作成した直後のインスタンスを出すために使う。 */
export function setHomeSelection(client: QueryClient, instanceId: string): void {
  client.setQueryData<string | null>(HOME_SELECTION_KEY, instanceId);
}

/**
 * 一覧から、ホームに出すインスタンスを決める。選択があり一覧に残っていればそれを、
 * なければ先頭（backend が最後に遊んだ順、なければ作成の新しい順に並べている）を返す。
 */
export function pickHomeInstance(list: Instance[], selection: string | null): Instance | undefined {
  return list.find((i) => i.id === selection) ?? list[0];
}

/** ホームとコマンドパレットが共有する、ホームに出すインスタンス。 */
export function useHomeInstance(): Instance | undefined {
  const list = useInstances().data;
  const selection = useHomeSelection();
  return list ? pickHomeInstance(list, selection) : undefined;
}
