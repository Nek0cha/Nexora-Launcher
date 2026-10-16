import { useTranslation } from "react-i18next";
import IconEmpty from "~icons/ph/stack";
import { EmptyState } from "../components/EmptyState";
import { PlayButton } from "../components/PlayButton";
import { useHomeInstance } from "../features/instances/useInstances";

export default function Home() {
  const { t } = useTranslation();
  const instance = useHomeInstance();
  if (!instance) return <EmptyState icon={IconEmpty} title={t("nav.home")} body={t("empty.instances")} />;
  return (
    <div className="flex h-full flex-col justify-end gap-6 p-10">
      <h1 className="font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">{instance.name}</h1>
      <section aria-label={t("play.play")}>
        <PlayButton instance={instance} />
      </section>
    </div>
  );
}
