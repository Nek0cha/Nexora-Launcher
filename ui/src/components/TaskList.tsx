import { useMutation } from "@tanstack/react-query";
import { Button, ProgressBar, Tooltip, TooltipTrigger } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconWarning from "~icons/ph/warning-circle";
import IconX from "~icons/ph/x";
import { cancelTask } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import { errorKey } from "../features/errors/errorText";
import { isTerminal, phaseLabel, progressView, sortedTasks } from "../features/tasks/taskTable";
import { useTasks } from "../features/tasks/useTasks";
import i18n from "../i18n";
import { tooltipClass } from "./tooltip";

const focusRing = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

function TaskRow({ task }: { task: TaskSnapshot }) {
  const { t } = useTranslation();
  const cancel = useMutation({ mutationFn: (id: string) => cancelTask(id) });
  const done = isTerminal(task);
  const view = progressView(task);
  const percent = view.percent ?? 0;
  // 全体の分からない段階（と、進捗の無いタスク）は、バーで進み具合を示さない
  const indeterminate = !done && view.percent === null;
  // 段階のあるタスクは、プレイボタンと同じ文言で「何をしているか」を出す
  const phase = phaseLabel(view);
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
              {done || task.state === "cancelling"
                ? t(`tasks.state.${task.state}`)
                : phase
                  ? String(t(phase.key as never, phase.values as never))
                  : indeterminate
                    ? t(`tasks.state.${task.state}`)
                    : `${percent}%`}
            </span>
          </div>
          {!done && (
            <div className="relative h-1 overflow-hidden rounded-full bg-surface-3">
              <div
                className="play-fill h-full bg-accent"
                style={{ transform: `scaleX(${indeterminate ? 0 : percent / 100})` }}
              />
              {indeterminate && <div className="play-indeterminate absolute inset-y-0 left-0 bg-accent" />}
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
              className={`grid size-7 shrink-0 place-items-center rounded-sm text-text-muted transition-colors duration-(--duration-fast) ease-out-quint data-hovered:bg-surface-3 data-hovered:text-text data-pressed:bg-surface-3 ${focusRing}`}
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
          <span className="select-text">
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

/**
 * タスクの一覧。AppBar のタスクのボタンが開くポップアップの中身として使う。
 * 進行中のものを先に、終わったものは新しい 5 件まで並べる。
 */
export function TaskList() {
  const { t } = useTranslation();
  const all = sortedTasks(useTasks());
  const unfinished = all.filter((task) => !isTerminal(task));
  const finished = all.filter(isTerminal).slice(0, 5);
  const rows = [...unfinished, ...finished];

  if (rows.length === 0) return <p className="px-2 py-2 text-sm text-text-muted">{t("tasks.empty")}</p>;

  return (
    <ul className="flex flex-col gap-1">
      {rows.map((task) => (
        <TaskRow key={task.id} task={task} />
      ))}
    </ul>
  );
}
