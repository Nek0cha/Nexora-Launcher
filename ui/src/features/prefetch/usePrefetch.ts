import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { getStartupStatus } from "../../api/commands";
import { loaderGameVersionsQuery, versionsQuery } from "../instances/useCreateInstance";

/** 同時に取りに行く数。起動直後の通信を詰まらせないための上限 */
const CONCURRENCY = 2;

/** インスタンス作成ダイアログが最初に必要とする一覧。上から順に取る */
const jobs = (): { key: readonly unknown[]; run: (client: QueryClient) => Promise<void> }[] => [
  { key: versionsQuery().queryKey, run: (c) => c.prefetchQuery(versionsQuery()) },
  ...(["fabric", "quilt", "forge", "neoforge"] as const).map((loader) => ({
    key: loaderGameVersionsQuery(loader).queryKey,
    run: (c: QueryClient) => c.prefetchQuery(loaderGameVersionsQuery(loader)),
  })),
];

/**
 * アプリが ready になったあと、作成ダイアログの一覧を先に取っておく（同時に 2 件まで）。
 * 失敗した問い合わせは結果を残さず、開いたときの通常の問い合わせに任せる。
 */
export function usePrefetch() {
  const client = useQueryClient();
  // 起動の状態は StartupGate と同じ問い合わせを共有する
  const { data: status } = useQuery({ queryKey: ["startupStatus"], queryFn: getStartupStatus });
  const ready = status?.state === "ready";

  useEffect(() => {
    if (!ready) return;
    let cancelled = false;
    const queue = jobs();
    const worker = async () => {
      for (let job = queue.shift(); job && !cancelled; job = queue.shift()) {
        await job.run(client);
        // prefetchQuery は失敗を握りつぶして失敗の状態を残す。ダイアログが失敗から始まらないよう、元に戻す
        if (client.getQueryState(job.key)?.status === "error") await client.resetQueries({ queryKey: job.key, exact: true });
      }
    };
    for (let i = 0; i < CONCURRENCY; i++) void worker();
    return () => {
      cancelled = true;
    };
  }, [ready, client]);
}
