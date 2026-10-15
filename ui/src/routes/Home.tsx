import { useTranslation } from "react-i18next";
import { PlayButton } from "../components/PlayButton";

export default function Home() {
  const { t } = useTranslation();
  return (
    <div className="flex h-full flex-col justify-end gap-6 p-10">
      {/* 仮の文言。実際のインスタンス名が入る見出しと同じ組み方にし、プレイボタンの上で何を起動するかを示す */}
      <h1 className="font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">{t("home.placeholderInstance")}</h1>
      <section aria-label={t("play.play")}>
        <PlayButton />
      </section>
    </div>
  );
}
