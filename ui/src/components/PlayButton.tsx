import { useMutation } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState, type ComponentType, type SVGProps } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlay from "~icons/ph/play-fill";
import IconRetry from "~icons/ph/arrow-clockwise";
import IconInstall from "~icons/ph/download-simple";
import IconStop from "~icons/ph/x";
import IconStopGame from "~icons/ph/stop-fill";
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
import { useStopFlow } from "../features/play/useStopFlow";
import { phaseKey, phaseLabel, type PhaseLabel, type ProgressView } from "../features/tasks/taskTable";
import { useTasks } from "../features/tasks/useTasks";
import { FailureNotice } from "./FailureNotice";
import { useToast } from "./toast/useToast";

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

/**
 * 「ダウンロード中 37%」「ローダーを構築中 2/5」の数字だけを等幅にし、桁が変わっても文字が動かないようにする。
 * % は 2 桁分の幅を確保する（100% は完了の表示に変わる）。
 * 一覧の行の小さなボタン（small）では、段階の名前と数が 1 行に収まるよう文字を小さくする。
 */
function PreparingLabel({ label, small }: { label: PhaseLabel; small: boolean }) {
  const { t } = useTranslation();
  const names = Object.keys(label.values);
  const marked = Object.fromEntries(names.map((name, i) => [name, `${NUMBER_MARK}${i}${NUMBER_MARK}`]));
  // 印で挟んだ番号のところで分ける。奇数番目が数の番号になる
  const parts = String(t(label.key as never, marked as never)).split(NUMBER_MARK);
  return (
    <span className={`whitespace-nowrap ${small ? "text-xs" : ""}`}>
      {parts.map((part, i) => {
        if (i % 2 === 0) return part;
        const name = names[Number(part)] ?? "";
        return (
          <span key={i} className={`tabular inline-block text-right ${name === "percent" ? "min-w-[2ch]" : ""}`}>
            {label.values[name]}
          </span>
        );
      })}
    </span>
  );
}

/** 段階の無いタスクは、インストールと起動で文言を分けた % で見せる。 */
function labelOf(phase: Phase, progress: ProgressView): PhaseLabel {
  return (
    phaseLabel(progress) ?? {
      key: phase === "install" ? "play.preparingPercent" : "play.launchingPercent",
      values: { percent: progress.percent ?? 0 },
    }
  );
}

// 重ねた層。同じマスに置き、不透明度を既存の長さとイージングで切り替える
const layer =
  "col-start-1 row-start-1 transition-opacity duration-(--duration-fast) ease-out-quint motion-reduce:transition-none";

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
  const stopFlow = useStopFlow();
  const stop = stopFlow.stateOf(instance.id);
  const { press, starting, error: startError, errorId: startErrorId } = play;
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
    setMessage(
      a.key === "play.announce.progress"
        ? t(a.key, { percent: a.percent })
        : a.key === "play.announce.phase"
          ? t(phaseKey(a.phase) as never)
          : t(a.key),
    );
  }, [state, snapshot?.state, t]);

  // キャンセルを送ってから応答が来るまでも「キャンセルしています」として扱い、重ねて送らない
  const view: PlayState =
    state.kind === "preparing" && cancel.isPending ? { ...state, kind: "cancelling" } : state;
  const active = view.kind === "preparing" || view.kind === "cancelling";
  const percent = active ? view.progress.percent : null;
  // 全体の分からない段階（情報の取得、ローダーの構築、仕上げなど）では、バーは動きだけで進んでいることを示す
  const indeterminate = active && percent === null;
  const label = view.kind === "preparing" ? labelOf(view.phase, view.progress) : null;
  // 押しても何も起きない状態。isDisabled にするとフォーカスが外れるため、フォーカスは残して押下だけを無視する
  const inert =
    view.kind === "cancelling" || view.kind === "checking" || (view.kind === "playing" && stop === "waiting");

  // 開始の失敗は、押し直せる状態のときだけ出す（別の場所から開始されて動き出したら古い失敗になる）
  const shownStartError = view.kind === "ready" || view.kind === "needsInstall" || view.kind === "failed" || view.kind === "crashed"
      ? startError
      : null;
  const cancelError = cancel.error ? toErrorPayload(cancel.error) : null;
  // id は失敗ごとに変わる印。一覧の行では、変わったときだけトーストで知らせる
  const failure: { error: ErrorPayload | null; id: string } | null = shownStartError
    ? { error: shownStartError, id: startErrorId ?? "start" }
    : cancelError
      ? { error: cancelError, id: `cancel:${cancel.submittedAt}` }
      // ログインのダイアログで扱う失敗は、ボタンのそばには出さない
      : view.kind === "failed" && !loginPrompt(view.error)
        ? { error: view.error, id: `task:${snapshot?.id}` }
        : view.kind === "crashed"
          ? { error: crashPayload(view.exitCode), id: `exit:${lastExit?.at}` }
          : null;

  // 一覧の行では、失敗をその場に出すと行の高さが変わるため、トーストで出す。
  // 画面を開いた時点ですでにある失敗は知らせ直さない（undefined はまだ見ていない印）
  const toast = useToast();
  const toasted = useRef<string | null | undefined>(undefined);
  const failureId = failure?.id ?? null;
  useEffect(() => {
    if (!row || view.kind === "checking") return;
    if (toasted.current === undefined || toasted.current === failureId) {
      toasted.current = failureId;
      return;
    }
    toasted.current = failureId;
    if (failure) toast.showError(failure.error ?? { code: "unknown", params: {}, detail: null });
  }, [row, failureId, view.kind]);

  // プレイ中に、ポインタを乗せるかキーボードでフォーカスしたとき、赤い停止の表示にする。待っている間は変えない
  const revealsStop = (isHovered: boolean, isFocusVisible: boolean) =>
    view.kind === "playing" && stop !== "waiting" && (isHovered || isFocusVisible);

  const onPress = () => {
    if (view.kind === "playing") {
      // 待っている間は押しても何もしない。確認のダイアログを閉じた後は、押したときだけ強制終了する
      if (stop === "idle") stopFlow.requestStop(instance.id);
      else if (stop === "forceReady") stopFlow.forceStop(instance.id);
    } else if (view.kind === "preparing") {
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
          ? stop === "waiting"
            ? t("play.waitingExit")
            : stop === "forceReady"
              ? t("play.forceStop")
              : t("play.stop")
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
  const fillScale = active ? (percent ?? 0) / 100 : view.kind === "playing" ? 1 : 0;

  return (
    <div className={`flex flex-col gap-3 ${row ? "items-end" : "items-start"}`}>
      {/* 失敗の説明はボタンの上に出す。画面の下端に揃えているため、下に足すとボタンが押し上げられ、再試行しようとした位置からずれる */}
      {failure && !row && <FailureNotice key={failure.error?.code ?? "unknown"} error={failure.error} />}
      <Button
        ref={buttonRef}
        onPress={onPress}
        isPending={starting}
        isDisabled={isDisabled}
        aria-disabled={inert || undefined}
        aria-label={name}
        aria-describedby={view.kind === "preparing" || view.kind === "playing" ? describedBy : undefined}
        className={({ isHovered, isFocusVisible }) =>
          [
            "play-press relative overflow-hidden font-display font-semibold",
            row ? "h-9 w-36 rounded-md text-sm" : "h-14 w-72 rounded-lg text-lg",
            "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
            "data-pending:cursor-progress data-disabled:opacity-50",
            "transition-colors duration-(--duration-fast) ease-out-quint",
            view.kind === "playing"
              ? revealsStop(isHovered, isFocusVisible)
                ? "bg-danger text-danger-ink"
                : "bg-surface-3 text-text-strong"
              : "bg-text-strong text-surface-0",
            view.kind === "playing" || view.kind === "cancelling" ? "" : "data-hovered:opacity-90",
            inert ? "cursor-default" : "",
          ].join(" ")
        }
      >
        {({ isHovered, isFocusVisible }) => {
          // 準備中は進捗を見せ、ポインタを乗せるかキーボードでフォーカスしたときだけ、押すと何が起きるか（キャンセル）を見せる
          const revealCancel = view.kind === "preparing" && (isHovered || isFocusVisible);
          // プレイ中も同じく、乗せたときだけ赤い「停止」（確認の後は「強制終了」）を見せる
          const revealStop = revealsStop(isHovered, isFocusVisible);
          return (
            <>
              <span
                aria-hidden
                data-testid="play-fill"
                data-indeterminate={indeterminate || undefined}
                className="play-fill absolute inset-0 bg-accent"
                style={{ transform: `scaleX(${indeterminate ? 0 : fillScale})`, opacity: active ? 1 : 0 }}
              />
              {indeterminate && <span aria-hidden className="play-indeterminate absolute inset-y-0 left-0 bg-accent" />}
              <span
                aria-hidden
                className={`relative flex items-center justify-center gap-2 ${active ? "text-accent-ink" : ""}`}
              >
                {revealCancel ? (
                  <>
                    <IconStop className={row ? "size-4" : "size-5"} />
                    <span>{name}</span>
                  </>
                ) : view.kind === "playing" ? (
                  // プレイ中は「プレイ中」と赤い「停止」を同じ場所に重ね、不透明度だけを切り替えて交差させる（面の色は親の transition で変わる）
                  <span className="grid place-items-center">
                    <span
                      data-testid="play-layer-playing"
                      data-active={!revealStop || undefined}
                      className={`${layer} flex items-center justify-center gap-2 ${revealStop ? "opacity-0" : ""}`}
                    >
                      {stop !== "idle" ? (
                        <span className={row ? "text-xs" : "text-base"}>{t("play.waitingExit")}</span>
                      ) : (
                        <>
                          <span>{t("play.playing")}</span>
                          <span className={`tabular font-normal text-text-muted ${row ? "text-xs" : "text-sm"}`}>
                            {formatElapsed(now - view.launchedAt)}
                          </span>
                        </>
                      )}
                    </span>
                    {/* 待っている間は停止の層を出さない（名前が重複するため。待っている間は赤くならない） */}
                    {stop !== "waiting" && (
                      <span
                        data-testid="play-layer-stop"
                        data-active={revealStop || undefined}
                        className={`${layer} flex items-center justify-center gap-2 ${revealStop ? "" : "opacity-0"}`}
                      >
                        <IconStopGame className={row ? "size-4" : "size-5"} />
                        <span>{name}</span>
                      </span>
                    )}
                  </span>
                ) : label ? (
                  <PreparingLabel label={label} small={row} />
                ) : (
                  <>
                    {Icon && <Icon className={row ? "size-4" : "size-5"} />}
                    <span>{name}</span>
                  </>
                )}
              </span>
            </>
          );
        }}
      </Button>
      {label && (
        <span id={describedBy} className="sr-only">
          {t(label.key as never, label.values)}
        </span>
      )}
      {view.kind === "playing" && (
        <span id={describedBy} className="sr-only">
          {t("play.playing")}
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
