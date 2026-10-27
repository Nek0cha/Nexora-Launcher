import { useQuery } from "@tanstack/react-query";
import { MotionConfig } from "motion/react";
import { useEffect, useSyncExternalStore } from "react";
import { I18nProvider } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { Outlet } from "react-router";
import { appInfo } from "../api/commands";
import { AppBar } from "../components/appbar/AppBar";
import { RunningPill } from "../components/appbar/RunningPill";
import { CommandPalette } from "../components/CommandPalette";
import { Sidebar } from "../components/Sidebar";
import { ToastRegion } from "../components/toast/ToastRegion";
import { useTaskSync } from "../features/tasks/useTasks";
import { useGameSync } from "../features/play/useGameSync";
import { useSyncEvents } from "../features/sync/useSyncEvents";
import { useSettings, useUpdateSetting } from "../features/settings/useSettings";
import { detectLanguage, setLanguage } from "../i18n";
import { useIsWindows } from "./platform";
import { applyTheme } from "./theme";

const FORCED_COLORS = "(forced-colors: active)";

function subscribeForcedColors(onChange: () => void): () => void {
  const media = window.matchMedia(FORCED_COLORS);
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}

/** 強制カラーモードの切り替えに、再起動なしで追従する */
function useForcedColors(): boolean {
  return useSyncExternalStore(subscribeForcedColors, () => window.matchMedia(FORCED_COLORS).matches);
}

export function App() {
  useTaskSync();
  useGameSync();
  useSyncEvents();
  const { i18n } = useTranslation();
  const { data: settings } = useSettings();
  const update = useUpdateSetting();
  const { data: info } = useQuery({ queryKey: ["appInfo"], queryFn: appInfo });
  // app_info が失敗しても、Windows ではウィンドウボタンを出す
  const isWindows = useIsWindows();

  useEffect(() => {
    if (!settings) return;
    return applyTheme(settings.theme);
  }, [settings?.theme]);

  useEffect(() => {
    if (!settings) return;
    if (settings.language === null) {
      // 初回は OS の言語で決めて保存する
      update.mutate({ key: "language", value: detectLanguage(navigator.language) });
      return;
    }
    void setLanguage(settings.language);
  }, [settings?.language]);

  const forcedColors = useForcedColors();
  const materialOn = Boolean(info?.materialAvailable) && !forcedColors;

  useEffect(() => {
    document.documentElement.dataset.material = materialOn ? "on" : "off";
  }, [materialOn]);

  return (
    // React Aria の部品（数値の書式、増減ボタンの読み上げ名）もアプリの言語に合わせる
    <I18nProvider locale={i18n.language}>
      {/* Motion の動きは、OS の「視差効果を減らす」に従う */}
      <MotionConfig reducedMotion="user">
        <div className="flex h-screen flex-col">
          {/* Windows では OS のタイトルバーの代わり、macOS と Linux では OS のタイトルバーの下に置く */}
          <AppBar showWindowControls={isWindows} materialOn={materialOn} slot={<RunningPill />} />
          <div className="flex min-h-0 flex-1">
            <Sidebar materialOn={materialOn} />
            <main className="scroll-thin min-w-0 flex-1 overflow-auto bg-surface-0">
              <Outlet />
            </main>
          </div>
          <CommandPalette />
        </div>
        <ToastRegion />
      </MotionConfig>
    </I18nProvider>
  );
}
