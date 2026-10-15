import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import IconWarning from "~icons/ph/warning-circle";
import { getStartupStatus } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { ErrorPayload } from "../bindings/ErrorPayload";
import { errorKey } from "../features/errors/errorText";
import i18n from "../i18n";

export function StartupGate({ children }: { children: ReactNode }) {
  const { data: status, error } = useQuery({
    queryKey: ["startupStatus"],
    queryFn: getStartupStatus,
    // 起動中の間だけ 100ms ごとに取り直す
    refetchInterval: (query) => (query.state.data?.state === "starting" ? 100 : false),
  });
  // 状態そのものを取得できない場合も、空の画面のままにせず起動エラーとして見せる
  if (error) return <StartupError data={toErrorPayload(error)} />;
  if (!status || status.state === "starting") return null;
  if (status.state === "ready") return <>{children}</>;
  return <StartupError data={status.error} />;
}

function StartupError({ data }: { data: ErrorPayload }) {
  const { t } = useTranslation();
  return (
    <main role="alert" className="mx-auto flex min-h-screen max-w-xl flex-col justify-center gap-4 bg-surface-0 p-8">
      <IconWarning aria-hidden className="size-8 text-danger" />
      <h1 className="font-display text-xl font-semibold text-text-strong">{t("startup.title")}</h1>
      <p className="text-text">{t("startup.body")}</p>
      {/* 利用者向けの説明。報告に貼れるよう選択できるようにする。コードと詳細はその下に置く */}
      <p className="select-text text-text">
        {t(errorKey(data.code, (k) => i18n.exists(k)) as never, {
          ...(data.params as Record<string, unknown>),
          code: data.code,
        })}
      </p>
      {/* 報告に貼れるよう、コードと詳細も選択できるようにする */}
      <pre className="tabular select-textoverflow-auto rounded-md border border-line bg-surface-1 p-3 text-xs whitespace-pre-wrap text-text">
        {data.code}
        {data.detail ? `\n${data.detail}` : ""}
      </pre>
    </main>
  );
}
