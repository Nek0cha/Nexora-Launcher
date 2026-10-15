import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { NumberField } from "../components/NumberField";
import { Select } from "../components/Select";
import { useSettings, useUpdateSetting } from "../features/settings/useSettings";
import { detectLanguage, type Lang } from "../i18n";
import type { Theme } from "../bindings/Theme";

// 1段の行は 44px、ラベルと説明の2段の行は 52px（mode-app の行の高さ）
function Section({ title, twoLine = false, children }: { title: string; twoLine?: boolean; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="select-none text-xs font-medium tracking-[0.08em] text-text-muted">{title}</h2>
      {/* 設定の項目は細い線で区切る */}
      <div
        className={`flex flex-col divide-y divide-line border-y border-line [&>*]:py-1 ${twoLine ? "[&>*]:min-h-13" : "[&>*]:min-h-11"}`}
      >
        {children}
      </div>
    </section>
  );
}

export default function Settings() {
  const { t } = useTranslation("settings");
  const { data } = useSettings();
  const update = useUpdateSetting();
  if (!data) return null;
  const language: Lang = data.language ?? detectLanguage(navigator.language);

  return (
    <div className="flex max-w-2xl flex-col gap-8 p-10">
      <h1 className="font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">{t("title")}</h1>
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
      <Section title={t("downloads.section")} twoLine>
        <NumberField
          label={t("downloads.label")}
          description={t("downloads.description")}
          value={data.maxConcurrentDownloads}
          min={1}
          max={16}
          onChange={(value) => update.mutate({ key: "maxConcurrentDownloads", value })}
        />
      </Section>
    </div>
  );
}
