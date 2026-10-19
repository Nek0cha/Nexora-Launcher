import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createInstance, listVersions } from "../../api/commands";
import { INSTANCES_KEY } from "./useInstances";

export function useVersions(enabled: boolean) {
  return useQuery({ queryKey: ["versions"], queryFn: listVersions, enabled, staleTime: 10 * 60 * 1000 });
}

export function useCreateInstance() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ name, mcVersion }: { name: string; mcVersion: string }) => createInstance(name, mcVersion),
    onSuccess: () => client.invalidateQueries({ queryKey: INSTANCES_KEY }),
  });
}
