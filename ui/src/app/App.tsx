import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { I18nProvider } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { Outlet } from "react-router";
import { appInfo } from "../api/commands";
import { Sidebar } from "../components/Sidebar";
import { useTaskSync } from "../features/tasks/useTasks";
import { useSettings, useUpdateSetting } from "../features/settings/useSettings";
import { detectLanguage, setLanguage } from "../i18n";
import { applyTheme } from "./theme";

export function App() {
  useTaskSync();
  const { i18n } = useTranslation();
  const { data: settings } = useSettings();
  const update = useUpdateSetting();
  const { data: info } = useQuery({ queryKey: ["appInfo"], queryFn: appInfo });

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

  const forcedColors = window.matchMedia("(forced-colors: active)").matches;
  const materialOn = Boolean(info?.materialAvailable) && !forcedColors;

  useEffect(() => {
    document.documentElement.dataset.material = materialOn ? "on" : "off";
  }, [materialOn]);

  return (
    // React Aria の部品（数値の書式、増減ボタンの読み上げ名）もアプリの言語に合わせる
    <I18nProvider locale={i18n.language}>
      <div className="flex h-screen">
        <Sidebar materialOn={materialOn} />
        <main className="min-w-0 flex-1 overflow-auto bg-surface-0">
          <Outlet />
        </main>
      </div>
    </I18nProvider>
  );
}
