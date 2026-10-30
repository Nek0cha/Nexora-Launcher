import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { NumberField } from "../components/NumberField";
import { Select } from "../components/Select";
import { FailureNotice } from "../components/FailureNotice";
import { toErrorPayload } from "../api/ipc";
import { useSettings, useSettingsWriteError, useUpdateSetting } from "../features/settings/useSettings";
import { detectLanguage, type Lang } from "../i18n";
import type { Theme } from "../bindings/Theme";

// 行の高さは 44px（mode-app の行の高さ）
function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-xs font-medium tracking-[0.08em] text-text-muted">{title}</h2>
      {/* 設定の項目は細い線で区切る */}
      <div className="flex flex-col divide-y divide-line border-y border-line [&>*]:min-h-11 [&>*]:py-1">{children}</div>
    </section>
  );
}

export default function Settings() {
  const { t } = useTranslation("settings");
  const { data, error } = useSettings();
  const update = useUpdateSetting();
  const writeError = useSettingsWriteError();
  const title = (
    <h1 className="font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">{t("title")}</h1>
  );
  // 読み込みに失敗しても空白にせず、理由を見せる
  if (error)
    return (
      <div className="flex max-w-2xl flex-col gap-8 p-10">
        {title}
        <div role="alert">
          <FailureNotice error={toErrorPayload(error)} />
        </div>
      </div>
    );
  if (!data) return null;
  const language: Lang = data.language ?? detectLanguage(navigator.language);

  return (
    <div className="flex max-w-2xl flex-col gap-8 p-10">
      {title}
      {/* 書き込みの失敗は、コマンドパレットからのものも含めてここに出す。次の書き込みの成功で消える */}
      {writeError && (
        <div role="alert">
          <FailureNotice error={writeError} />
        </div>
      )}
      <Section title={t("appearance")}>
        <Select<Theme>
          label={t("theme.label")}
          value={data.theme}
          options={[
            { id: "system", label: t("theme.system") },
            { id: "dark", label: t("theme.dark") },
            { id: "light", label: t("theme.light") },
          ]}
          onChange={(value) => update.mutate({ key: "theme", value })}
        />
        <Select<Lang>
          label={t("language.label")}
          value={language}
          options={[
            { id: "ja", label: t("language.ja") },
            { id: "en", label: t("language.en") },
          ]}
          onChange={(value) => update.mutate({ key: "language", value })}
        />
      </Section>
      <Section title={t("downloads.section")}>
        <NumberField
          label={t("downloads.label")}
          value={data.maxConcurrentDownloads}
          min={1}
          max={16}
          onChange={(value) => update.mutate({ key: "maxConcurrentDownloads", value })}
        />
      </Section>
    </div>
  );
}
