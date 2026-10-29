import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import {
  addSavedSkin,
  addSavedSkinFromPath,
  applyDefaultSkin,
  applySavedSkin,
  deleteSavedSkin,
  getCurrentSkin,
  getSavedSkinImage,
  importCurrentSkin,
  listDefaultSkins,
  listSavedSkins,
  setCape,
  updateSavedSkin,
} from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { SavedSkin } from "../../bindings/SavedSkin";
import type { SavedSkinPatch } from "../../bindings/SavedSkinPatch";
import { SKIN_KEY } from "../accounts/useAccounts";

/** スキンの PNG の上限（本体の検査と同じ 64 KiB） */
export const MAX_SKIN_BYTES = 64 * 1024;

/** スキン画面のクエリの先頭のキー。現在のスキン・保存済みの一覧がこの下にある */
export const SKINS_KEY = ["skins"] as const;
// 画像とデフォルトスキンは変わらない（または取得が重い）ので、SKINS_KEY の取り直しに巻き込まない
const SKIN_IMAGE_KEY = ["skin-image"] as const;
const DEFAULT_SKINS_KEY = ["default-skins"] as const;

/**
 * Mojang はスキンとマントの変更の回数を制限している。続けて押せないように、変更のあと
 * この間は操作を止める（成功したあと / 取り直しが終わってから数える）。
 */
export const CHANGE_COOLDOWN_MS = 3000;
/** HTTP 429 で止められたあとの冷却。すぐ押し直すとさらに長く止められるので長めにとる */
export const RATE_LIMITED_COOLDOWN_MS = 30000;

function isRateLimited(error: unknown): boolean {
  const { code, params } = toErrorPayload(error);
  return code === "net.http_status" && params.status === 429;
}

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

/**
 * アカウントを切り替えた直後は、新しいアカウントのスキンが届くまで前のアカウントの表示（プレビューとチェック）を残し、
 * 空の枠を挟まずに 1 回で移る。アカウントが無い（null）ときは残さない（別のアカウントのスキンを誤って見せない）。
 * 残っている間は isPlaceholderData が真。
 */
export function useCurrentSkin(accountId: string | null) {
  return useQuery({
    queryKey: [...SKINS_KEY, "current", accountId],
    queryFn: () => getCurrentSkin(accountId as string),
    enabled: accountId !== null,
    placeholderData: (previous) => (accountId !== null ? previous : undefined),
  });
}

/**
 * いま着ているスキンをライブラリに取り込む（無ければ追加される）。便利機能なので、失敗は画面に出さず警告だけにする。
 * 追加されたときだけ、保存済みの一覧を取り直す（現在のスキンは変わらないので巻き込まない）。
 */
export function useImportCurrentSkin() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ accountId }: { accountId: string }) => importCurrentSkin(accountId),
    onSuccess: (skin) => (skin ? client.invalidateQueries({ queryKey: [...SKINS_KEY, "saved"] }) : undefined),
    onError: (e) => console.warn("import current skin failed", e),
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
  // 冷却は busy に含める。進行中から冷却へ隙間なくつなぎ、全部の操作が同じ見た目（無効）のまま続くようにする
  const [cooling, setCooling] = useState(false);
  const coolTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(coolTimer.current), []);
  /** 冷却に入る。ms を渡すとその時間で明け、渡さなければ（取り直しの間）明けるのを止めておく */
  const cool = (ms?: number) => {
    clearTimeout(coolTimer.current);
    setCooling(true);
    if (ms !== undefined) coolTimer.current = setTimeout(() => setCooling(false), ms);
  };
  // サイドバーの顔（SKIN_KEY）も、スキンが変われば取り直す
  const refresh = () =>
    Promise.all([
      client.invalidateQueries({ queryKey: SKINS_KEY }),
      client.invalidateQueries({ queryKey: SKIN_KEY }),
    ]);
  // Mojang へ変更を送る操作の成功のあと: 取り直しの間も冷却のままにし、取り直しが終わってから数える
  const changed = async () => {
    cool();
    try {
      await refresh();
    } finally {
      cool(CHANGE_COOLDOWN_MS);
    }
  };
  const limited = (e: unknown) => {
    if (isRateLimited(e)) cool(RATE_LIMITED_COOLDOWN_MS);
  };
  const need = () => {
    if (accountId === null) throw new Error("no account");
    return accountId;
  };

  const apply = useMutation({
    mutationFn: ({ skinId }: { skinId: string }) => applySavedSkin(need(), skinId),
    onSuccess: changed,
    onError: limited,
  });
  const applyDefault = useMutation({
    mutationFn: ({ key }: { key: string }) => applyDefaultSkin(need(), key),
    onSuccess: changed,
    onError: limited,
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
    onSuccess: changed,
    onError: limited,
  });

  const busy =
    cooling || [apply, applyDefault, add, addFromPath, update, remove, setCapeMutation].some((m) => m.isPending);
  return { apply, applyDefault, add, addFromPath, update, remove, setCape: setCapeMutation, busy };
}
