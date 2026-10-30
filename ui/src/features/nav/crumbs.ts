import type { TFunction } from "i18next";

/** パンくずの 1 つ。`to` が null なら今の画面（リンクにしない） */
export type Crumb = { label: string; to: string | null };

/** 先頭の区間の名前。キーは文字列のまま書く（使われていないキーの検査が見つけられるように） */
function sectionLabel(segment: string, t: TFunction): string | null {
  switch (segment) {
    case "":
      return t("nav.home");
    case "instances":
      return t("nav.instances");
    case "sync":
      return t("nav.sync");
    case "browse":
      return t("nav.browse");
    case "skins":
      return t("nav.skins");
    case "accounts":
      return t("nav.accounts");
    case "settings":
      return t("nav.settings");
    default:
      return null;
  }
}

/**
 * 今の場所のパンくず。サイドバーの項目と同じ語で終わる。
 * インスタンスの詳細では、名前がまだ取れていなければ「インスタンス」だけにする。
 */
export function crumbsFor(pathname: string, ctx: { instanceName: string | null; t: TFunction }): Crumb[] {
  const { instanceName, t } = ctx;
  const [section = "", id, sub] = pathname.split("/").filter(Boolean);
  const label = sectionLabel(section, t);
  if (label === null) return [];
  if (section !== "instances" || !id) return [{ label, to: null }];
  if (instanceName === null) return [{ label, to: null }];
  if (sub === "modrinth") {
    return [
      { label, to: "/instances" },
      { label: instanceName, to: `/instances/${id}` },
      { label: t("modrinth.title"), to: null },
    ];
  }
  return [
    { label, to: "/instances" },
    { label: instanceName, to: null },
  ];
}
