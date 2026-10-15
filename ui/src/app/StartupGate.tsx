import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import IconWarning from "~icons/ph/warning-circle";
import { getStartupStatus } from "../api/commands";

export function StartupGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const { data: status } = useQuery({
    queryKey: ["startupStatus"],
    queryFn: getStartupStatus,
    // 起動中の間だけ 100ms ごとに取り直す
    refetchInterval: (query) => (query.state.data?.state === "starting" ? 100 : false),
  });
  if (!status || status.state === "starting") return null;
  if (status.state === "ready") return <>{children}</>;
  const data = status.error;
  return (
    <main role="alert" className="mx-auto flex min-h-screen max-w-xl flex-col justify-center gap-4 bg-surface-0 p-8">
      <IconWarning aria-hidden className="size-8 text-danger" />
      <h1 className="font-display text-xl font-semibold text-text-strong">{t("startup.title")}</h1>
      <p className="text-text">{t("startup.body")}</p>
      {/* 報告に貼れるよう、コードと詳細は選択できるまま残す */}
      <pre className="tabular overflow-auto rounded-md border border-line bg-surface-1 p-3 text-xs whitespace-pre-wrap text-text">
        {data.code}
        {data.detail ? `\n${data.detail}` : ""}
      </pre>
    </main>
  );
}
