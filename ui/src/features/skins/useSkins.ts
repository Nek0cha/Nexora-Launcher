import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  addSavedSkin,
  addSavedSkinFromPath,
  applyDefaultSkin,
  applySavedSkin,
  deleteSavedSkin,
  getCurrentSkin,
  getSavedSkinImage,
  listDefaultSkins,
  listSavedSkins,
  setCape,
  updateSavedSkin,
} from "../../api/commands";
import type { SavedSkin } from "../../bindings/SavedSkin";
import type { SavedSkinPatch } from "../../bindings/SavedSkinPatch";
import { SKIN_KEY } from "../accounts/useAccounts";

/** スキン画面のクエリの先頭のキー。現在のスキン・保存済みの一覧がこの下にある */
export const SKINS_KEY = ["skins"] as const;
// 画像とデフォルトスキンは変わらない（または取得が重い）ので、SKINS_KEY の取り直しに巻き込まない
const SKIN_IMAGE_KEY = ["skin-image"] as const;
const DEFAULT_SKINS_KEY = ["default-skins"] as const;

export function useSavedSkins() {
  return useQuery({ queryKey: [...SKINS_KEY, "saved"], queryFn: listSavedSkins });
}

/** 保存済みスキンの画像。内容が変わると sha256 が変わるので、キーに含める */
export function useSavedSkinImage(skin: SavedSkin) {
  return useQuery({
    queryKey: [...SKIN_IMAGE_KEY, skin.id, skin.sha256],
    queryFn: () => getSavedSkinImage(skin.id),
    staleTime: Infinity,
  });
}

export function useCurrentSkin(accountId: string | null) {
  return useQuery({
    queryKey: [...SKINS_KEY, "current", accountId],
    queryFn: () => getCurrentSkin(accountId as string),
    enabled: accountId !== null,
  });
}

/** 初回はゲームの jar を取りに行くことがあり、時間がかかり、失敗もする。自動では取り直さず、再試行は refetch で呼び出し側が決める */
export function useDefaultSkins() {
  return useQuery({
    queryKey: DEFAULT_SKINS_KEY,
    queryFn: listDefaultSkins,
    retry: false,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });
}

/** スキン画面の操作。成功したら取り直しが終わるまで進行中のままにする（古い表示に一瞬戻らないように） */
export function useSkinActions(accountId: string | null) {
  const client = useQueryClient();
  // サイドバーの顔（SKIN_KEY）も、スキンが変われば取り直す
  const refresh = () =>
    Promise.all([
      client.invalidateQueries({ queryKey: SKINS_KEY }),
      client.invalidateQueries({ queryKey: SKIN_KEY }),
    ]);
  const need = () => {
    if (accountId === null) throw new Error("no account");
    return accountId;
  };

  const apply = useMutation({
    mutationFn: ({ skinId }: { skinId: string }) => applySavedSkin(need(), skinId),
    onSuccess: refresh,
  });
  const applyDefault = useMutation({
    mutationFn: ({ key }: { key: string }) => applyDefaultSkin(need(), key),
    onSuccess: refresh,
  });
  const add = useMutation({
    mutationFn: ({ name, png }: { name: string; png: number[] }) => addSavedSkin(name, png),
    onSuccess: refresh,
  });
  const addFromPath = useMutation({
    mutationFn: ({ path }: { path: string }) => addSavedSkinFromPath(path),
    onSuccess: refresh,
  });
  const update = useMutation({
    mutationFn: ({ skinId, patch }: { skinId: string; patch: SavedSkinPatch }) => updateSavedSkin(skinId, patch),
    onSuccess: refresh,
  });
  const remove = useMutation({
    mutationFn: ({ skinId }: { skinId: string }) => deleteSavedSkin(skinId),
    onSuccess: refresh,
  });
  const setCapeMutation = useMutation({
    mutationFn: ({ capeId }: { capeId: string | null }) => setCape(need(), capeId),
    onSuccess: refresh,
  });

  const busy = [apply, applyDefault, add, addFromPath, update, remove, setCapeMutation].some((m) => m.isPending);
  return { apply, applyDefault, add, addFromPath, update, remove, setCape: setCapeMutation, busy };
}
