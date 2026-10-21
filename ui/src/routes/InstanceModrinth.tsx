import { useTranslation } from "react-i18next";
import { Link, useParams } from "react-router";
import IconBack from "~icons/ph/arrow-left";
import { toErrorPayload } from "../api/ipc";
import { FailureNotice } from "../components/FailureNotice";
import { ModrinthBrowser } from "../components/modrinth/ModrinthBrowser";
import { Notice } from "../components/settingsLayout";
import { useInstances } from "../features/instances/useInstances";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 見出しの横の控えめなリンク（インスタンスの詳細の「インスタンス」へ戻るリンクと同じ）
const quietLink = [
  "inline-flex h-8 min-w-0 items-center gap-1.5 rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  ring,
].join(" ");

const title = "font-display text-xl font-semibold tracking-[-0.01em] text-text-strong";

/** インスタンスの「コンテンツ」タブから開く、そのインスタンスに MOD を追加する画面（spec 6.1）。 */
export default function InstanceModrinth() {
  const { t } = useTranslation();
  const { id } = useParams();
  const { data, error } = useInstances();

  if (!data && error)
    return (
      <div className="flex flex-col gap-8 p-10">
        <h1 className={title}>{t("nav.instances")}</h1>
        <div role="alert">
          <FailureNotice error={toErrorPayload(error)} />
        </div>
      </div>
    );
  if (!data) return null;

  const instance = data.find((i) => i.id === id);
  if (!instance)
    return (
      <div className="flex max-w-2xl flex-col items-start gap-4 p-10">
        <p className="text-text">{t("instances.detail.notFound")}</p>
        <Link to="/instances" className={`${quietLink} -ml-2.5`}>
          <IconBack aria-hidden className="size-4" />
          <span>{t("instances.detail.backToList")}</span>
        </Link>
      </div>
    );

  return (
    <div className="flex h-full min-h-[34rem] max-w-5xl flex-col gap-6 p-10">
      <header className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h1 className={`${title} truncate`}>{t("modrinth.instanceTitle", { name: instance.name })}</h1>
          <span className="text-sm text-text-muted">{t("modrinth.title")}</span>
        </div>
        {/* 戻るとコンテンツのタブを開く */}
        <Link to={`/instances/${instance.id}`} state={{ tab: "content" }} className={`${quietLink} shrink-0`}>
          <IconBack aria-hidden className="size-4 shrink-0" />
          <span className="max-w-48 truncate">{instance.name}</span>
        </Link>
      </header>
      {instance.loader === "vanilla" ? (
        <Notice tone="info">{t("modrinth.noLoader")}</Notice>
      ) : (
        // インスタンスが替わったら絞り込みを作り直す
        <ModrinthBrowser key={instance.id} instance={instance} />
      )}
    </div>
  );
}
