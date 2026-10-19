import { getCurrentWindow } from "@tauri-apps/api/window";
import type { MouseEvent, ReactNode } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { useLocation, useMatch } from "react-router";
import IconCaretLeft from "~icons/ph/caret-left";
import IconCaretRight from "~icons/ph/caret-right";
import { useInstances } from "../../features/instances/useInstances";
import { useHistoryNav } from "../../features/nav/useHistoryNav";
import { TaskButton } from "./TaskButton";
import { WindowControls } from "./WindowControls";

type NavKey = "home" | "instances" | "browse" | "accounts" | "settings";

const SECTIONS: Record<string, NavKey> = {
  "": "home",
  instances: "instances",
  browse: "browse",
  accounts: "accounts",
  settings: "settings",
};

// ドラッグ領域の中でも、押せる部品の上のダブルクリックは最大化に使わない（Tauri の判定と同じ考え）
const INTERACTIVE = "button, a, input, select, textarea, label, [role='button'], [role='link'], [tabindex]:not([tabindex='-1'])";

const navButton = [
  "grid size-7 place-items-center rounded-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "data-hovered:bg-surface-2 data-hovered:text-text data-pressed:bg-surface-3",
  "data-disabled:text-text-faint data-disabled:data-hovered:bg-transparent",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

/** 今の画面の名前。インスタンスの詳細では、そのインスタンスの名前を出す。 */
function usePageName(): string {
  const { t } = useTranslation();
  const { pathname } = useLocation();
  const detail = useMatch("/instances/:id");
  const { data: instances } = useInstances();
  if (detail) {
    return instances?.find((instance) => instance.id === detail.params.id)?.name ?? t("nav.instances");
  }
  const key = SECTIONS[pathname.split("/")[1] ?? ""];
  return key ? t(`nav.${key}`) : "";
}

/** ロゴの形（favicon と同じ）。色はテーマのトークンに合わせる。 */
function Logo() {
  return (
    <svg aria-hidden viewBox="0 0 32 32" className="size-4 shrink-0">
      <rect x="3.5" y="6.5" width="25" height="19" rx="4.5" fill="none" strokeWidth="3" className="stroke-text-strong" />
      <rect x="7" y="10" width="12" height="12" rx="1.5" className="fill-accent" />
    </svg>
  );
}

/**
 * アプリの上端のバー。左から、ロゴと名前、戻る・進む、今の画面の名前、ドラッグ領域、
 * 差し込みの場所（slot。実行中のゲームの表示が入る）、タスクのボタン、ウィンドウボタン（Windows のみ）。
 */
export function AppBar({
  showWindowControls,
  materialOn = false,
  slot,
}: {
  showWindowControls: boolean;
  materialOn?: boolean;
  slot?: ReactNode;
}) {
  const { t } = useTranslation();
  const nav = useHistoryNav();
  const pageName = usePageName();

  // ダブルクリックでの最大化は、Tauri のドラッグ領域の処理に任せず、ここで行う。
  // 2 回目の mousedown を document まで届けないことで、Tauri 側と二重に切り替わらないようにする。
  // 1 回目の mousedown（移動の開始）は、そのまま Tauri に任せる。
  const onMouseDown = (e: MouseEvent<HTMLElement>) => {
    if (e.button !== 0 || e.detail !== 2) return;
    if (e.target instanceof Element && e.target.closest(INTERACTIVE)) return;
    e.preventDefault();
    e.stopPropagation();
    getCurrentWindow()
      .toggleMaximize()
      .catch(() => {});
  };
  // macOS の Tauri は mouseup で最大化するので、そちらも止める
  const onMouseUp = (e: MouseEvent<HTMLElement>) => {
    if (e.button !== 0 || e.detail !== 2) return;
    if (e.target instanceof Element && e.target.closest(INTERACTIVE)) return;
    e.stopPropagation();
  };

  return (
    <header
      data-tauri-drag-region="deep"
      data-testid="app-bar"
      onMouseDown={onMouseDown}
      onMouseUp={onMouseUp}
      className={`flex h-10 shrink-0 items-center gap-1 pl-3 ${showWindowControls ? "" : "pr-3"} ${materialOn ? "bg-transparent" : "bg-surface-1"}`}
    >
      <Logo />
      <span className="ml-1.5 mr-2 font-display text-sm font-semibold tracking-[-0.01em] text-text-strong">
        {t("appName")}
      </span>
      <Button aria-label={t("appBar.back")} isDisabled={!nav.canGoBack} onPress={nav.back} className={navButton}>
        <IconCaretLeft aria-hidden className="size-4" />
      </Button>
      <Button aria-label={t("appBar.forward")} isDisabled={!nav.canGoForward} onPress={nav.forward} className={navButton}>
        <IconCaretRight aria-hidden className="size-4" />
      </Button>
      <span data-testid="page-name" className="ml-2 min-w-0 truncate text-sm text-text">
        {pageName}
      </span>
      {/* 空きの部分。ここをつかんでウィンドウを動かす */}
      <div data-testid="drag-region" className="h-full min-w-8 flex-1" />
      {slot}
      <TaskButton />
      {showWindowControls && (
        <div className="ml-2 h-full">
          <WindowControls />
        </div>
      )}
    </header>
  );
}
