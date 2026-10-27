import { useId } from "react";
import { Input, type Key, SearchField, ToggleButton, ToggleButtonGroup } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCheck from "~icons/ph/check";
import IconSearch from "~icons/ph/magnifying-glass";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import type { Side } from "../../bindings/Side";
import type { FilterState, StatusKey } from "../../features/content/contentFilter";
import i18n, { type resources } from "../../i18n";
import { FailureNotice } from "../FailureNotice";
import { inputBase } from "../settingsLayout";
import { Skeleton } from "../Skeleton";

type CategorySlug = keyof (typeof resources)["ja"]["common"]["content"]["category"];

const sides: Side[] = ["required", "optional", "unsupported"];
const defaultStatuses: StatusKey[] = ["enabled", "disabled", "dependency"];

/** 訳のあるカテゴリか。Modrinth の検索結果の categories には、ローダーの名前も混ざっている。 */
export function isKnownCategory(slug: string): boolean {
  return i18n.exists(`content.category.${slug}`);
}

// 選べる値の札。選んだ札は面を明るくし、色だけに頼らないようチェックの印も付ける
// 寸法は状態で変えない。読み込み中の場所取り（ChipSkeleton）と共有し、折り返しまで本物と同じにする
const chipGeometry =
  "inline-flex h-7 shrink-0 items-center gap-1 rounded-full border px-2.5 text-xs whitespace-nowrap";

const chipClass = [
  `${chipGeometry} cursor-default border-line text-text-muted`,
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  "selected:border-line-strong selected:bg-surface-3 selected:text-text-strong",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

/** Modrinth のカテゴリの名前。訳の無い（新しく増えた）ものは slug のまま出す。 */
export function categoryLabel(slug: string): string {
  const key = `content.category.${slug}`;
  return i18n.exists(key) ? i18n.t(`content.category.${slug as CategorySlug}`) : slug;
}

/** 札を並べた複数選択。Modrinth の検索のローダーの絞り込みでも使う。 */
export function ChipGroup<T extends string>(props: {
  label: string;
  options: { id: T; label: string }[];
  selected: T[];
  onChange: (next: T[]) => void;
}) {
  const labelId = useId();
  return (
    <div className="flex flex-col gap-2">
      <span id={labelId} className="text-xs text-text-muted">
        {props.label}
      </span>
      <ToggleButtonGroup
        aria-labelledby={labelId}
        selectionMode="multiple"
        selectedKeys={props.selected}
        // 選んだ順を保つ（今までの選択の後ろに足される）
        onSelectionChange={(keys: Set<Key>) => props.onChange(Array.from(keys) as T[])}
        className="flex flex-wrap gap-1.5"
      >
        {props.options.map((o) => (
          <ToggleButton key={o.id} id={o.id} className={chipClass}>
            {({ isSelected }) => (
              <>
                {/* チェックの場所は選ばれていない札でも確保し、選んでも幅が変わらないようにする */}
                <span aria-hidden data-check-slot className="-ml-0.5 grid size-3.5 shrink-0 place-items-center">
                  {isSelected && <IconCheck className="size-3.5" />}
                </span>
                <span>{o.label}</span>
              </>
            )}
          </ToggleButton>
        ))}
      </ToggleButtonGroup>
    </div>
  );
}

/**
 * 名前で絞り込む入力。絞り込みの部品の中にも、一覧の上の行にも置ける。
 * `label` を渡すと名前と入力の案内を差し替える（Modrinth の検索で使う）。
 */
export function ContentSearchField(props: {
  value: string;
  onChange: (query: string) => void;
  className?: string;
  label?: string;
}) {
  const { t } = useTranslation();
  const label = props.label ?? t("content.filter.search");
  return (
    <SearchField
      aria-label={label}
      value={props.value}
      onChange={props.onChange}
      className={`relative flex items-center ${props.className ?? ""}`}
    >
      <IconSearch aria-hidden className="pointer-events-none absolute left-3 size-4 text-text-muted" />
      <Input
        placeholder={label}
        className={`${inputBase} w-full pl-9 [&::-webkit-search-cancel-button]:hidden`}
      />
    </SearchField>
  );
}

/**
 * 絞り込みの部品。一覧には依存しない（5-4 の検索画面でも使う）。
 * `categories` が null（取得できない）ならカテゴリを、`showStatus` が偽なら状態を出さない。
 * `showSearch` が偽なら名前の入力を出さない（呼び出し側が ContentSearchField を別の場所に置く）。
 * `statuses` は出す状態の札。省くと有効、無効、依存の 3 つ（「更新あり」は MOD のサブタブで足す）。
 */
export function ContentFilter(props: {
  state: FilterState;
  onChange: (next: FilterState) => void;
  categories: string[] | null;
  /** カテゴリの取得に失敗したときの理由。あれば、カテゴリの場所に理由を出す */
  categoriesError?: ErrorPayload | null;
  showStatus: boolean;
  showSearch?: boolean;
  statuses?: StatusKey[];
}) {
  const { t } = useTranslation();
  const { state, onChange } = props;
  const sideOptions = sides.map((id) => ({ id, label: t(`content.filter.side.${id}`) }));

  return (
    <div className="flex flex-col gap-4">
      {(props.showSearch ?? true) && (
        <ContentSearchField value={state.query} onChange={(query) => onChange({ ...state, query })} />
      )}
      <ChipGroup
        label={t("content.filter.client")}
        options={sideOptions}
        selected={state.client}
        onChange={(client) => onChange({ ...state, client })}
      />
      <ChipGroup
        label={t("content.filter.server")}
        options={sideOptions}
        selected={state.server}
        onChange={(server) => onChange({ ...state, server })}
      />
      {props.categoriesError && (
        <div data-testid="categories-failed" className="flex flex-col gap-2">
          <span className="text-xs text-text-muted">{t("content.filter.categories")}</span>
          <FailureNotice error={props.categoriesError} />
        </div>
      )}
      {props.categories !== null && props.categories.length > 0 && (
        <ChipGroup
          label={t("content.filter.categories")}
          options={props.categories.map((id) => ({ id, label: categoryLabel(id) }))}
          selected={state.categories}
          onChange={(categories) => onChange({ ...state, categories })}
        />
      )}
      {props.showStatus && (
        <ChipGroup
          label={t("content.filter.status")}
          options={(props.statuses ?? defaultStatuses).map((id) => ({
            id,
            label: t(`content.filter.statusValue.${id}`),
          }))}
          selected={state.status}
          onChange={(status) => onChange({ ...state, status })}
        />
      )}
    </div>
  );
}

/** 札と同じ寸法の場所取り。文字は見えないが同じ幅を取るので、折り返しも本物と同じになる */
export function ChipSkeleton({ label }: { label: string }) {
  return (
    <div aria-hidden className={`${chipGeometry} border-transparent bg-surface-2 motion-safe:animate-pulse`}>
      <span className="-ml-0.5 size-3.5 shrink-0" />
      <span className="invisible">{label}</span>
    </div>
  );
}

function ChipGroupSkeleton({ labels }: { labels: string[] }) {
  return (
    <div className="flex flex-col gap-2">
      <Skeleton className="h-[18px] w-16" />
      <div className="flex flex-wrap gap-1.5">
        {labels.map((label) => (
          <ChipSkeleton key={label} label={label} />
        ))}
      </div>
    </div>
  );
}

// Modrinth の MOD のカテゴリ。完成形の折り返しに近づけるための並び（取得できたものと違っても、高さはほぼ同じ）
const skeletonCategories = [
  "adventure", "cursed", "decoration", "economy", "equipment", "food", "game-mechanics", "library", "magic",
  "management", "minigame", "mobs", "optimization", "social", "storage", "technology", "transportation",
  "utility", "worldgen",
];

/**
 * 絞り込みの列と同じ並びと寸法のスケルトン。見出しと札の数は、完成形（クライアント、サーバー、カテゴリ、状態）に合わせる。
 * 札は本物と同じ幅の場所取りで、折り返しも同じになる。
 */
export function FilterSkeleton({ status }: { status: boolean }) {
  const { t } = useTranslation();
  const sideLabels = sides.map((id) => t(`content.filter.side.${id}`));
  return (
    <div className="flex flex-col gap-4">
      <ChipGroupSkeleton labels={sideLabels} />
      <ChipGroupSkeleton labels={sideLabels} />
      <ChipGroupSkeleton labels={skeletonCategories.map(categoryLabel)} />
      {status && <ChipGroupSkeleton labels={defaultStatuses.map((id) => t(`content.filter.statusValue.${id}`))} />}
    </div>
  );
}
