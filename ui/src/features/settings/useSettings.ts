import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getSettings, setSetting } from "../../api/commands";
import type { SettingChange } from "../../bindings/SettingChange";
import type { Settings } from "../../bindings/Settings";

export const SETTINGS_KEY = ["settings"] as const;

export function useSettings() {
  return useQuery<Settings>({ queryKey: SETTINGS_KEY, queryFn: getSettings });
}

export function useUpdateSetting() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (change: SettingChange) => setSetting(change),
    onSuccess: (settings) => queryClient.setQueryData(SETTINGS_KEY, settings),
  });
}
