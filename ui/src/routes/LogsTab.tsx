import { useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  Button,
  Input,
  type Key,
  SearchField,
  ToggleButton,
  ToggleButtonGroup,
  Tooltip,
  TooltipTrigger,
} from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCopy from "~icons/ph/copy";
import IconFolder from "~icons/ph/folder-open";
import IconLog from "~icons/ph/file-text";
import IconSearch from "~icons/ph/magnifying-glass";
import { openInstanceFolder } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import { EmptyState } from "../components/EmptyState";
import { FailureNotice } from "../components/FailureNotice";
import { Select } from "../components/Select";
import { inputBase } from "../components/settingsLayout";
import { useToast } from "../components/toast/useToast";
import { tooltipClass } from "../components/tooltip";
import { useGameLog, useGameLogs } from "../features/logs/useGameLogs";
import { filterLines, type LevelFilter, type LogLevel, parseLog } from "../features/logs/logLines";
import { useRunning } from "../features/play/useGameSync";
import i18n from "../i18n";

/** 一度に描く行の上限。多すぎると画面が重くなるので、末尾（新しい側）だけを出す */
export const MAX_RENDERED_LINES = 3000;

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const iconButton = [
  "inline-flex size-9 items-center justify-center rounded-md text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3 data-disabled:opacity-40",
  ring,
].join(" ");

const chipClass = [
  "inline-flex h-8 cursor-default items-center rounded-md border border-line px-3 text-xs text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  "selected:border-line-strong selected:bg-surface-3 selected:text-text-strong",
  ring,
].join(" ");

const levelClass: Record<LogLevel, string> = {
  error: "text-danger",
  warn: "text-warning",
  other: "text-text",
};

const LEVELS: LevelFilter[] = ["all", "warn", "error"];

/** 近い所までスクロールしていれば、末尾を追いかけているものとみなす（px） */
const STICK_SLACK = 24;

/**
 * インスタンスのゲームのログ。起動ごとに選べて、検索とレベルで絞り込み、全体をコピーできる。
 * 起動中は、いちばん新しい起動のログを一定の間隔で読み直して末尾を追いかける。
 */
export function LogsTab({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const toast = useToast();
  const running = useRunning();
  const isRunning = running?.some((s) => s.instanceId === instance.id) ?? false;
  const logs = useGameLogs(instance.id, isRunning);
  // 選んだ起動。選んでいなければ、いちばん新しいもの
  const [picked, setPicked] = useState<string | null>(null);
  const [level, setLevel] = useState<LevelFilter>("all");
  const [query, setQuery] = useState("");
  const list = logs.data;
  const selected = list?.find((l) => l.name === picked) ?? list?.[0] ?? null;
  const live = isRunning && selected !== null && selected.name === list?.[0]?.name;
  const content = useGameLog(instance.id, selected?.name ?? null, live);

  const date = useMemo(
    () => new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "medium" }),
    // 言語を替えたときに作り直す
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [i18n.language],
  );

  const lines = useMemo(() => parseLog(content.data?.text ?? ""), [content.data?.text]);
  const filtered = useMemo(() => filterLines(lines, level, query), [lines, level, query]);
  const shown = filtered.length > MAX_RENDERED_LINES ? filtered.slice(-MAX_RENDERED_LINES) : filtered;

  // 末尾にいる間は、行が増えても末尾を見せ続ける。上へ読みに行ったら動かさない
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const selectedName = selected?.name;
  useLayoutEffect(() => {
    stick.current = true;
  }, [selectedName]);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [shown, selectedName]);

  const copy = () => {
    navigator.clipboard
      .writeText(filtered.map((l) => l.text).join("\n"))
      .then(() => toast.show({ kind: "success", message: t("logs.copied") }))
      .catch(toast.showError);
  };

  if (!list && logs.error)
    return (
      <div role="alert">
        <FailureNotice error={toErrorPayload(logs.error)} />
      </div>
    );
  if (!list) return null;
  if (!selected) return <EmptyState icon={IconLog} title={t("logs.empty")} />;

  const total = content.data?.truncated ? `${lines.length}+` : String(lines.length);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <Select
          label={t("logs.launch")}
          value={selected.name}
          options={list.map((l) => ({ id: l.name, label: date.format(l.launchedAt) }))}
          onChange={setPicked}
          triggerClassName="min-w-56"
        />
        <SearchField value={query} onChange={setQuery} aria-label={t("logs.search")} className="relative w-56">
          <IconSearch aria-hidden className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-text-muted" />
          <Input className={`${inputBase} w-full pl-8`} />
        </SearchField>
        <ToggleButtonGroup
          aria-label={t("logs.level")}
          selectionMode="single"
          disallowEmptySelection
          selectedKeys={[level]}
          onSelectionChange={(keys: Set<Key>) => {
            const next = [...keys][0];
            if (next !== undefined) setLevel(next as LevelFilter);
          }}
          className="flex gap-1"
        >
          {LEVELS.map((id) => (
            <ToggleButton key={id} id={id} className={chipClass}>
              {t(`logs.levels.${id}`)}
            </ToggleButton>
          ))}
        </ToggleButtonGroup>
        <div className="ml-auto flex items-center gap-1">
          <span className="tabular mr-2 text-xs text-text-muted">
            {filtered.length === lines.length ? total : `${filtered.length} / ${total}`}
          </span>
          <TooltipTrigger delay={500}>
            <Button aria-label={t("logs.copy")} onPress={copy} isDisabled={filtered.length === 0} className={iconButton}>
              <IconCopy aria-hidden className="size-4" />
            </Button>
            <Tooltip offset={6} className={tooltipClass}>
              {t("logs.copy")}
            </Tooltip>
          </TooltipTrigger>
          <TooltipTrigger delay={500}>
            <Button
              aria-label={t("logs.openFolder")}
              onPress={() => {
                openInstanceFolder(instance.id, "logs").catch(toast.showError);
              }}
              className={iconButton}
            >
              <IconFolder aria-hidden className="size-4" />
            </Button>
            <Tooltip offset={6} className={tooltipClass}>
              {t("logs.openFolder")}
            </Tooltip>
          </TooltipTrigger>
        </div>
      </div>
      {content.error && !content.data ? (
        <div role="alert">
          <FailureNotice error={toErrorPayload(content.error)} />
        </div>
      ) : (
        <div
          ref={scroller}
          role="log"
          aria-label={t("logs.content")}
          tabIndex={0}
          onScroll={(e) => {
            const el = e.currentTarget;
            stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_SLACK;
          }}
          className={`scroll-thin min-h-0 flex-1 overflow-auto rounded-md border border-line bg-surface-1 p-3 font-mono text-xs leading-5 ${ring}`}
        >
          {shown.map((line, i) => (
            <div key={i} className={`break-words whitespace-pre-wrap ${levelClass[line.level]}`}>
              {line.text === "" ? " " : line.text}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
