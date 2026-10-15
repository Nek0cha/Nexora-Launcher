import { useEffect, useId, useRef, useState, type ComponentType, type SVGProps } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlay from "~icons/ph/play-fill";
import IconRetry from "~icons/ph/arrow-clockwise";
import IconStop from "~icons/ph/x";
import IconWarning from "~icons/ph/warning-circle";
import { cancelTask } from "../api/commands";
import { errorKey } from "../features/errors/errorText";
import { announcement, latestDemo, playStateFrom, type PlayState } from "../features/play/playState";
import { useDemoStart } from "../features/play/useDemoStart";
import { useTasks } from "../features/tasks/useTasks";
import i18n from "../i18n";

type IconComponent = ComponentType<SVGProps<SVGSVGElement>>;

/** 完了の表示の間だけ時計を進める（経過時間の表示と、準備完了へ戻すため）。 */
function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(timer);
  }, [active]);
  return now;
}

function formatElapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

const NUMBER_MARK = "\u0000";

/** 「準備中 37%」の数字だけを等幅にし、2 桁分の幅を確保して、桁が変わっても文字が動かないようにする（100% は完了の表示に変わる）。 */
function PreparingLabel({ percent }: { percent: number }) {
  const { t } = useTranslation();
  const [before, after = ""] = t("play.preparingPercent", { percent: NUMBER_MARK }).split(NUMBER_MARK);
  return (
    <span>
      {before}
      <span className="tabular inline-block min-w-[2ch] text-right">{percent}</span>
      {after}
    </span>
  );
}

export function PlayButton() {
  const { t } = useTranslation();
  const tasks = useTasks();
  const snapshot = latestDemo(tasks);
  const now = useNow(snapshot?.state === "succeeded");
  const state = playStateFrom(snapshot, now);
  const { start, starting } = useDemoStart();
  const [message, setMessage] = useState("");
  const [showDetail, setShowDetail] = useState(false);
  const prev = useRef<PlayState>(state);
  const describedBy = useId();
  const detailId = useId();

  useEffect(() => {
    const a = announcement(prev.current, state);
    prev.current = state;
    if (!a) return;
    setMessage(a.key === "play.announce.progress" ? t(a.key, { percent: a.percent }) : t(a.key));
  }, [state, t]);

  const active = state.kind === "preparing" || state.kind === "cancelling";
  const percent = active ? state.percent : 0;
  // 押しても何も起きない状態。isDisabled にするとフォーカスが外れるため、フォーカスは残して押下だけを無視する
  const inert = state.kind === "cancelling" || state.kind === "done";

  const onPress = () => {
    if (state.kind === "preparing") void cancelTask(state.taskId);
    else if (state.kind === "ready" || state.kind === "failed") {
      setShowDetail(false);
      start();
    }
  };

  const name =
    state.kind === "preparing"
      ? t("play.cancel")
      : state.kind === "cancelling"
        ? t("play.cancelling")
        : state.kind === "done"
          ? t("play.playing")
          : state.kind === "failed"
            ? t("play.retry")
            : t("play.play");

  const Icon: IconComponent | null = state.kind === "ready" ? IconPlay : state.kind === "failed" ? IconRetry : null;

  // 進捗の面は準備中だけ見せる。完了では満ちた位置のまま消し、キャンセルと失敗では巻き戻しながら消す
  const fillScale = active ? percent / 100 : state.kind === "done" ? 1 : 0;

  return (
    <div className="flex flex-col items-start gap-3">
      {/* 失敗の説明はボタンの上に出す。画面の下端に揃えているため、下に足すとボタンが押し上げられ、再試行しようとした位置からずれる */}
      {state.kind === "failed" && (
        <div className="flex max-w-xl flex-col gap-2">
          <p className="flex items-center gap-2 text-text">
            <IconWarning aria-hidden className="size-4 shrink-0 text-danger" />
            <span>
              {state.error
                ? t(errorKey(state.error.code, (k) => i18n.exists(k)) as never, {
                    ...(state.error.params as Record<string, unknown>),
                    code: state.error.code,
                  })
                : t("errors:generic" as never, { code: "unknown" })}
            </span>
          </p>
          {state.error?.detail && (
            <Button
              onPress={() => setShowDetail((v) => !v)}
              aria-expanded={showDetail}
              aria-controls={showDetail ? detailId : undefined}
              className="self-start select-none rounded-sm text-sm text-text-muted underline underline-offset-2 data-hovered:text-text focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
            >
              {t("play.details")}
            </Button>
          )}
          {showDetail && state.error?.detail && (
            <pre
              id={detailId}
              className="tabular max-h-48 overflow-auto whitespace-pre-wrap rounded-md bg-surface-1 p-3 text-xs text-text"
            >
              {state.error.detail}
            </pre>
          )}
        </div>
      )}
      <Button
        onPress={onPress}
        isPending={starting}
        aria-disabled={inert || undefined}
        aria-label={name}
        aria-describedby={active ? describedBy : undefined}
        className={[
          "play-press relative h-14 w-72 select-none overflow-hidden rounded-lg font-display text-lg font-semibold",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
          "data-pending:cursor-progress",
          state.kind === "done" ? "bg-surface-3 text-text-strong" : "bg-text-strong text-surface-0",
          state.kind === "ready" || state.kind === "failed" ? "data-hovered:opacity-90" : "",
          inert ? "cursor-default" : "",
        ].join(" ")}
      >
        {({ isHovered, isFocusVisible }) => {
          // 準備中は進捗を見せ、ポインタを乗せるかキーボードでフォーカスしたときだけ、押すと何が起きるか（キャンセル）を見せる
          const revealCancel = state.kind === "preparing" && (isHovered || isFocusVisible);
          return (
            <>
              <span
                aria-hidden
                className="play-fill absolute inset-0 bg-accent"
                style={{ transform: `scaleX(${fillScale})`, opacity: active ? 1 : 0 }}
              />
              <span
                aria-hidden
                className={`relative flex items-center justify-center gap-2 ${active ? "text-accent-ink" : ""}`}
              >
                {revealCancel ? (
                  <>
                    <IconStop className="size-5" />
                    <span>{name}</span>
                  </>
                ) : state.kind === "preparing" ? (
                  <PreparingLabel percent={percent} />
                ) : (
                  <>
                    {Icon && <Icon className="size-5" />}
                    <span>{name}</span>
                    {state.kind === "done" && (
                      <span className="tabular text-sm font-normal text-text-muted">
                        {formatElapsed(now - state.startedAt)}
                      </span>
                    )}
                  </>
                )}
              </span>
            </>
          );
        }}
      </Button>
      {active && (
        <span id={describedBy} className="sr-only">
          {t("play.preparingPercent", { percent })}
        </span>
      )}
      <div aria-live="polite" className="sr-only">
        {message}
      </div>
    </div>
  );
}
