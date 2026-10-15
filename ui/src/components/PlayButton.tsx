import { useMutation } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState, type ComponentType, type SVGProps } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlay from "~icons/ph/play-fill";
import IconRetry from "~icons/ph/arrow-clockwise";
import IconStop from "~icons/ph/x";
import IconWarning from "~icons/ph/warning-circle";
import { cancelTask } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { ErrorPayload } from "../bindings/ErrorPayload";
import { errorKey } from "../features/errors/errorText";
import {
  announcement,
  DONE_DISPLAY_MS,
  latestDemo,
  playStateFrom,
  type PlayState,
} from "../features/play/playState";
import { useDemoStart } from "../features/play/useDemoStart";
import { useTasks } from "../features/tasks/useTasks";
import i18n from "../i18n";

type IconComponent = ComponentType<SVGProps<SVGSVGElement>>;

/**
 * 完了の表示の間（until まで）だけ時計を進める。経過時間の表示と、準備完了へ戻すために使う。
 * until を過ぎたら止まるため、古い完了を読み込んだだけでは時計は動かない。
 */
function useNow(until: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (Date.now() >= until) return;
    let timer = 0;
    const tick = () => {
      const current = Date.now();
      setNow(current);
      if (current >= until) window.clearInterval(timer);
    };
    tick();
    timer = window.setInterval(tick, 500);
    return () => window.clearInterval(timer);
  }, [until]);
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

/** 失敗の説明。タスクの失敗と、開始やキャンセルの呼び出し自体の失敗の両方に使う。 */
function FailureNotice({ error }: { error: ErrorPayload | null }) {
  const { t } = useTranslation();
  const [showDetail, setShowDetail] = useState(false);
  const detailId = useId();
  return (
    <div className="flex max-w-xl flex-col gap-2">
      <p className="flex items-center gap-2 text-danger">
        <IconWarning aria-hidden className="size-4 shrink-0" />
        <span>
          {error
            ? t(errorKey(error.code, (k) => i18n.exists(k)) as never, {
                ...(error.params as Record<string, unknown>),
                code: error.code,
              })
            : t("errors:generic" as never, { code: "unknown" })}
        </span>
      </p>
      {error?.detail && (
        <Button
          onPress={() => setShowDetail((v) => !v)}
          aria-expanded={showDetail}
          aria-controls={showDetail ? detailId : undefined}
          className="self-start select-none rounded-sm text-sm text-text-muted underline underline-offset-2 data-hovered:text-text focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        >
          {t("play.details")}
        </Button>
      )}
      {showDetail && error?.detail && (
        <pre
          id={detailId}
          className="tabular max-h-48 overflow-auto whitespace-pre-wrap rounded-md bg-surface-1 p-3 text-xs text-text"
        >
          {error.detail}
        </pre>
      )}
    </div>
  );
}

export function PlayButton() {
  const { t } = useTranslation();
  const tasks = useTasks();
  const snapshot = latestDemo(tasks);
  const now = useNow(snapshot?.state === "succeeded" ? snapshot.updatedAt + DONE_DISPLAY_MS : 0);
  const state = playStateFrom(snapshot, now);
  const { start, starting, error: startError } = useDemoStart();
  const cancel = useMutation({ mutationFn: (taskId: string) => cancelTask(taskId) });
  // isPending は次の描画まで反映されないため、続けて押された分は印で止める
  const cancelInFlight = useRef(false);
  const [message, setMessage] = useState("");
  const prev = useRef<PlayState>(state);
  const describedBy = useId();

  useEffect(() => {
    const a = announcement(prev.current, state, snapshot?.state);
    prev.current = state;
    if (!a) return;
    setMessage(a.key === "play.announce.progress" ? t(a.key, { percent: a.percent }) : t(a.key));
  }, [state, snapshot?.state, t]);

  // キャンセルを送ってから応答が来るまでも「キャンセルしています」として扱い、重ねて送らない
  const view: PlayState =
    state.kind === "preparing" && cancel.isPending ? { ...state, kind: "cancelling" } : state;
  const active = view.kind === "preparing" || view.kind === "cancelling";
  const percent = active ? view.percent : 0;
  // 押しても何も起きない状態。isDisabled にするとフォーカスが外れるため、フォーカスは残して押下だけを無視する
  const inert = view.kind === "cancelling" || view.kind === "done";

  // 開始の失敗は、押し直せる状態のときだけ出す（別の場所から開始されて動き出したら古い失敗になる）
  const shownStartError = view.kind === "ready" || view.kind === "failed" ? startError : null;
  const cancelError = cancel.error ? toErrorPayload(cancel.error) : null;
  const failure: { error: ErrorPayload | null } | null = shownStartError
    ? { error: shownStartError }
    : cancelError
      ? { error: cancelError }
      : view.kind === "failed"
        ? { error: view.error }
        : null;

  const onPress = () => {
    if (view.kind === "preparing") {
      if (cancelInFlight.current) return;
      cancelInFlight.current = true;
      cancel.mutate(view.taskId, { onSettled: () => (cancelInFlight.current = false) });
    } else if (view.kind === "ready" || view.kind === "failed") {
      cancel.reset();
      start();
    }
  };

  const name =
    view.kind === "preparing"
      ? t("play.cancel")
      : view.kind === "cancelling"
        ? t("play.cancelling")
        : view.kind === "done"
          ? t("play.playing")
          : view.kind === "failed"
            ? t("play.retry")
            : t("play.play");

  const Icon: IconComponent | null = view.kind === "ready" ? IconPlay : view.kind === "failed" ? IconRetry : null;

  // 進捗の面は準備中だけ見せる。完了では満ちた位置のまま消し、キャンセルと失敗では巻き戻しながら消す
  const fillScale = active ? percent / 100 : view.kind === "done" ? 1 : 0;

  return (
    <div className="flex flex-col items-start gap-3">
      {/* 失敗の説明はボタンの上に出す。画面の下端に揃えているため、下に足すとボタンが押し上げられ、再試行しようとした位置からずれる */}
      {failure && <FailureNotice key={failure.error?.code ?? "unknown"} error={failure.error} />}
      <Button
        onPress={onPress}
        isPending={starting}
        aria-disabled={inert || undefined}
        aria-label={name}
        aria-describedby={view.kind === "preparing" ? describedBy : undefined}
        className={[
          "play-press relative h-14 w-72 select-none overflow-hidden rounded-lg font-display text-lg font-semibold",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
          "data-pending:cursor-progress",
          view.kind === "done" ? "bg-surface-3 text-text-strong" : "bg-text-strong text-surface-0",
          view.kind === "ready" || view.kind === "failed" ? "data-hovered:opacity-90" : "",
          inert ? "cursor-default" : "",
        ].join(" ")}
      >
        {({ isHovered, isFocusVisible }) => {
          // 準備中は進捗を見せ、ポインタを乗せるかキーボードでフォーカスしたときだけ、押すと何が起きるか（キャンセル）を見せる
          const revealCancel = view.kind === "preparing" && (isHovered || isFocusVisible);
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
                ) : view.kind === "preparing" ? (
                  <PreparingLabel percent={percent} />
                ) : (
                  <>
                    {Icon && <Icon className="size-5" />}
                    <span>{name}</span>
                    {view.kind === "done" && (
                      <span className="tabular text-sm font-normal text-text-muted">
                        {formatElapsed(now - view.startedAt)}
                      </span>
                    )}
                  </>
                )}
              </span>
            </>
          );
        }}
      </Button>
      {view.kind === "preparing" && (
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
