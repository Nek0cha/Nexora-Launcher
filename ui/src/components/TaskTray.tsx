import { useMutation } from "@tanstack/react-query";
import { Button, Dialog, DialogTrigger, Popover, ProgressBar, Tooltip, TooltipTrigger } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconWarning from "~icons/ph/warning-circle";
import IconX from "~icons/ph/x";
import { cancelTask } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import { errorKey } from "../features/errors/errorText";
import { activeCount, isTerminal, sortedTasks } from "../features/tasks/taskTable";
import { useTasks } from "../features/tasks/useTasks";
import i18n from "../i18n";

const focusRing = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const popoverClass = [
  "w-80 rounded-lg border border-line bg-surface-2 p-2 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "data-entering:opacity-0 data-exiting:opacity-0 data-exiting:duration-(--duration-fast)",
  "data-[placement=top]:data-entering:translate-y-1 data-[placement=top]:data-exiting:translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

const tooltipClass = [
  "select-none rounded-sm border border-line bg-surface-2 px-2 py-1 text-xs text-text shadow-float",
  "transition-[opacity,translate] duration-(--duration-fast) ease-out-quint",
  "data-entering:opacity-0 data-exiting:opacity-0",
  "data-[placement=top]:data-entering:translate-y-1 data-[placement=bottom]:data-entering:-translate-y-1",
  "motion-reduce:transition-opacity",
].join(" ");

function percentOf(task: TaskSnapshot): number {
  const { current, total } = task.progress;
  return total > 0 ? Math.min(100, Math.floor((current * 100) / total)) : 0;
}

function TaskRow({ task }: { task: TaskSnapshot }) {
  const { t } = useTranslation();
  const cancel = useMutation({ mutationFn: (id: string) => cancelTask(id) });
  const done = isTerminal(task);
  const percent = percentOf(task);
  const indeterminate = !done && task.progress.total <= 0;
  const label = t(`tasks.kind.${task.kind}` as never, { defaultValue: task.kind });
  const failure = cancel.error ? toErrorPayload(cancel.error) : null;
  const cancellable = !done && task.state !== "cancelling";

  return (
    <li className="flex flex-col gap-1 rounded-sm px-2 py-2">
      <div className="flex items-center gap-2">
        <ProgressBar
          value={percent}
          isIndeterminate={indeterminate}
          aria-label={label}
          className="flex min-w-0 flex-1 flex-col gap-1"
        >
          <div className="flex justify-between gap-2 text-sm">
            <span className={`truncate ${done ? "text-text-muted" : "text-text"}`}>{label}</span>
            <span className="tabular shrink-0 text-text-muted">
              {done || indeterminate || task.state === "cancelling" ? t(`tasks.state.${task.state}`) : `${percent}%`}
            </span>
          </div>
          {!done && (
            <div className="h-1 overflow-hidden rounded-full bg-surface-3">
              <div
                className="play-fill h-full bg-accent"
                style={{ transform: `scaleX(${indeterminate ? 0 : percent / 100})` }}
              />
            </div>
          )}
        </ProgressBar>
        {cancellable && (
          <TooltipTrigger delay={500}>
            <Button
              aria-label={t("tasks.cancel")}
              aria-disabled={cancel.isPending || undefined}
              onPress={() => {
                if (cancel.isPending) return;
                cancel.reset();
                // 失敗は mutation の error に入り、行の中に出す（未処理の拒否にしない）
                cancel.mutate(task.id);
              }}
              className={`grid size-7 shrink-0 select-none place-items-center rounded-sm text-text-muted outline-none transition-colors duration-(--duration-fast) ease-out-quint data-hovered:bg-surface-3 data-hovered:text-text data-pressed:bg-surface-3 ${focusRing}`}
            >
              <IconX aria-hidden className="size-4" />
            </Button>
            <Tooltip offset={6} className={tooltipClass}>
              {t("tasks.cancel")}
            </Tooltip>
          </TooltipTrigger>
        )}
      </div>
      {failure && (
        <p role="alert" className="flex items-start gap-2 text-sm text-text">
          <IconWarning aria-hidden className="mt-0.5 size-4 shrink-0 text-danger" />
          <span>
            {t(errorKey(failure.code, (k) => i18n.exists(k)) as never, {
              ...(failure.params as Record<string, unknown>),
              code: failure.code,
            })}
          </span>
        </p>
      )}
    </li>
  );
}

export function TaskTray() {
  const { t } = useTranslation();
  const table = useTasks();
  const running = activeCount(table);
  const all = sortedTasks(table);
  const unfinished = all.filter((task) => !isTerminal(task));
  const finished = all.filter(isTerminal).slice(0, 5);

  if (running === 0) return null;

  return (
    <DialogTrigger>
      <Button
        aria-label={`${t("tasks.title")}, ${t("tasks.running", { count: running })}`}
        className={`flex h-10 w-full select-none items-center justify-between rounded-md px-3 text-text-muted outline-none transition-colors duration-(--duration-fast) ease-out-quint data-hovered:bg-surface-2 data-pressed:bg-surface-3 ${focusRing}`}
      >
        <span>{t("tasks.title")}</span>
        <span className="tabular text-text">{running}</span>
      </Button>
      <Popover placement="top start" offset={6} className={popoverClass}>
        <Dialog aria-label={t("tasks.title")} className="outline-hidden">
          <ul className="flex flex-col gap-1">
            {[...unfinished, ...finished].map((task) => (
              <TaskRow key={task.id} task={task} />
            ))}
          </ul>
        </Dialog>
      </Popover>
    </DialogTrigger>
  );
}
