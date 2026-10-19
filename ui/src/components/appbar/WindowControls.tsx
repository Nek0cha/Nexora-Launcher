import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCopy from "~icons/ph/copy";
import IconMinus from "~icons/ph/minus";
import IconSquare from "~icons/ph/square";
import IconX from "~icons/ph/x";

const buttonBase = [
  // Windows の標準のキャプションボタンと同じ幅（46px）で、バーの高さいっぱいに置く
  "grid h-full w-[46px] place-items-center text-text-muted outline-hidden",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent",
].join(" ");

const neutral = `${buttonBase} data-hovered:bg-surface-3 data-hovered:text-text-strong data-pressed:bg-surface-2`;
// 閉じるだけは、OS と同じく、マウスを乗せると危険の色になる
const danger = `${buttonBase} data-hovered:bg-danger data-hovered:text-danger-ink data-pressed:bg-danger data-pressed:opacity-80`;

/** 最大化しているか。サイズの変化（最大化、元に戻す、スナップ）のたびに問い合わせ直す。 */
export function useMaximized(): boolean {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const refresh = () => {
      win.isMaximized().then(
        (value) => {
          if (!disposed) setMaximized(value);
        },
        () => {},
      );
    };
    refresh();
    win.onResized(refresh).then(
      (stop) => {
        if (disposed) stop();
        else unlisten = stop;
      },
      () => {},
    );
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return maximized;
}

/** ウィンドウの操作の失敗は、画面に出しても利用者にできることがないので捨てる */
function run(action: () => Promise<void>): void {
  action().catch(() => {});
}

/** 最小化、最大化と元に戻す、閉じる（Windows だけで使う）。 */
export function WindowControls() {
  const { t } = useTranslation();
  const maximized = useMaximized();
  const win = getCurrentWindow();
  const toggleLabel = maximized ? t("appBar.restore") : t("appBar.maximize");

  return (
    <div className="flex h-full shrink-0">
      <Button aria-label={t("appBar.minimize")} onPress={() => run(() => win.minimize())} className={neutral}>
        <IconMinus aria-hidden className="size-4" />
      </Button>
      <Button aria-label={toggleLabel} onPress={() => run(() => win.toggleMaximize())} className={neutral}>
        {maximized ? (
          <IconCopy aria-hidden data-testid="restore-icon" className="size-3.5 -scale-x-100" />
        ) : (
          <IconSquare aria-hidden data-testid="maximize-icon" className="size-3.5" />
        )}
      </Button>
      <Button aria-label={t("appBar.close")} onPress={() => run(() => win.close())} className={danger}>
        <IconX aria-hidden className="size-4" />
      </Button>
    </div>
  );
}
