import { useMutation } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState, type ComponentType, type SVGProps } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlay from "~icons/ph/play-fill";
import IconRetry from "~icons/ph/arrow-clockwise";
import IconInstall from "~icons/ph/download-simple";
import IconStop from "~icons/ph/x";
import { cancelTask } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { ErrorPayload } from "../bindings/ErrorPayload";
import type { Instance } from "../bindings/Instance";
import {
  announcement,
  crashPayload,
  latestInstanceTask,
  playStateFrom,
  type Phase,
  type PlayState,
} from "../features/play/playState";
import { PlayLoginDialog, usePlayDialogHost } from "../features/login/LoginDialog";
import { loginPrompt } from "../features/login/loginState";
import { useLastExits, useRunning } from "../features/play/useGameSync";
import { usePlay } from "../features/play/usePlay";
import { useTasks } from "../features/tasks/useTasks";
import { FailureNotice } from "./FailureNotice";

type IconComponent = ComponentType<SVGProps<SVGSVGElement>>;

/**
 * until まで時計を進める。プレイ中の経過時間の表示に使う（プレイ中は until を無限大にする）。
 * until を過ぎたら止まる。
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
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

const NUMBER_MARK = "\u0000";

/** 「準備中 37%」の数字だけを等幅にし、2 桁分の幅を確保して、桁が変わっても文字が動かないようにする（100% は完了の表示に変わる）。 */
function PreparingLabel({ percent, labelKey }: { percent: number; labelKey: "play.preparingPercent" | "play.launchingPercent" }) {
  const { t } = useTranslation();
  const [before, after = ""] = t(labelKey, { percent: NUMBER_MARK }).split(NUMBER_MARK);
  return (
    <span>
      {before}
      <span className="tabular inline-block min-w-[2ch] text-right">{percent}</span>
      {after}
    </span>
  );
}

const phaseLabelKey = (phase: Phase) => (phase === "install" ? "play.preparingPercent" : "play.launchingPercent");

export function PlayButton({
  instance,
  size = "hero",
  isDisabled = false,
}: {
  instance: Instance;
  /** hero はホームの大きなボタン、row は一覧の行に収まる小さなボタン */
  size?: "hero" | "row";
  /** 複製の最中など、このインスタンスを操作できない間に押せなくする */
  isDisabled?: boolean;
}) {
  const row = size === "row";
  const { t } = useTranslation();
  const tasks = useTasks();
  const snapshot = latestInstanceTask(tasks, instance.id);
  const sessions = useRunning();
  const running = sessions?.find((r) => r.instanceId === instance.id);
  const lastExit = useLastExits()[instance.id];
  const state = playStateFrom({ instance, task: snapshot, running, runningLoaded: sessions !== undefined, lastExit });
  const now = useNow(state.kind === "playing" ? Number.POSITIVE_INFINITY : 0);
  const play = usePlay(instance.id);
  const { press, starting, error: startError } = play;
  // ログインのダイアログはここに置く（ホームが出ている間は、コマンドパレットからの起動の分も）
  usePlayDialogHost(instance.id);
  const buttonRef = useRef<HTMLButtonElement>(null);
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
  const inert = view.kind === "cancelling" || view.kind === "playing" || view.kind === "checking";

  // 開始の失敗は、押し直せる状態のときだけ出す（別の場所から開始されて動き出したら古い失敗になる）
  const shownStartError = view.kind === "ready" || view.kind === "needsInstall" || view.kind === "failed" || view.kind === "crashed"
      ? startError
      : null;
  const cancelError = cancel.error ? toErrorPayload(cancel.error) : null;
  const failure: { error: ErrorPayload | null } | null = shownStartError
    ? { error: shownStartError }
    : cancelError
      ? { error: cancelError }
      // ログインのダイアログで扱う失敗は、ボタンのそばには出さない
      : view.kind === "failed" && !loginPrompt(view.error)
        ? { error: view.error }
        : view.kind === "crashed"
          ? { error: crashPayload(view.exitCode) }
          : null;

  const onPress = () => {
    if (view.kind === "preparing") {
      if (cancelInFlight.current) return;
      cancelInFlight.current = true;
      cancel.mutate(view.taskId, { onSettled: () => (cancelInFlight.current = false) });
    } else if (
      view.kind === "ready" || view.kind === "needsInstall" || view.kind === "failed" || view.kind === "crashed"
    ) {
      cancel.reset();
      press();
    }
  };

  const name =
    view.kind === "preparing"
      ? t("play.cancel")
      : view.kind === "cancelling"
        ? t("play.cancelling")
        : view.kind === "playing"
          ? t("play.playing")
          : view.kind === "needsInstall"
            ? t("play.install")
            : view.kind === "failed"
              ? t("play.retry")
              : t("play.play");

  const Icon: IconComponent | null = view.kind === "ready" || view.kind === "crashed"
      ? IconPlay
      : view.kind === "needsInstall"
        ? IconInstall
        : view.kind === "failed"
          ? IconRetry
          : null;

  // 進捗の面は準備中だけ見せる。完了では満ちた位置のまま消し、キャンセルと失敗では巻き戻しながら消す
  const fillScale = active ? percent / 100 : view.kind === "playing" ? 1 : 0;

  return (
    <div className={`flex flex-col gap-3 ${row ? "items-end" : "items-start"}`}>
      {/* 失敗の説明はボタンの上に出す。画面の下端に揃えているため、下に足すとボタンが押し上げられ、再試行しようとした位置からずれる */}
      {failure && <FailureNotice key={failure.error?.code ?? "unknown"} error={failure.error} />}
      <Button
        ref={buttonRef}
        onPress={onPress}
        isPending={starting}
        isDisabled={isDisabled}
        aria-disabled={inert || undefined}
        aria-label={name}
        aria-describedby={view.kind === "preparing" ? describedBy : undefined}
        className={[
          "play-press relative overflow-hidden font-display font-semibold",
          row ? "h-9 w-36 rounded-md text-sm" : "h-14 w-72 rounded-lg text-lg",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
          "data-pending:cursor-progress data-disabled:opacity-50",
          view.kind === "playing" ? "bg-surface-3 text-text-strong" : "bg-text-strong text-surface-0",
          view.kind === "playing" || view.kind === "cancelling" ? "" : "data-hovered:opacity-90",
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
                    <IconStop className={row ? "size-4" : "size-5"} />
                    <span>{name}</span>
                  </>
                ) : view.kind === "preparing" ? (
                  <PreparingLabel percent={percent} labelKey={phaseLabelKey(view.phase)} />
                ) : (
                  <>
                    {Icon && <Icon className={row ? "size-4" : "size-5"} />}
                    <span>{name}</span>
                    {view.kind === "playing" && (
                      <span className={`tabular font-normal text-text-muted ${row ? "text-xs" : "text-sm"}`}>
                        {formatElapsed(now - view.launchedAt)}
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
          {t(phaseLabelKey(view.phase), { percent })}
        </span>
      )}
      <div aria-live="polite" className="sr-only">
        {message}
      </div>
      <PlayLoginDialog
        instanceId={instance.id}
        play={play}
        // 閉じたらフォーカスをプレイボタンに戻す。閉じる動きに入ってフォーカスの閉じ込めが解けてから移す
        onClosed={() => window.setTimeout(() => buttonRef.current?.focus(), 0)}
      />
    </div>
  );
}
