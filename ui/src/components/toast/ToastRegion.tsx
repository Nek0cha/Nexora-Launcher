import { AnimatePresence, motion, useIsPresent } from "motion/react";
import { type FocusEvent, useCallback, useEffect, useRef, useState } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconSuccess from "~icons/ph/check-circle";
import IconInfo from "~icons/ph/info";
import IconError from "~icons/ph/warning-circle";
import IconClose from "~icons/ph/x";
import { dismissToast, TOAST_TIMEOUT_MS, type ToastItem, useToastList } from "./useToast";

// tokens.css の --ease-out-quint と --duration-slow / --duration-base に合わせる
const EASE_OUT_QUINT = [0.22, 1, 0.36, 1] as const;
const enter = { duration: 0.24, ease: EASE_OUT_QUINT };
const leave = { duration: 0.18, ease: EASE_OUT_QUINT };

const icons = {
  success: { Icon: IconSuccess, className: "text-success" },
  info: { Icon: IconInfo, className: "text-text-muted" },
  error: { Icon: IconError, className: "text-danger" },
} as const;

/**
 * 画面の右下のトースト。React Aria の UNSTABLE_Toast は、すべてを role="alert" で読み上げ、
 * 上限を超えた分を消さずに待たせるため、ここでは自作する。
 * 失敗は role="alert" で読み上げる。成功と情報は、後から足した要素の role="status" では
 * 読み上げられないことがあるため、いつも置いてある 1 つの読み上げ用の領域に文を差し込む。
 */
export function ToastRegion() {
  const { t } = useTranslation();
  const toasts = useToastList();
  const listRef = useRef<HTMLOListElement>(null);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  // 乗せている間とフォーカスがある間は、すべてのトーストの時計を止める（並びが手の下で動かないように）
  const paused = (hovered || focused) && toasts.length > 0;

  // フォーカスのある要素が消えると blur が届かないため、並びが変わるたびに DOM から決め直す
  const resync = useCallback(() => {
    const list = listRef.current;
    if (!list) return;
    let hover = false;
    try {
      hover = list.matches(":hover");
    } catch {
      hover = false;
    }
    setHovered(hover);
    // 消える動きの途中のトーストにあるフォーカスは数えない（閉じるボタンを押した直後など）
    const active = document.activeElement;
    setFocused(Boolean(active && list.contains(active) && !active.closest("[data-exiting]")));
  }, []);

  useEffect(resync, [toasts, resync]);

  const announcement = useAnnouncement(toasts);

  const onBlur = (e: FocusEvent<HTMLOListElement>) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFocused(false);
  };

  return (
    <section aria-label={t("toast.region")} className="pointer-events-none fixed right-4 bottom-4 z-50 w-88 max-w-[calc(100vw-2rem)]">
      {/* 成功と情報の読み上げ用。中身が入れ替わったときに読み上げられるよう、いつも置いておく */}
      <div role="status" aria-live="polite" aria-atomic="true" className="sr-only">
        {announcement && <p key={announcement.id}>{announcement.message}</p>}
      </div>
      <ol
        ref={listRef}
        className="flex flex-col items-stretch gap-2"
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        onFocus={() => setFocused(true)}
        onBlur={onBlur}
      >
        {/* 消える動きが終わって要素が無くなったときも決め直す */}
        <AnimatePresence initial={false} onExitComplete={resync}>
          {toasts.map((toast) => (
            <Toast key={toast.id} toast={toast} paused={paused} />
          ))}
        </AnimatePresence>
      </ol>
    </section>
  );
}

/** 読み上げる成功と情報のトースト。新しいものが来たら差し替え、消えたら空にする。 */
function useAnnouncement(toasts: ToastItem[]): ToastItem | null {
  const [current, setCurrent] = useState<ToastItem | null>(null);
  const lastId = useRef(0);
  useEffect(() => {
    const fresh = toasts.filter((toast) => toast.kind !== "error" && toast.id > lastId.current).at(-1);
    if (fresh) {
      lastId.current = fresh.id;
      setCurrent(fresh);
    } else if (current && !toasts.some((toast) => toast.id === current.id)) {
      // 閉じたトーストの文が、読み上げ用の領域に残らないようにする
      setCurrent(null);
    }
  }, [toasts]);
  return current;
}

function Toast({ toast, paused }: { toast: ToastItem; paused: boolean }) {
  const { t } = useTranslation();
  const remaining = useRef(TOAST_TIMEOUT_MS[toast.kind]);
  const { Icon, className } = icons[toast.kind];
  // 消える動きの間は、読み上げと操作の対象から外す
  const present = useIsPresent();

  useEffect(() => {
    if (paused) return;
    const startedAt = Date.now();
    const timer = window.setTimeout(() => dismissToast(toast.id), remaining.current);
    return () => {
      window.clearTimeout(timer);
      remaining.current = Math.max(0, remaining.current - (Date.now() - startedAt));
    };
  }, [paused, toast.id]);

  return (
    <motion.li
      layout
      initial={{ opacity: 0, y: 12, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1, transition: enter }}
      exit={{ opacity: 0, x: 24, transition: leave }}
      transition={enter}
      data-exiting={present ? undefined : ""}
      aria-hidden={present ? undefined : true}
      inert={!present}
      className="pointer-events-auto flex items-start gap-3 rounded-lg border border-line bg-surface-2 py-3 pr-2 pl-3 shadow-float"
    >
      {/* 成功と情報は、読み上げ用の領域で読み上げる */}
      <div role={toast.kind === "error" ? "alert" : undefined} className="flex min-w-0 flex-1 items-start gap-2.5">
        <Icon aria-hidden className={`mt-0.5 size-4 shrink-0 ${className}`} />
        <p className="min-w-0 flex-1 text-sm text-text-strong select-text">{toast.message}</p>
      </div>
      <Button
        aria-label={t("toast.close")}
        onPress={() => dismissToast(toast.id)}
        className={[
          "-my-1 inline-flex size-7 shrink-0 items-center justify-center rounded-md text-text-muted",
          "transition-colors duration-(--duration-fast) ease-out-quint",
          "hover:bg-surface-3 hover:text-text pressed:bg-surface-3",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
        ].join(" ")}
      >
        <IconClose aria-hidden className="size-4" />
      </Button>
    </motion.li>
  );
}
