import { useQuery } from "@tanstack/react-query";
import { listInstances } from "../../api/commands";
import type { Instance } from "../../bindings/Instance";

export const INSTANCES_KEY = ["instances"] as const;

export function useInstances() {
  return useQuery({ queryKey: INSTANCES_KEY, queryFn: listInstances });
}

/**
 * ホームとコマンドパレットが共有する、ホームに出すインスタンス。
 * 一覧の先頭（backend が最後に遊んだ順、なければ作成の新しい順に並べている）を返す。
 */
export function useHomeInstance(): Instance | undefined {
  return useInstances().data?.[0];
}
