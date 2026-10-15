import { useMutation, useMutationState, useQuery, useQueryClient } from "@tanstack/react-query";
import { getSettings, setSetting } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import type { SettingChange } from "../../bindings/SettingChange";
import type { Settings } from "../../bindings/Settings";

export const SETTINGS_KEY = ["settings"] as const;
const SET_KEY = ["setSetting"] as const;

export function useSettings() {
  return useQuery<Settings>({ queryKey: SETTINGS_KEY, queryFn: getSettings });
}

function applyChange(settings: Settings, change: SettingChange): Settings {
  return { ...settings, [change.key]: change.value };
}

/**
 * 設定を 1 つ書き換える。呼び出し元（設定画面とコマンドパレット）で共有される。
 * - 画面の値は書き込みの応答を待たずに先に更新する（続けて押した増減が、直前の値から続くように）。失敗したら元に戻す。
 * - 同じ scope の書き込みは 1 件ずつ順に実行される。応答の順が入れ替わって古い値が残ることはない。
 */
export function useUpdateSetting() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationKey: SET_KEY,
    scope: { id: "settings" },
    mutationFn: (change: SettingChange) => setSetting(change),
    onMutate: async (change) => {
      // 取得中の古い結果が、先に更新した値を上書きしないようにする
      await queryClient.cancelQueries({ queryKey: SETTINGS_KEY });
      const previous = queryClient.getQueryData<Settings>(SETTINGS_KEY);
      if (previous) queryClient.setQueryData(SETTINGS_KEY, applyChange(previous, change));
      return { previous };
    },
    onError: (_error, _change, context) => {
      if (context?.previous) queryClient.setQueryData(SETTINGS_KEY, context.previous);
    },
    onSuccess: (settings) => {
      // 後続の書き込みが待っているあいだは、その先に更新した値を応答で戻さない
      if (queryClient.isMutating({ mutationKey: SET_KEY }) <= 1) queryClient.setQueryData(SETTINGS_KEY, settings);
    },
  });
}

/**
 * 最後に終わった設定の書き込みの失敗。次に書き込みに成功すると消える。
 * 書き込みはどの画面からでも行えるため、mutation の履歴から求めて呼び出し元をまたいで共有する。
 */
export function useSettingsWriteError(): ErrorPayload | null {
  const settled = useMutationState({
    filters: { mutationKey: SET_KEY },
    select: (m) => ({ status: m.state.status, error: m.state.error }),
  }).filter((m) => m.status === "error" || m.status === "success");
  const last = settled[settled.length - 1];
  return last?.status === "error" ? toErrorPayload(last.error) : null;
}
