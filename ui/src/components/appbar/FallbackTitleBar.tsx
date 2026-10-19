import { useTranslation } from "react-i18next";
import { useIsWindows } from "../../app/platform";
import { Logo } from "./AppBar";
import { WindowControls } from "./WindowControls";

/**
 * アプリの上端のバーが出せない画面（起動中、起動の失敗、画面の描画の失敗）で使う、最小限のバー。
 * Windows ではウィンドウの枠を外しているので、つかんで動かせる場所とウィンドウボタンだけは必ず出す。
 * ほかの OS では OS のタイトルバーがあるので、何も出さない。
 */
export function FallbackTitleBar() {
  const { t } = useTranslation();
  const isWindows = useIsWindows();
  if (!isWindows) return null;
  return (
    <header
      data-tauri-drag-region="deep"
      data-testid="fallback-title-bar"
      className="flex h-10 shrink-0 items-center gap-1 bg-surface-1 pl-3"
    >
      <Logo />
      <span className="ml-1.5 font-display text-sm font-semibold tracking-[-0.01em] text-text-strong">
        {t("appName")}
      </span>
      {/* 空きの部分。ここをつかんでウィンドウを動かす */}
      <div className="h-full min-w-8 flex-1" />
      <div className="ml-2 h-full">
        <WindowControls />
      </div>
    </header>
  );
}
