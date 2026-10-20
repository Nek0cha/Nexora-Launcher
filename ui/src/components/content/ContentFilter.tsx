import { useId } from "react";
import { Input, type Key, SearchField, ToggleButton, ToggleButtonGroup } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCheck from "~icons/ph/check";
import IconSearch from "~icons/ph/magnifying-glass";
import type { Side } from "../../bindings/Side";
import type { FilterState, StatusKey } from "../../features/content/contentFilter";
import i18n, { type resources } from "../../i18n";
import { inputBase } from "../settingsLayout";

type CategorySlug = keyof (typeof resources)["ja"]["common"]["content"]["category"];

const sides: Side[] = ["required", "optional", "unsupported"];
const statuses: StatusKey[] = ["enabled", "disabled", "dependency"];

// 選べる値の札。選んだ札は面を明るくし、色だけに頼らないようチェックの印も付ける
const chipClass = [
  "inline-flex h-7 cursor-default items-center gap-1 rounded-full border border-line px-2.5 text-xs text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  "selected:border-line-strong selected:bg-surface-3 selected:text-text-strong",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

/** Modrinth のカテゴリの名前。訳の無い（新しく増えた）ものは slug のまま出す。 */
function categoryLabel(slug: string): string {
  const key = `content.category.${slug}`;
  return i18n.exists(key) ? i18n.t(`content.category.${slug as CategorySlug}`) : slug;
}

function ChipGroup<T extends string>(props: {
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
                {isSelected && <IconCheck aria-hidden className="-ml-0.5 size-3.5" />}
                <span>{o.label}</span>
              </>
            )}
          </ToggleButton>
        ))}
      </ToggleButtonGroup>
    </div>
  );
}

/** 名前で絞り込む入力。絞り込みの部品の中にも、一覧の上の行にも置ける。 */
export function ContentSearchField(props: { value: string; onChange: (query: string) => void; className?: string }) {
  const { t } = useTranslation();
  return (
    <SearchField
      aria-label={t("content.filter.search")}
      value={props.value}
      onChange={props.onChange}
      className={`relative flex items-center ${props.className ?? ""}`}
    >
      <IconSearch aria-hidden className="pointer-events-none absolute left-3 size-4 text-text-muted" />
      <Input
        placeholder={t("content.filter.search")}
        className={`${inputBase} w-full pl-9 [&::-webkit-search-cancel-button]:hidden`}
      />
    </SearchField>
  );
}

/**
 * 絞り込みの部品。一覧には依存しない（5-4 の検索画面でも使う）。
 * `categories` が null（取得できない）ならカテゴリを、`showStatus` が偽なら状態を出さない。
 * `showSearch` が偽なら名前の入力を出さない（呼び出し側が ContentSearchField を別の場所に置く）。
 */
export function ContentFilter(props: {
  state: FilterState;
  onChange: (next: FilterState) => void;
  categories: string[] | null;
  showStatus: boolean;
  showSearch?: boolean;
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
          options={statuses.map((id) => ({ id, label: t(`content.filter.statusValue.${id}`) }))}
          selected={state.status}
          onChange={(status) => onChange({ ...state, status })}
        />
      )}
    </div>
  );
}
