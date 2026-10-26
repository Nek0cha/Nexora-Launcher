import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { LoaderKind } from "../../bindings/LoaderKind";
import type { NewInstance } from "../../bindings/NewInstance";
import { useTranslation } from "react-i18next";
import { createInstance, joinSyncProfile, listLoaderGameVersions, listLoaderVersions, listVersions } from "../../api/commands";
import { useToast } from "../../components/toast/useToast";
import { SYNC_KEY } from "../sync/useInstanceSync";
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

export type CreateRequest = {
  request: NewInstance;
  /** 作成のあとで参加する共通設定。null なら参加しない */
  syncProfileId: string | null;
};

/**
 * インスタンスを作り、共通設定が選ばれていれば、作ったあとで参加させる。
 * 新しいインスタンスには設定ファイルが無く、差は 0 件になるため、共通設定に合わせる側で参加する。
 * 参加の失敗は作成の失敗にせず、トーストで知らせる。
 */
export function useCreateInstance() {
  const { t } = useTranslation();
  const client = useQueryClient();
  const toast = useToast();
  return useMutation({
    mutationFn: async ({ request, syncProfileId }: CreateRequest) => {
      const created = await createInstance(request);
      if (syncProfileId !== null) {
        try {
          await joinSyncProfile(created.instance.id, syncProfileId, "keepProfile");
        } catch {
          toast.show({ kind: "error", message: t("create.shared.joinFailed", { name: created.instance.name }) });
        }
        void client.invalidateQueries({ queryKey: SYNC_KEY });
      }
      return created;
    },
    onSuccess: () => client.invalidateQueries({ queryKey: INSTANCES_KEY }),
  });
}
