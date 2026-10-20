import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button, GridListItem, Tooltip, TooltipTrigger } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconResourcePack from "~icons/ph/image";
import IconShader from "~icons/ph/package";
import IconMod from "~icons/ph/puzzle-piece";
import IconTrash from "~icons/ph/trash";
import { getContentIcon } from "../../api/commands";
import type { ContentItem } from "../../bindings/ContentItem";
import type { ContentKind } from "../../bindings/ContentKind";
import { tooltipClass } from "../tooltip";

/** 1 行の高さ（px）。ListLayout の rowSize と行の h-14 を揃える。 */
export const ROW_SIZE = 56;

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const defaultIcons: Record<ContentKind, typeof IconMod> = {
  mod: IconMod,
  resourcepack: IconResourcePack,
  shader: IconShader,
};

// 一覧の行の操作。面は乗せたときだけ現れる（インスタンスの詳細のアイコンのボタンと同じ）
const iconButton = [
  "inline-flex size-8 shrink-0 items-center justify-center rounded-md text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3 disabled:opacity-50 disabled:hover:bg-transparent",
  ring,
].join(" ");

// 有効無効のトグル。押されている（有効）ときは溝を明るくし、つまみを右へ寄せる
const toggleClass = [
  "group relative inline-flex h-5 w-8 shrink-0 items-center rounded-full border border-line-control bg-surface-1",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "aria-pressed:border-transparent aria-pressed:bg-accent disabled:opacity-50",
  ring,
].join(" ");

const knobClass = [
  "ml-0.5 size-3.5 rounded-full bg-text-muted",
  "transition-[translate,background-color] duration-(--duration-fast) ease-out-quint",
  "group-aria-pressed:translate-x-3 group-aria-pressed:bg-accent-ink motion-reduce:transition-colors",
].join(" ");

/** 行のアイコン。Modrinth の URL、jar の中のアイコン、既定の絵の順に使う。 */
function RowIcon({ item }: { item: ContentItem }) {
  // URL が読めなかった（オフラインなど）ときは既定の絵に戻す
  const [failed, setFailed] = useState(false);
  const local = useQuery({
    queryKey: ["content-icon", item.sha1],
    queryFn: () => getContentIcon(item.sha1),
    staleTime: Infinity,
    enabled: item.iconUrl === null && item.hasLocalIcon,
  });
  const src = item.iconUrl ?? local.data ?? null;
  if (src !== null && !failed)
    return (
      <img
        src={src}
        alt=""
        loading="lazy"
        onError={() => setFailed(true)}
        className="size-8 shrink-0 rounded-sm object-cover"
      />
    );
  const Icon = defaultIcons[item.kind];
  return (
    <span className="grid size-8 shrink-0 place-items-center rounded-sm bg-surface-2 text-text-muted">
      <Icon aria-hidden className="size-4" />
    </span>
  );
}

export function ContentRow(props: {
  item: ContentItem;
  onToggle: (item: ContentItem) => void;
  onRemove: (item: ContentItem) => void;
  busy: boolean;
}) {
  const { t } = useTranslation();
  const { item } = props;
  const name = item.name ?? item.fileName;

  return (
    <GridListItem
      id={item.relPath}
      textValue={name}
      className={`flex h-14 items-center gap-3 rounded-md px-3 outline-none hover:bg-surface-1 ${ring}`}
    >
      {/* 無効の行は中身だけを薄くする。操作のボタンは薄くしない */}
      <div className={`flex min-w-0 flex-1 items-center gap-3 ${item.enabled ? "" : "opacity-50"}`}>
        <RowIcon item={item} />
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex min-w-0 items-baseline gap-2">
            <span className="truncate text-text">{name}</span>
            {item.versionNumber !== null && (
              <span className="tabular shrink-0 text-xs text-text-muted">{item.versionNumber}</span>
            )}
            {item.installedAs === "dependency" && (
              <span className="shrink-0 self-center rounded-sm border border-line px-1.5 text-xs leading-4 text-text-muted">
                {t("content.row.dependency")}
              </span>
            )}
          </div>
          {item.description !== null && <span className="truncate text-xs text-text-muted">{item.description}</span>}
        </div>
      </div>
      <Button
        aria-label={t("content.row.enable", { name })}
        aria-pressed={item.enabled}
        isDisabled={props.busy}
        onPress={() => props.onToggle(item)}
        className={toggleClass}
      >
        <span aria-hidden className={knobClass} />
      </Button>
      {/* アイコンだけのボタンなので、名前をツールチップでも見せる */}
      <TooltipTrigger delay={500}>
        <Button
          aria-label={t("content.row.remove", { name })}
          isDisabled={props.busy}
          onPress={() => props.onRemove(item)}
          className={iconButton}
        >
          <IconTrash aria-hidden className="size-4" />
        </Button>
        <Tooltip offset={6} className={tooltipClass}>
          {t("content.row.removeShort")}
        </Tooltip>
      </TooltipTrigger>
    </GridListItem>
  );
}
