import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  deleteSyncProfile,
  joinSyncProfile,
  leaveSyncProfile,
  previewJoinSyncProfile,
  renameSyncProfile,
  setSyncCategories,
} from "../../api/commands";
import type { Category } from "../../bindings/Category";
import { useToast } from "../../components/toast/useToast";
import { SYNC_KEY } from "./useInstanceSync";

export { useSyncProfiles } from "./useInstanceSync";

/** 画面に並べるカテゴリの順。保存するときもこの順に揃える */
export const CATEGORIES: Category[] = ["keys", "mouse", "sound", "graphics", "chat", "accessibility", "language"];

export function orderCategories(categories: Iterable<Category>): Category[] {
  const set = new Set(categories);
  return CATEGORIES.filter((c) => set.has(c));
}

/** 共通設定の画面の変更。成功したら共通設定のキャッシュをまとめて取り直し、失敗はトーストで出す */
export function useSyncProfileActions() {
  const { t } = useTranslation();
  const client = useQueryClient();
  const toast = useToast();
  // 取り直しが終わるまで mutation を進行中のままにする。先に終わると、画面が一瞬古い値に戻ってから新しい値になる
  const refresh = () => client.invalidateQueries({ queryKey: SYNC_KEY });

  const rename = useMutation({
    mutationKey: SYNC_KEY,
    mutationFn: ({ profileId, name }: { profileId: string; name: string }) => renameSyncProfile(profileId, name),
    onSuccess: refresh,
    onError: (e) => {
      refresh();
      toast.showError(e);
    },
  });

  const categories = useMutation({
    mutationKey: SYNC_KEY,
    mutationFn: ({ profileId, next }: { profileId: string; next: Category[] }) =>
      setSyncCategories(profileId, orderCategories(next)),
    onSuccess: refresh,
    onError: toast.showError,
  });

  const leave = useMutation({
    mutationKey: SYNC_KEY,
    mutationFn: ({ instanceId }: { instanceId: string; name: string }) => leaveSyncProfile(instanceId),
    onSuccess: (_, { name }) => {
      refresh();
      toast.show({ kind: "success", message: t("sync.screen.left", { name }) });
    },
    onError: toast.showError,
  });

  const remove = useMutation({
    mutationKey: SYNC_KEY,
    mutationFn: ({ profileId }: { profileId: string; name: string }) => deleteSyncProfile(profileId),
    onSuccess: (_, { name }) => {
      refresh();
      toast.show({ kind: "success", message: t("sync.screen.deleted", { name }) });
    },
    onError: toast.showError,
  });

  /**
   * 参加の最初の段。値の違いが無ければそのまま参加して 0 を返し、違いがあれば参加せずに件数を返す
   * （どちらに揃えるかは、呼び出し側が JoinSyncDialog で聞く）。
   */
  const join = useMutation({
    mutationKey: SYNC_KEY,
    mutationFn: async ({ instanceId, profileId }: { instanceId: string; profileId: string }) => {
      const differing = await previewJoinSyncProfile(instanceId, profileId);
      if (differing > 0) return differing;
      await joinSyncProfile(instanceId, profileId, "keepProfile");
      return 0;
    },
    onSuccess: (differing) => {
      if (differing === 0) refresh();
    },
    onError: toast.showError,
  });

  return { rename, categories, leave, remove, join };
}
