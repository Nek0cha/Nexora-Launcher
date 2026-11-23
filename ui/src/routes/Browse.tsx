import { useSearchParams } from "react-router";
import { type Key, ToggleButton, ToggleButtonGroup } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { ModrinthBrowser } from "../components/modrinth/ModrinthBrowser";
import { contentKinds, parseKind } from "../features/modrinth/kind";

const title = "font-display text-xl font-semibold tracking-[-0.01em] text-text-strong";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 種類の切り替え。検索画面の並び順の切り替えと同じ見せ方
const kindItemClass = [
  "inline-flex h-7 cursor-default items-center rounded-sm px-3 text-sm text-text-muted outline-none",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:text-text selected:bg-surface-3 selected:text-text-strong",
  ring,
].join(" ");

/** サイドバーから開く、インスタンスに関係なく MOD、リソースパック、シェーダーを探す画面（spec 6.1）。 */
export default function Browse() {
  const { t } = useTranslation();
  // 種類は URL に持つ（戻ったときに同じ種類で開く）
  const [params, setParams] = useSearchParams();
  const kind = parseKind(params.get("kind"));
  return (
    <div className="flex h-full min-h-[34rem] max-w-5xl flex-col gap-6 p-10">
      <header className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h1 className={title}>{t("modrinth.browseTitle")}</h1>
          <span className="text-sm text-text-muted">{t("modrinth.title")}</span>
        </div>
        <ToggleButtonGroup
          aria-label={t("content.tabs.label")}
          selectionMode="single"
          disallowEmptySelection
          selectedKeys={[kind]}
          onSelectionChange={(keys: Set<Key>) => {
            const [next] = keys;
            if (next !== undefined) setParams(next === "mod" ? {} : { kind: String(next) }, { replace: true });
          }}
          className="inline-flex h-9 shrink-0 items-center gap-0.5 rounded-md border border-line-control p-0.5"
        >
          {contentKinds.map((k) => (
            <ToggleButton key={k} id={k} className={kindItemClass}>
              {t(`content.tabs.${k}`)}
            </ToggleButton>
          ))}
        </ToggleButtonGroup>
      </header>
      {/* 種類が替わったら絞り込みを作り直す（カテゴリーは種類ごとに違う） */}
      <ModrinthBrowser key={kind} kind={kind} />
    </div>
  );
}
