import { useTranslation } from "react-i18next";

export default function Home() {
  const { t } = useTranslation();
  return (
    <div className="flex h-full flex-col justify-end gap-6 p-10">
      <p className="text-text-muted">{t("home.placeholderInstance")}</p>
      <section aria-label={t("play.play")} data-slot="play" />
    </div>
  );
}
