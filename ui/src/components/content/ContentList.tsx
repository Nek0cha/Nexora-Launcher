import { GridList, ListLayout, Virtualizer } from "react-aria-components";
import { useTranslation } from "react-i18next";
import type { ContentItem } from "../../bindings/ContentItem";
import { ContentRow, ROW_SIZE } from "./ContentRow";

/**
 * 導入済みの一覧。見えている行だけを描画する（数百件でも軽く動かす）。
 * 高さは親が決める。親の高さいっぱいに広がり、その中でスクロールする。
 */
export function ContentList(props: {
  items: ContentItem[];
  onToggle: (item: ContentItem) => void;
  onRemove: (item: ContentItem) => void;
  busy: boolean;
}) {
  const { t } = useTranslation();
  return (
    <Virtualizer layout={ListLayout} layoutOptions={{ rowSize: ROW_SIZE }}>
      <GridList
        aria-label={t("content.row.list")}
        items={props.items}
        // 行の描画は items が同じなら使い回されるので、busy と操作が替わったら描き直す
        dependencies={[props.busy, props.onToggle, props.onRemove]}
        className="scroll-thin h-full min-h-0 overflow-auto outline-none"
      >
        {(item) => <ContentRow item={item} onToggle={props.onToggle} onRemove={props.onRemove} busy={props.busy} />}
      </GridList>
    </Virtualizer>
  );
}
