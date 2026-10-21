import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AvailableUpdate } from "../../bindings/AvailableUpdate";
import type { ContentItem } from "../../bindings/ContentItem";
import i18n from "../../i18n";
import { ContentList, type RowUpdates } from "./ContentList";

vi.mock("../../api/commands", () => ({
  getContentIcon: vi.fn().mockResolvedValue(null),
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
});

function mount(items: ContentItem[], updates: Partial<RowUpdates>, busy = false) {
  const onUpdate = vi.fn<(item: ContentItem) => void>();
  const rowUpdates: RowUpdates = {
    available: new Map(),
    noCompatible: new Set(),
    updating: new Set(),
    onUpdate,
    ...updates,
  };
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ContentList items={items} onToggle={vi.fn()} onRemove={vi.fn()} busy={busy} updates={rowUpdates} />
    </QueryClientProvider>,
  );
  return { onUpdate };
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
