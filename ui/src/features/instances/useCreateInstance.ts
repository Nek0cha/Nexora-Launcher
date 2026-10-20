import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { LoaderKind } from "../../bindings/LoaderKind";
import type { NewInstance } from "../../bindings/NewInstance";
import { createInstance, listLoaderGameVersions, listLoaderVersions, listVersions } from "../../api/commands";
import { INSTANCES_KEY } from "./useInstances";

export function useVersions(enabled: boolean) {
  return useQuery({ queryKey: ["versions"], queryFn: listVersions, enabled, staleTime: 10 * 60 * 1000 });
}

export function useLoaderGameVersions(loader: LoaderKind, enabled: boolean) {
  return useQuery({
    queryKey: ["loader-game-versions", loader],
    queryFn: () => listLoaderGameVersions(loader),
    enabled,
    staleTime: 10 * 60 * 1000,
  });
}

export function useLoaderVersions(loader: LoaderKind, mcVersion: string | null, enabled: boolean) {
  return useQuery({
    queryKey: ["loader-versions", loader, mcVersion],
    queryFn: () => listLoaderVersions(loader, mcVersion as string),
    enabled: enabled && mcVersion !== null,
    staleTime: 10 * 60 * 1000,
  });
}

export function useCreateInstance() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (request: NewInstance) => createInstance(request),
    onSuccess: () => client.invalidateQueries({ queryKey: INSTANCES_KEY }),
  });
}
