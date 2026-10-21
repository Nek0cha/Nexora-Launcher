import { type ReactNode, useId, useMemo, useState } from "react";
import { Button, Tooltip, TooltipTrigger } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconOpen from "~icons/ph/arrow-square-out";
import IconDownload from "~icons/ph/download-simple";
import IconMod from "~icons/ph/puzzle-piece";
import type { LoaderKind } from "../../bindings/LoaderKind";
import type { SearchHit } from "../../bindings/SearchHit";
import { categoryLabel, isKnownCategory } from "../content/ContentFilter";
import { tooltipClass } from "../tooltip";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 結果の操作のボタン（確認のダイアログの「キャンセル」と同じ、枠だけのボタン）。20 件並ぶので面は塗らない
export const actionButton = [
  "inline-flex h-8 min-w-0 items-center gap-1.5 whitespace-nowrap rounded-md border border-line-control px-2.5 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3",
  "data-disabled:opacity-50 data-disabled:hover:bg-transparent data-pending:cursor-progress",
  ring,
].join(" ");

// 面は乗せたときだけ現れるアイコンのボタン（インスタンスの詳細の見出しのボタンと同じ）
const iconButton = [
  "inline-flex size-8 shrink-0 items-center justify-center rounded-md text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  ring,
].join(" ");

const loaderNames: LoaderKind[] = ["fabric", "quilt", "forge", "neoforge"];

/**
 * 結果のアイコン。読めない（オフラインなど）ときは既定の絵にする。
 * 枠が大きさを決める正方形で、画像は枠に重ねて広げる。img のままだと既定の `max-width: 100%` で
 * 列の幅に縮み、正方形でなくなる（コンテンツの一覧の行のアイコンと同じ作り）。
 */
function HitIcon({ url }: { url: string | null }) {
  const [failed, setFailed] = useState(false);
  return (
    <span
      data-testid="hit-icon"
      className="relative grid aspect-square size-12 shrink-0 place-items-center overflow-hidden rounded-md bg-surface-2 text-text-muted"
    >
      {url !== null && !failed ? (
        <img
          src={url}
          alt=""
          width={48}
          height={48}
          loading="lazy"
          onError={() => setFailed(true)}
          className="absolute inset-0 block size-full max-w-none object-cover"
        />
      ) : (
        <IconMod aria-hidden className="size-5" />
      )}
    </span>
  );
}

/**
 * 検索結果の 1 件。名前、説明、ダウンロード数、カテゴリ、ブラウザで開くボタンを出し、
 * 追加の操作（`actions`）は下の行の右に置く。幅が狭いときは操作が次の行に回る。
 */
export function HitCard(props: {
  hit: SearchHit;
  actions: ReactNode;
  onOpen: (slug: string) => void;
  /** 対応するローダーを出すか（ローダーを固定していない全体の画面だけで出す） */
  showLoaders: boolean;
}) {
  const { t, i18n } = useTranslation();
  const { hit } = props;
  const titleId = useId();
  const compact = useMemo(() => new Intl.NumberFormat(i18n.language, { notation: "compact" }), [i18n.language]);
  const categories = hit.categories.filter(isKnownCategory);
  const loaders = loaderNames.filter((l) => hit.categories.includes(l));

  return (
    // 左の列の幅はアイコンに合わせる（auto）。rem の固定の幅は、ルートの文字の大きさによってアイコンより狭くなる。
    // 一覧（@container）が狭いときは、下の行（ダウンロード数と操作）をアイコンの下まで広げて、ボタンを 1 行に収める
    <article aria-labelledby={titleId} className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 py-4">
      <HitIcon url={hit.iconUrl} />
      <div className="col-start-2 flex min-w-0 flex-col gap-1">
        <div className="flex min-w-0 items-start gap-2">
          <h3 id={titleId} className="min-w-0 flex-1 truncate pt-0.5 font-medium text-text-strong">
            {hit.title}
          </h3>
          {/* アイコンだけのボタンなので、名前をツールチップでも見せる */}
          <TooltipTrigger delay={500}>
            <Button aria-label={t("modrinth.openPage")} onPress={() => props.onOpen(hit.slug)} className={`-my-1 ${iconButton}`}>
              <IconOpen aria-hidden className="size-4" />
            </Button>
            <Tooltip offset={6} className={tooltipClass}>
              {t("modrinth.openPage")}
            </Tooltip>
          </TooltipTrigger>
        </div>
        <p className="line-clamp-2 text-sm text-text-muted">{hit.description}</p>
      </div>
      <div className="col-start-2 mt-2.5 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2 @max-lg:col-span-2 @max-lg:col-start-1">
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5 text-xs text-text-muted">
          <span className="tabular inline-flex items-center gap-1">
            <IconDownload aria-hidden className="size-3.5" />
            {t("modrinth.downloads", { value: compact.format(hit.downloads) })}
          </span>
          {props.showLoaders && loaders.length > 0 && (
            <span>{loaders.map((l) => t(`create.tab.${l}`)).join(" · ")}</span>
          )}
          {categories.length > 0 && (
            <ul className="flex flex-wrap gap-1">
              {categories.map((c) => (
                <li key={c} className="rounded-sm border border-line px-1.5 leading-4">
                  {categoryLabel(c)}
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="ml-auto flex min-w-0 items-center gap-1.5">{props.actions}</div>
      </div>
    </article>
  );
}
