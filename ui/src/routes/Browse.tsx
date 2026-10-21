import { useTranslation } from "react-i18next";
import { ModrinthBrowser } from "../components/modrinth/ModrinthBrowser";

const title = "font-display text-xl font-semibold tracking-[-0.01em] text-text-strong";

/** サイドバーから開く、インスタンスに関係なく MOD を探す画面（spec 6.1）。 */
export default function Browse() {
  const { t } = useTranslation();
  return (
    <div className="flex h-full min-h-[34rem] max-w-5xl flex-col gap-6 p-10">
      <header className="flex flex-col gap-0.5">
        <h1 className={title}>{t("modrinth.browseTitle")}</h1>
        <span className="text-sm text-text-muted">{t("modrinth.title")}</span>
      </header>
      <ModrinthBrowser />
    </div>
  );
}
