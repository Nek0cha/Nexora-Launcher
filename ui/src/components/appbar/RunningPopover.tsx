import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconLog from "~icons/ph/file-text";
import IconStop from "~icons/ph/stop-fill";
import { openInstanceFolder } from "../../api/commands";
import type { StopFlow, StopState } from "../../features/play/useStopFlow";
import { useToast } from "../toast/useToast";

export type RunningEntry = { id: string; name: string; stop: StopState };

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

/** 動いているゲームの印。色はトークンの success。 */
export function RunningDot({ className = "" }: { className?: string }) {
  return <span aria-hidden className={`block size-2 shrink-0 rounded-full bg-success ${className}`} />;
}

/**
 * 停止のボタン（赤）。待っている間は「終了を待っています」と出して押下を無視し、
 * 確認のダイアログを閉じた後（forceReady）は「強制終了」に変わる。
 */
export function StopButton({ entry, flow }: { entry: RunningEntry; flow: StopFlow }) {
  const { t } = useTranslation();
  if (entry.stop === "waiting") {
    return (
      <span className="inline-flex h-6 shrink-0 items-center px-2 text-xs whitespace-nowrap text-text-muted">
        {t("play.waitingExit")}
      </span>
    );
  }
  const force = entry.stop === "forceReady";
  const label = force ? t("play.forceStop") : t("play.stop");
  return (
    <Button
      aria-label={force ? t("running.forceStop", { name: entry.name }) : t("running.stop", { name: entry.name })}
      onPress={() => (force ? flow.forceStop(entry.id) : flow.requestStop(entry.id))}
      className={[
        "inline-flex h-6 shrink-0 items-center gap-1 rounded-full px-2 text-xs font-medium whitespace-nowrap text-danger",
        "transition-colors duration-(--duration-fast) ease-out-quint",
        "data-hovered:bg-danger data-hovered:text-danger-ink data-pressed:bg-danger data-pressed:text-danger-ink",
        ring,
      ].join(" ")}
    >
      <IconStop aria-hidden className="size-3" />
      <span>{label}</span>
    </Button>
  );
}

/**
 * ログを開くボタン。ゲームのログは、ゲームのフォルダの logs に Minecraft が書くため、
 * ゲームのフォルダを開く（ログだけのフォルダを開く command はまだない）。
 */
export function LogButton({ entry }: { entry: RunningEntry }) {
  const { t } = useTranslation();
  const toast = useToast();
  return (
    <Button
      aria-label={t("running.openLog", { name: entry.name })}
      onPress={() => {
        openInstanceFolder(entry.id, "game").catch(toast.showError);
      }}
      className={[
        "grid size-6 shrink-0 place-items-center rounded-full text-text-muted",
        "transition-colors duration-(--duration-fast) ease-out-quint",
        "data-hovered:bg-surface-3 data-hovered:text-text",
        ring,
      ].join(" ")}
    >
      <IconLog aria-hidden className="size-3.5" />
    </Button>
  );
}

/** ピルから開く一覧。各行に名前（押すと詳細へ）、停止、ログを開くボタンを置く。 */
export function RunningPopover({
  entries,
  flow,
  onNavigate,
}: {
  entries: RunningEntry[];
  flow: StopFlow;
  onNavigate: (instanceId: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <ul className="flex flex-col gap-0.5">
      {entries.map((entry) => (
        <li key={entry.id} className="flex items-center gap-1 rounded-md pr-1">
          <Button
            aria-label={t("running.openDetail", { name: entry.name })}
            onPress={() => onNavigate(entry.id)}
            className={[
              "flex h-8 min-w-0 flex-1 items-center gap-2 rounded-md px-2 text-left text-sm text-text",
              "transition-colors duration-(--duration-fast) ease-out-quint data-hovered:bg-surface-3",
              ring,
            ].join(" ")}
          >
            <RunningDot />
            <span className="truncate">{entry.name}</span>
          </Button>
          <StopButton entry={entry} flow={flow} />
          <LogButton entry={entry} />
        </li>
      ))}
    </ul>
  );
}
