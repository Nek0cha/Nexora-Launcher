import type { ContentItem } from "../../bindings/ContentItem";
import type { Side } from "../../bindings/Side";

export type StatusKey = "enabled" | "disabled" | "dependency";

export type FilterState = {
  query: string;
  client: Side[];
  server: Side[];
  categories: string[];
  status: StatusKey[];
};

export const emptyFilter: FilterState = { query: "", client: [], server: [], categories: [], status: [] };

/** クライアント側、サーバー側、カテゴリのどれかが選ばれているか。 */
export function isMetadataFiltered(state: FilterState): boolean {
  return state.client.length > 0 || state.server.length > 0 || state.categories.length > 0;
}

function matchesName(item: ContentItem, query: string): boolean {
  const q = query.trim().toLowerCase();
  return q === "" || (item.name ?? item.fileName).toLowerCase().includes(q);
}

function matchesStatus(item: ContentItem, status: StatusKey[]): boolean {
  if (status.length === 0) return true;
  return status.some((key) =>
    key === "enabled" ? item.enabled : key === "disabled" ? !item.enabled : item.installedAs === "dependency",
  );
}

function matchesMetadata(item: ContentItem, state: FilterState): boolean {
  if (state.client.length > 0 && !(item.clientSide !== null && state.client.includes(item.clientSide))) return false;
  if (state.server.length > 0 && !(item.serverSide !== null && state.server.includes(item.serverSide))) return false;
  return state.categories.every((c) => item.categories.includes(c));
}

/**
 * 絞り込む。メタデータの条件があるとき、Modrinth 未特定（local）の行は `visible` に入れず、
 * 名前と状態には合っていた行だけを `excluded` に入れる。
 */
export function applyFilter(
  items: ContentItem[],
  state: FilterState,
): { visible: ContentItem[]; excluded: ContentItem[] } {
  const metadata = isMetadataFiltered(state);
  const visible: ContentItem[] = [];
  const excluded: ContentItem[] = [];
  for (const item of items) {
    if (!matchesName(item, state.query) || !matchesStatus(item, state.status)) continue;
    if (metadata && item.provider === "local") excluded.push(item);
    else if (matchesMetadata(item, state)) visible.push(item);
  }
  return { visible, excluded };
}

/**
 * 画面に出す名前（無ければファイル名）の順に並べた新しい配列を返す。大文字と小文字は区別しない。
 * 名前が同じなら `relPath` の順にして、並びがぶれないようにする。
 */
export function sortByName(items: ContentItem[]): ContentItem[] {
  const key = (item: ContentItem) => (item.name ?? item.fileName).toLowerCase();
  return items
    .map((item) => ({ item, key: key(item) }))
    .sort((a, b) => a.key.localeCompare(b.key) || a.item.relPath.localeCompare(b.item.relPath))
    .map(({ item }) => item);
}

/** `target` に依存している、有効な行。プロジェクト ID が無ければ空。 */
export function dependentsOf(items: ContentItem[], target: ContentItem): ContentItem[] {
  const id = target.projectId;
  if (id === null) return [];
  return items.filter((i) => i !== target && i.enabled && i.dependencies.includes(id));
}
