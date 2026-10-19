import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import IconWarning from "~icons/ph/warning-circle";
import { getStartupStatus } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { ErrorPayload } from "../bindings/ErrorPayload";
import { FallbackTitleBar } from "../components/appbar/FallbackTitleBar";
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
  // 起動中は中身を出さないが、Windows では枠が無いので、ウィンドウを動かしたり閉じたりできるバーだけは出す
  if (!status || status.state === "starting") return <FallbackTitleBar />;
  if (status.state === "ready") return <>{children}</>;
  return <StartupError data={status.error} />;
}

function StartupError({ data }: { data: ErrorPayload }) {
  const { t } = useTranslation();
  return (
    <div className="flex h-screen flex-col bg-surface-0">
      <FallbackTitleBar />
      <main
        role="alert"
        className="scroll-thin mx-auto flex w-full max-w-xl flex-1 flex-col justify-center gap-4 overflow-auto p-8"
      >
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
        <pre className="tabular scroll-thin select-text overflow-auto rounded-md border border-line bg-surface-1 p-3 text-xs whitespace-pre-wrap text-text">
          {data.code}
          {data.detail ? `\n${data.detail}` : ""}
        </pre>
      </main>
    </div>
  );
}
