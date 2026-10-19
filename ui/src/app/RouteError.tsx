import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { isRouteErrorResponse, useRouteError } from "react-router";
import IconWarning from "~icons/ph/warning-circle";
import { FallbackTitleBar } from "../components/appbar/FallbackTitleBar";

function describe(error: unknown): string {
  if (isRouteErrorResponse(error)) return `${error.status} ${error.statusText}`;
  if (error instanceof Error) return error.stack ?? `${error.name}: ${error.message}`;
  return String(error);
}

/**
 * 画面の描画に失敗したときの表示。アプリのバーごと置き換わるので、
 * Windows では最小限のバー（ドラッグ領域とウィンドウボタン）を自分で出す。
 */
export function RouteError() {
  const { t } = useTranslation();
  const error = useRouteError();
  return (
    <div className="flex h-screen flex-col bg-surface-0">
      <FallbackTitleBar />
      <main
        role="alert"
        className="scroll-thin mx-auto flex w-full max-w-xl flex-1 flex-col justify-center gap-4 overflow-auto p-8"
      >
        <IconWarning aria-hidden className="size-8 text-danger" />
        <h1 className="font-display text-xl font-semibold text-text-strong">{t("routeError.title")}</h1>
        <p className="text-text">{t("routeError.body")}</p>
        {/* 報告に貼れるよう、詳細は選択できるようにする */}
        <pre className="tabular scroll-thin select-text max-h-64 overflow-auto rounded-md border border-line bg-surface-1 p-3 text-xs whitespace-pre-wrap text-text">
          {describe(error)}
        </pre>
        <Button
          onPress={() => window.location.reload()}
          className="inline-flex h-9 items-center self-start rounded-md bg-text-strong px-4 text-sm font-medium text-surface-0 transition-opacity duration-(--duration-fast) ease-out-quint data-hovered:opacity-90 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        >
          {t("routeError.reload")}
        </Button>
      </main>
    </div>
  );
}
