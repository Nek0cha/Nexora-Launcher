import { useMemo } from "react";
import { GridList, ListLayout, Virtualizer } from "react-aria-components";
import { useTranslation } from "react-i18next";
import type { AvailableUpdate } from "../../bindings/AvailableUpdate";
import type { ContentItem } from "../../bindings/ContentItem";
import { ContentRow, ROW_SIZE } from "./ContentRow";

/** 行に出す更新の状態。キーはどれも行の `relPath`。 */
export type RowUpdates = {
  /** 更新の確認で見つかった更新 */
  available: ReadonlyMap<string, AvailableUpdate>;
  /** このインスタンスに合う版が Modrinth に無かった行 */
  noCompatible: ReadonlySet<string>;
  /** いま更新している行 */
  updating: ReadonlySet<string>;
  onUpdate: (item: ContentItem) => void;
};

/**
 * 導入済みの一覧。見えている行だけを描画する（数百件でも軽く動かす）。
 * 高さは親が決める。親の高さいっぱいに広がり、その中でスクロールする。
 * `label` は読み上げの名前。省くと「導入済みのコンテンツ」になる。
 * `updates` を省くと、更新のボタンと印は出さない（パックとシェーダー）。
 */
export function ContentList(props: {
  items: ContentItem[];
  onToggle: (item: ContentItem) => void;
  onRemove: (item: ContentItem) => void;
  busy: boolean;
  label?: string;
  updates?: RowUpdates;
}) {
  const { t } = useTranslation();
  const { updates } = props;
  // 行はデータの id で識別される。id が無いと位置で識別され、切り替えや更新でファイル名（= relPath）が
  // 変わったときに「id を変えられない」で落ちる。relPath を id にして、行ごとに別の項目として扱わせる
  const rows = useMemo(() => props.items.map((item) => ({ id: item.relPath, item })), [props.items]);
  return (
    <Virtualizer layout={ListLayout} layoutOptions={{ rowSize: ROW_SIZE }}>
      <GridList
        aria-label={props.label ?? t("content.row.list")}
        items={rows}
        // 行の描画は items が同じなら使い回されるので、busy と操作と更新の状態が替わったら描き直す
        dependencies={[props.busy, props.onToggle, props.onRemove, updates]}
        // 行の中身を一覧の幅で切り替える（狭いときは更新のボタンを絵だけにする）
        className="scroll-thin @container h-full min-h-0 overflow-auto outline-none"
      >
        {({ item }) => (
          <ContentRow
            item={item}
            onToggle={props.onToggle}
            onRemove={props.onRemove}
            busy={props.busy}
            update={updates?.available.get(item.relPath) ?? null}
            noCompatible={updates?.noCompatible.has(item.relPath) ?? false}
            updating={updates?.updating.has(item.relPath) ?? false}
            onUpdate={updates?.onUpdate}
          />
        )}
      </GridList>
    </Virtualizer>
  );
}
