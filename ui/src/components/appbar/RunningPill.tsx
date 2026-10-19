import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { Button, Dialog, DialogTrigger, Popover } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { useInstances } from "../../features/instances/useInstances";
import { useRunning } from "../../features/play/useGameSync";
import { useStopFlow } from "../../features/play/useStopFlow";
import { ForceStopDialog } from "../ForceStopDialog";
import { LogButton, RunningDot, RunningPopover, StopButton, type RunningEntry } from "./RunningPopover";

/** 並べる点の数の上限。超えた分は「+N」 */
export const MAX_DOTS = 3;

// tokens.css の --ease-out-quint と --duration-base / --duration-fast に合わせる
const EASE_OUT_QUINT = [0.22, 1, 0.36, 1] as const;
const enter = { duration: 0.18, ease: EASE_OUT_QUINT };
const leave = { duration: 0.12, ease: EASE_OUT_QUINT };

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const popoverClass = [
  "scroll-thin max-h-[28rem] w-72 overflow-auto rounded-lg border border-line bg-surface-2 p-1.5 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "data-entering:opacity-0 data-exiting:opacity-0 data-exiting:duration-(--duration-fast)",
  "motion-safe:data-[placement=bottom]:data-entering:-translate-y-1 motion-safe:data-[placement=bottom]:data-exiting:-translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

/**
 * 動いているゲームと、停止を頼んで終了を待っているインスタンスのピル（spec 2.2）。
 * 1 件なら名前、停止、ログを開くボタンを並べ、2 件以上なら点を並べる。押すと一覧を開く。
 * 0 件なら何も出さない。強制終了の確認のダイアログもここに置く（ピルが消えていても開ける）。
 */
export function RunningPill() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const sessions = useRunning() ?? [];
  const { data: instances } = useInstances();
  const flow = useStopFlow();
  const [open, setOpen] = useState(false);

  // 動いている順に並べ、一覧から消えても終了を待っているものは後ろに残す
  const ids = [...sessions.map((s) => s.instanceId)];
  for (const id of flow.stopping) if (!ids.includes(id)) ids.push(id);
  const entries: RunningEntry[] = ids.map((id) => ({
    id,
    name: instances?.find((instance) => instance.id === id)?.name ?? id,
    stop: flow.stateOf(id),
  }));
  const single = entries.length === 1 ? entries[0] : undefined;
  const anyWaiting = entries.some((entry) => entry.stop === "waiting");
  const hidden = entries.length - MAX_DOTS;

  return (
    <>
      <AnimatePresence initial={false}>
        {entries.length > 0 && (
          <motion.div
            key="running-pill"
            data-testid="running-pill"
            layout
            initial={{ opacity: 0, scale: 0.96 }}
            animate={{ opacity: 1, scale: 1, transition: enter }}
            exit={{ opacity: 0, scale: 0.96, transition: leave }}
            className="mr-1 flex h-7 shrink-0 items-center gap-0.5 rounded-full border border-line bg-surface-2 pr-0.5"
          >
            <DialogTrigger isOpen={open} onOpenChange={setOpen}>
              <Button
                aria-label={single ? undefined : t("running.label", { count: entries.length })}
                className={[
                  "flex h-full min-w-0 items-center gap-1.5 rounded-full pr-1.5 pl-2.5 text-xs text-text",
                  "transition-colors duration-(--duration-fast) ease-out-quint data-hovered:bg-surface-3",
                  ring,
                ].join(" ")}
              >
                {single ? (
                  <>
                    <RunningDot />
                    <span className="max-w-40 truncate">{single.name}</span>
                  </>
                ) : (
                  <>
                    <span className="flex items-center gap-1">
                      <AnimatePresence initial={false}>
                        {entries.slice(0, MAX_DOTS).map((entry) => (
                          <motion.span
                            key={entry.id}
                            data-testid="running-dot"
                            layout
                            initial={{ opacity: 0, scale: 0 }}
                            animate={{ opacity: 1, scale: 1, transition: enter }}
                            exit={{ opacity: 0, scale: 0, transition: leave }}
                          >
                            <RunningDot />
                          </motion.span>
                        ))}
                      </AnimatePresence>
                    </span>
                    {hidden > 0 && (
                      <span className="tabular text-text-muted">{t("running.more", { count: hidden })}</span>
                    )}
                    {anyWaiting && <span className="whitespace-nowrap text-text-muted">{t("play.waitingExit")}</span>}
                  </>
                )}
              </Button>
              <Popover placement="bottom end" offset={6} className={popoverClass}>
                <Dialog aria-label={t("running.title")} className="outline-hidden">
                  <RunningPopover
                    entries={entries}
                    flow={flow}
                    onNavigate={(id) => {
                      setOpen(false);
                      navigate(`/instances/${id}`);
                    }}
                  />
                </Dialog>
              </Popover>
            </DialogTrigger>
            {single && (
              <>
                <StopButton entry={single} flow={flow} />
                <LogButton entry={single} />
              </>
            )}
          </motion.div>
        )}
      </AnimatePresence>
      <ForceStopDialog />
    </>
  );
}
