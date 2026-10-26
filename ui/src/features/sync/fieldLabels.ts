import type { i18n as I18n } from "i18next";

/**
 * 項目の表示名。文言のキー `sync.fields.<id>` で引く（ID の `.` もそのままキーに含める）。
 * 名前の無い項目（Mod が足したキー割り当てなど）は、ID を見せずに「その他の設定」とする。
 */
export function fieldLabel(id: string, i18n: Pick<I18n, "t" | "exists">): string {
  return hasFieldLabel(id, i18n) ? i18n.t(`sync.fields.${id}` as "sync.otherField") : i18n.t("sync.otherField");
}

/** 項目に固有の表示名があるか */
export function hasFieldLabel(id: string, i18n: Pick<I18n, "exists">): boolean {
  return id !== "" && i18n.exists(`sync.fields.${id}`);
}
