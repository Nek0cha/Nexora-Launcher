import { useIsMac, useIsWindows } from "../../app/platform";
import { Brand } from "./Brand";
import { WindowControls } from "./WindowControls";

/**
 * アプリの上端のバーが出せない画面（起動中、起動の失敗、画面の描画の失敗）で使う、最小限のバー。
 * Windows ではウィンドウの枠を外しているので、つかんで動かせる場所とウィンドウボタンだけは必ず出す。
 * macOS でも OS のタイトルバーを消しているので、信号のボタンを避けた同じバー（ウィンドウボタンなし）を出す。
 * Linux では OS のタイトルバーがあるので、何も出さない。
 */
export function FallbackTitleBar() {
  const isWindows = useIsWindows();
  const isMac = useIsMac();
  if (!isWindows && !isMac) return null;
  return (
    <header
      data-tauri-drag-region="deep"
      data-testid="fallback-title-bar"
      className={`flex h-10 shrink-0 items-center gap-1 bg-surface-1 ${isMac ? "pl-[78px] pr-3" : "pl-6"}`}
    >
      <Brand />
      {/* 空きの部分。ここをつかんでウィンドウを動かす */}
      <div className="h-full min-w-8 flex-1" />
      {isWindows && (
        <div className="ml-2 h-full">
          <WindowControls />
        </div>
      )}
    </header>
  );
}
