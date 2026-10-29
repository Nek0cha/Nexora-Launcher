import type { TFunction } from "i18next";
import { beforeAll, describe, expect, it } from "vitest";
import { ITEMS } from "../../components/Sidebar";
import i18n from "../../i18n";
import { crumbsFor } from "./crumbs";

let t: TFunction;

beforeAll(async () => {
  await i18n.changeLanguage("ja");
  t = i18n.getFixedT("ja");
});

const ctx = (instanceName: string | null = null) => ({ instanceName, t });

describe("crumbsFor", () => {
  it("returns_a_single_current_crumb_for_each_top_level_screen", () => {
    const cases: [string, string][] = [
      ["/", "ホーム"],
      ["/instances", "インスタンス"],
      ["/sync", "共通設定"],
      ["/browse", "探す"],
      ["/skins", "スキン"],
      ["/accounts", "アカウント"],
      ["/settings", "設定"],
    ];
    for (const [path, label] of cases) {
      expect(crumbsFor(path, ctx())).toEqual([{ label, to: null }]);
    }
  });

  it("links_the_instances_crumb_on_the_detail_screen", () => {
    expect(crumbsFor("/instances/i1", ctx("Forge 26.3"))).toEqual([
      { label: "インスタンス", to: "/instances" },
      { label: "Forge 26.3", to: null },
    ]);
  });

  it("adds_the_modrinth_crumb_under_the_instance", () => {
    expect(crumbsFor("/instances/i1/modrinth", ctx("Forge 26.3"))).toEqual([
      { label: "インスタンス", to: "/instances" },
      { label: "Forge 26.3", to: "/instances/i1" },
      { label: "Modrinth", to: null },
    ]);
  });

  it("falls_back_to_a_plain_instances_crumb_while_the_name_is_unknown", () => {
    expect(crumbsFor("/instances/i1", ctx(null))).toEqual([{ label: "インスタンス", to: null }]);
  });

  it("ends_with_the_same_word_as_the_sidebar_item", () => {
    for (const { to, key } of ITEMS) {
      const crumbs = crumbsFor(to, ctx());
      expect(crumbs.at(-1)?.label).toBe(t(`nav.${key}`));
    }
  });
});
