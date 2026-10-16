import { useQuery } from "@tanstack/react-query";
import { listInstances } from "../../api/commands";
import type { Instance } from "../../bindings/Instance";

export const INSTANCES_KEY = ["instances"] as const;

export function useInstances() {
  return useQuery({ queryKey: INSTANCES_KEY, queryFn: listInstances });
}

/** ホームに出すインスタンス。backend が最後に遊んだ順（なければ作成の新しい順）に並べている。 */
export function useHomeInstance(): Instance | undefined {
  return useInstances().data?.[0];
}
