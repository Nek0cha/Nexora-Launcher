import { useTranslation } from "react-i18next";
import IconEmpty from "~icons/ph/stack";
import { toErrorPayload } from "../api/ipc";
import { CreateInstanceDialog } from "../components/CreateInstanceDialog";
import { EmptyState } from "../components/EmptyState";
import { FailureNotice } from "../components/FailureNotice";
import { PlayButton } from "../components/PlayButton";
import { useInstances } from "../features/instances/useInstances";

export default function Home() {
  const { t } = useTranslation();
  const { data, error } = useInstances();
  if (error)
    return (
      <div className="flex h-full flex-col justify-end gap-6 p-10">
        <h1 className="font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">{t("nav.home")}</h1>
        <div role="alert">
          <FailureNotice error={toErrorPayload(error)} />
        </div>
      </div>
    );
  // 届く前に空の状態を出すと、インスタンスがあるのに一瞬「まだありません」と見えるため、何も出さない
  if (!data) return null;
  // backend が最後に遊んだ順（なければ作成の新しい順）に並べている
  const instance = data[0];
  if (!instance)
    return (
      <EmptyState icon={IconEmpty} title={t("nav.home")} body={t("empty.instances")}>
        <CreateInstanceDialog trigger="primary" />
      </EmptyState>
    );
  return (
    <div className="flex h-full flex-col justify-end gap-6 p-10">
      <header className="flex flex-col gap-1">
        <div className="flex items-center gap-3">
          <h1 className="min-w-0 truncate font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">
            {instance.name}
          </h1>
          <CreateInstanceDialog trigger="quiet" />
        </div>
        <p className="text-sm text-text-muted">{instance.mcVersion}</p>
      </header>
      <section aria-label={t("play.play")}>
        <PlayButton instance={instance} />
      </section>
    </div>
  );
}
