import { Button, Dialog, DialogTrigger, Popover } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconDownload from "~icons/ph/download-simple";
import { activeCount, isTerminal } from "../../features/tasks/taskTable";
import { useTasks } from "../../features/tasks/useTasks";
import { percentOf, TaskList } from "../TaskList";

const popoverClass = [
  "scroll-thin max-h-[28rem] w-80 overflow-auto rounded-lg border border-line bg-surface-2 p-2 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "data-entering:opacity-0 data-exiting:opacity-0 data-exiting:duration-(--duration-fast)",
  "motion-safe:data-[placement=bottom]:data-entering:-translate-y-1 motion-safe:data-[placement=bottom]:data-exiting:-translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

// ボタンは 32px（size-8）。viewBox も 32 にして 1 単位 = 1px とし、輪の線が画素に揃うようにする。
// 輪は半径 13・線幅 2 で外縁が 14。アイコンは 16px（Phosphor は 16px で線が 1px になる）で、輪とは同心で触れない。
const RADIUS = 13;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/**
 * 進行中のタスクの進み具合の輪。進み具合のわかるタスクの平均を描き、
 * どれもわからないときは、一部だけの弧を回して「動いている」ことだけを示す。
 */
function ProgressRing({ percent }: { percent: number | null }) {
  const indeterminate = percent === null;
  const filled = indeterminate ? 0.25 : percent / 100;
  return (
    <svg
      aria-hidden
      data-testid="task-ring"
      viewBox="0 0 32 32"
      className={`absolute inset-0 size-full -rotate-90 ${indeterminate ? "motion-safe:animate-spin" : ""}`}
    >
      <circle cx="16" cy="16" r={RADIUS} fill="none" strokeWidth="2" className="stroke-surface-3" />
      <circle
        cx="16"
        cy="16"
        r={RADIUS}
        fill="none"
        strokeWidth="2"
        strokeLinecap="round"
        strokeDasharray={CIRCUMFERENCE}
        strokeDashoffset={CIRCUMFERENCE * (1 - filled)}
        className="stroke-accent transition-[stroke-dashoffset] duration-(--duration-base) ease-out-quint"
      />
    </svg>
  );
}

/** タスクの丸いボタン。進行中のタスクがあるときだけ進捗の輪を出し、押すと一覧を開く。 */
export function TaskButton() {
  const { t } = useTranslation();
  const tasks = useTasks();
  const count = activeCount(tasks);
  const known = Object.values(tasks).filter((task) => !isTerminal(task) && task.progress.total > 0);
  const percent =
    known.length > 0 ? Math.floor(known.reduce((sum, task) => sum + percentOf(task), 0) / known.length) : null;
  const label = count > 0 ? t("tasks.runningLabel", { count }) : t("tasks.title");

  return (
    <DialogTrigger>
      <Button
        aria-label={label}
        className={[
          "relative grid size-8 shrink-0 place-items-center rounded-full",
          count > 0 ? "text-text" : "text-text-muted",
          "transition-colors duration-(--duration-fast) ease-out-quint data-hovered:bg-surface-2 data-hovered:text-text data-pressed:bg-surface-3",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
        ].join(" ")}
      >
        {count > 0 && <ProgressRing percent={percent} />}
        <IconDownload aria-hidden className="size-4" />
      </Button>
      <Popover placement="bottom end" offset={6} className={popoverClass}>
        <Dialog aria-label={t("tasks.title")} className="outline-hidden">
          <TaskList />
        </Dialog>
      </Popover>
    </DialogTrigger>
  );
}
