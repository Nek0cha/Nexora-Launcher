import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AvailableUpdate } from "../../bindings/AvailableUpdate";
import type { ContentItem } from "../../bindings/ContentItem";
import type { VersionChoice } from "../../bindings/VersionChoice";
import { listContentVersions } from "../../api/commands";
import i18n from "../../i18n";
import { ContentList, type RowUpdates } from "./ContentList";

vi.mock("../../api/commands", () => ({
  getContentIcon: vi.fn().mockResolvedValue(null),
  listContentVersions: vi.fn(),
}));

function item(over: Partial<ContentItem> = {}): ContentItem {
  return {
    kind: "mod", relPath: "sodium.jar", fileName: "sodium.jar", enabled: true, size: 1, sha1: "s1",
    provider: "modrinth", projectId: "p1", versionId: "v1", pinned: false, installedAs: "explicit",
    name: "Sodium", versionNumber: "0.6.0", description: null, iconUrl: null, hasLocalIcon: false,
    clientSide: "required", serverSide: "unsupported", categories: [], dependencies: [], identified: true,
    ...over,
  };
}

const sodium = item();
const iris = item({ relPath: "iris.jar", fileName: "iris.jar", sha1: "s2", projectId: "p2", name: "Iris" });
const lithium = item({
  relPath: "lithium.jar.disabled", fileName: "lithium.jar.disabled", sha1: "s3", projectId: "p3", name: "Lithium",
  enabled: false,
});

const update = (relPath: string, versionNumber: string): AvailableUpdate => ({ relPath, versionId: `v-${relPath}`, versionNumber });

// jsdom は要素の大きさを持たないので、Virtualizer が窓を計算できるよう一定の大きさを返す
const spies: { mockRestore(): void }[] = [];
beforeAll(async () => {
  await i18n.changeLanguage("en");
  const proto = HTMLElement.prototype;
  spies.push(
    vi.spyOn(proto, "clientWidth", "get").mockReturnValue(800),
    vi.spyOn(proto, "clientHeight", "get").mockReturnValue(600),
    vi.spyOn(proto, "offsetWidth", "get").mockReturnValue(800),
    vi.spyOn(proto, "offsetHeight", "get").mockReturnValue(600),
    vi.spyOn(proto, "scrollHeight", "get").mockReturnValue(600),
    vi.spyOn(proto, "getBoundingClientRect").mockReturnValue(DOMRect.fromRect({ x: 0, y: 0, width: 800, height: 600 })),
  );
});

afterAll(() => {
  for (const spy of spies) spy.mockRestore();
});

afterEach(() => {
  cleanup();
  vi.mocked(listContentVersions).mockReset();
});

function mount(items: ContentItem[], updates: Partial<RowUpdates>, busy = false) {
  const onUpdate = vi.fn<(item: ContentItem) => void>();
  const onChangeVersion = vi.fn<(item: ContentItem, choice: VersionChoice) => void>();
  const rowUpdates: RowUpdates = {
    available: new Map(),
    noCompatible: new Set(),
    updating: new Set(),
    onUpdate,
    instanceId: "i1",
    onChangeVersion,
    ...updates,
  };
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ContentList items={items} onToggle={vi.fn()} onRemove={vi.fn()} busy={busy} updates={rowUpdates} />
    </QueryClientProvider>,
  );
  return { onUpdate, onChangeVersion };
}

const row = (name: string) =>
  screen.getAllByRole("row").find((r) => within(r).queryByText(name) !== null)!;

describe("ContentRow updates", () => {
  it("update_button_appears_only_for_rows_with_an_update_and_shows_the_target_version_on_hover", async () => {
    const { onUpdate } = mount([sodium, iris], { available: new Map([["sodium.jar", update("sodium.jar", "0.6.1")]]) });
    const button = within(row("Sodium")).getByRole("button", { name: "Update Sodium to 0.6.1" });
    expect(within(row("Iris")).queryByRole("button", { name: /^Update/ })).toBeNull();
    // ボタンの文字は短く、行き先の版はツールチップで見せる
    expect(button.textContent).toBe("Update");
    fireEvent.keyDown(document.body, { key: "Tab" });
    act(() => button.focus());
    const tooltip = await screen.findByRole("tooltip");
    expect(tooltip.textContent).toBe("Update to 0.6.1");
    fireEvent.click(button);
    expect(onUpdate).toHaveBeenCalledWith(sodium);
  });

  it("disabled_mod_with_update_shows_the_button", () => {
    mount([lithium], { available: new Map([["lithium.jar.disabled", update("lithium.jar.disabled", "0.14.0")]]) });
    const button = screen.getByRole("button", { name: "Update Lithium to 0.14.0" });
    expect(button.hasAttribute("disabled")).toBe(false);
    // 無効の行でも、更新のボタンは薄くしない
    expect(button.closest(".opacity-50")).toBeNull();
  });

  it("the update button is disabled while busy", () => {
    const { onUpdate } = mount([sodium], { available: new Map([["sodium.jar", update("sodium.jar", "0.6.1")]]) }, true);
    const button = screen.getByRole("button", { name: "Update Sodium to 0.6.1" });
    expect(button.hasAttribute("disabled")).toBe(true);
    fireEvent.click(button);
    expect(onUpdate).not.toHaveBeenCalled();
  });

  it("an updating row shows progress instead of the button", () => {
    mount([sodium, iris], {
      available: new Map([["sodium.jar", update("sodium.jar", "0.6.1")]]),
      updating: new Set(["sodium.jar"]),
    });
    expect(screen.queryByRole("button", { name: "Update Sodium to 0.6.1" })).toBeNull();
    expect(within(row("Sodium")).getByText("Updating…")).toBeTruthy();
    expect(within(row("Iris")).queryByText("Updating…")).toBeNull();
  });

  it("row_shows_a_hint_when_no_compatible_version_is_found", async () => {
    mount([sodium, iris], { noCompatible: new Set(["sodium.jar"]) });
    const message = "No version for this instance was found on Modrinth";
    const hint = within(row("Sodium")).getByRole("img", { name: message });
    expect(within(row("Iris")).queryByRole("img", { name: message })).toBeNull();
    fireEvent.keyDown(document.body, { key: "Tab" });
    act(() => hint.focus());
    expect((await screen.findByRole("tooltip")).textContent).toBe(message);
  });

  it("shows no update controls without update state", () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ContentList items={[sodium]} onToggle={vi.fn()} onRemove={vi.fn()} busy={false} />
      </QueryClientProvider>,
    );
    expect(screen.queryByRole("button", { name: /^Update/ })).toBeNull();
  });
});

const choice = (versionId: string, versionNumber: string, versionType: string, current = false): VersionChoice => ({
  versionId, versionNumber, versionType, datePublished: "2024-05-01T00:00:00Z", current,
});

describe("ContentRow version picker", () => {
  const choices = [
    choice("v3", "0.7.0-beta", "beta"),
    choice("v2", "0.6.1", "release"),
    choice("v1", "0.6.0", "release", true),
  ];

  it("the version of a Modrinth mod opens the compatible versions and choosing one changes it", async () => {
    vi.mocked(listContentVersions).mockResolvedValue(choices);
    const { onChangeVersion } = mount([sodium], {});
    const trigger = screen.getByRole("button", { name: "Change the version of Sodium (now 0.6.0)" });
    expect(trigger.textContent).toContain("0.6.0");

    fireEvent.click(trigger);
    const list = await screen.findByRole("listbox", { name: "Versions of Sodium" });
    expect(listContentVersions).toHaveBeenCalledWith("i1", "sodium.jar");
    const [newest, release, current] = within(list).getAllByRole("option") as [HTMLElement, HTMLElement, HTMLElement];
    const options = [newest, release, current];
    // 新しい順。種類の札と、いまの版の印を出す
    expect(options.map((o) => o.textContent)).toEqual([
      expect.stringContaining("0.7.0-beta"),
      expect.stringContaining("0.6.1"),
      expect.stringContaining("0.6.0"),
    ]);
    expect(newest.textContent).toContain("Beta");
    expect(current.textContent).toContain("Current");
    // いまの版は選べない
    expect(current.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(current);
    expect(onChangeVersion).not.toHaveBeenCalled();

    fireEvent.click(release);
    expect(onChangeVersion).toHaveBeenCalledWith(sodium, choices[1]);
    // 選んだら閉じる（確認のダイアログは出さない）
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
    expect(screen.queryByRole("dialog", { name: /Change|Confirm/ })).toBeNull();
  });

  it("the version trigger is disabled while busy", () => {
    mount([sodium], {}, true);
    const trigger = screen.getByRole("button", { name: "Change the version of Sodium (now 0.6.0)" });
    expect(trigger.hasAttribute("disabled")).toBe(true);
    fireEvent.click(trigger);
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(listContentVersions).not.toHaveBeenCalled();
  });

  it("a mod that Modrinth did not identify shows its version as plain text", () => {
    const local = item({ provider: "local", projectId: null, versionId: null, versionNumber: "1.2.3" });
    mount([local], {});
    expect(screen.getByText("1.2.3")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Change the version/ })).toBeNull();
  });

  it("shows no version picker without update state", () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ContentList items={[sodium]} onToggle={vi.fn()} onRemove={vi.fn()} busy={false} />
      </QueryClientProvider>,
    );
    expect(screen.queryByRole("button", { name: /Change the version/ })).toBeNull();
    expect(screen.getByText("0.6.0")).toBeTruthy();
  });

  it("the full version name of the trigger is shown in a tooltip", async () => {
    const long = "mc1.21.5-0.6.13-fabric-build.1234+very-long-suffix";
    mount([item({ versionNumber: long })], {});
    const trigger = screen.getByRole("button", { name: `Change the version of Sodium (now ${long})` });
    // 長い版の名前は行の中では切り詰め、ツールチップで全体を見せる
    expect(trigger.querySelector(".truncate")).not.toBeNull();
    fireEvent.keyDown(document.body, { key: "Tab" });
    act(() => trigger.focus());
    expect((await screen.findByRole("tooltip")).textContent).toBe(long);
  });

  it("the full version name of an option is shown in a tooltip while it is focused", async () => {
    const long = "mc1.21.5-0.7.0-beta.3+build.987654-extra-long";
    vi.mocked(listContentVersions).mockResolvedValue([choice("v9", long, "beta"), choices[2]!]);
    mount([sodium], {});
    fireEvent.click(screen.getByRole("button", { name: "Change the version of Sodium (now 0.6.0)" }));
    const list = await screen.findByRole("listbox", { name: "Versions of Sodium" });
    // キーボードで先頭の版に移る
    const first = within(list).getAllByRole("option")[0]!;
    fireEvent.keyDown(document.body, { key: "Tab" });
    act(() => first.focus());
    // 開いている一覧の外は読み上げから隠れるので、隠れたものも含めて探す（読み上げは選択肢の文字で全体が分かる）
    const tooltip = await screen.findByRole("tooltip", { hidden: true });
    expect(tooltip.textContent).toBe(long);
    expect(within(list).getAllByRole("option")[0]!.textContent).toContain(long);
  });

  it("a pinned mod shows a pin that explains updates are not offered", async () => {
    mount([item({ pinned: true }), iris], {});
    const message = "Fixed at this version, so updates aren't offered. Choose the newest release in the version list to unpin it.";
    const pin = within(row("Sodium")).getByRole("img", { name: message });
    expect(within(row("Iris")).queryByRole("img", { name: message })).toBeNull();
    fireEvent.keyDown(document.body, { key: "Tab" });
    act(() => pin.focus());
    expect((await screen.findByRole("tooltip")).textContent).toBe(message);
  });

  it("a failed version list shows the reason in the popover", async () => {
    vi.mocked(listContentVersions).mockRejectedValue({ code: "net.offline", params: {}, detail: null });
    mount([sodium], {});
    fireEvent.click(screen.getByRole("button", { name: "Change the version of Sodium (now 0.6.0)" }));
    expect(await screen.findByRole("alert")).toBeTruthy();
  });
});

describe("ContentRow icon", () => {
  it("the icon and the placeholder sit in the same square frame", () => {
    const withIcon = item({ iconUrl: "https://cdn.modrinth.com/data/p1/icon.png" });
    const withoutIcon = item({ relPath: "plain.jar", fileName: "plain.jar", sha1: "s9", name: "Plain", iconUrl: null });
    mount([withIcon, withoutIcon], {});
    for (const name of ["Sodium", "Plain"]) {
      const frame = within(row(name)).getByTestId("content-icon");
      // 枠が大きさを決める（正方形、縮まない、はみ出しは切る）
      for (const c of ["size-9", "aspect-square", "shrink-0", "overflow-hidden"]) expect(frame.classList).toContain(c);
    }
    const img = within(row("Sodium")).getByTestId("content-icon").querySelector("img")!;
    expect(img.classList).toContain("size-full");
    expect(img.classList).toContain("object-cover");
    expect(within(row("Plain")).getByTestId("content-icon").querySelector("img")).toBeNull();
  });
});
