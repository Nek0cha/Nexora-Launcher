import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { getContentIcon } from "../../api/commands";
import type { ContentItem } from "../../bindings/ContentItem";
import i18n from "../../i18n";
import { ContentList } from "./ContentList";

vi.mock("../../api/commands", () => ({
  getContentIcon: vi.fn().mockResolvedValue("data:image/png;base64,AAAA"),
}));

function item(over: Partial<ContentItem> = {}): ContentItem {
  return {
    kind: "mod", relPath: "sodium.jar", fileName: "sodium.jar", enabled: true, size: 1, sha1: "s1",
    provider: "modrinth", projectId: "p1", versionId: "v1", pinned: false, installedAs: "explicit",
    name: "Sodium", versionNumber: "0.6.0", description: "Fast rendering", iconUrl: null, hasLocalIcon: false,
    clientSide: "required", serverSide: "unsupported", categories: [], dependencies: [], identified: true,
    ...over,
  };
}

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

function mount(items: ContentItem[], busy = false) {
  const onToggle = vi.fn<(item: ContentItem) => void>();
  const onRemove = vi.fn<(item: ContentItem) => void>();
  const client = new QueryClient();
  render(
    <QueryClientProvider client={client}>
      <ContentList items={items} onToggle={onToggle} onRemove={onRemove} busy={busy} />
    </QueryClientProvider>,
  );
  return { onToggle, onRemove };
}

describe("ContentList", () => {
  it("renders only the visible rows for 500 items", () => {
    const items = Array.from({ length: 500 }, (_, i) =>
      item({ relPath: `m${i}.jar`, fileName: `m${i}.jar`, sha1: `h${i}`, name: `Mod ${i}` }));
    mount(items);
    const rows = screen.getAllByRole("row");
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThan(50);
    expect(screen.getByRole("grid").getAttribute("aria-rowcount")).toBe("500");
  });

  it("dims a disabled row", () => {
    mount([item(), item({ relPath: "iris.jar.disabled", fileName: "iris.jar.disabled", sha1: "s2", name: "Iris", enabled: false })]);
    const [on, off] = screen.getAllByRole("row") as [HTMLElement, HTMLElement];
    // 薄くするのは行の中身（アイコンと文字）で、操作のボタンは薄くしない
    expect(off.querySelector(".opacity-50")).not.toBeNull();
    expect(on.querySelector(".opacity-50")).toBeNull();
  });

  it("the toggle reports aria-pressed and calls onToggle", () => {
    const disabled = item({ relPath: "iris.jar.disabled", sha1: "s2", name: "Iris", enabled: false });
    const { onToggle } = mount([item(), disabled]);
    const on = screen.getByRole("button", { name: "Enable Sodium" });
    const off = screen.getByRole("button", { name: "Enable Iris" });
    expect(on.getAttribute("aria-pressed")).toBe("true");
    expect(off.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(off);
    expect(onToggle).toHaveBeenCalledWith(disabled);
  });

  it("the remove button calls onRemove", () => {
    const target = item();
    const { onRemove } = mount([target]);
    fireEvent.click(screen.getByRole("button", { name: "Remove Sodium" }));
    expect(onRemove).toHaveBeenCalledWith(target);
  });

  it("controls are disabled while busy", () => {
    const { onToggle, onRemove } = mount([item()], true);
    const toggle = screen.getByRole("button", { name: "Enable Sodium" });
    const remove = screen.getByRole("button", { name: "Remove Sodium" });
    expect(toggle.hasAttribute("disabled")).toBe(true);
    expect(remove.hasAttribute("disabled")).toBe(true);
    fireEvent.click(toggle);
    fireEvent.click(remove);
    expect(onToggle).not.toHaveBeenCalled();
    expect(onRemove).not.toHaveBeenCalled();
  });

  it("shows the file name when the item has no name", () => {
    mount([item({ name: null, versionNumber: null, description: null, fileName: "mystery.jar" })]);
    const row = screen.getByRole("row");
    expect(within(row).getByText("mystery.jar")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Enable mystery.jar" })).toBeTruthy();
  });

  it("marks a dependency", () => {
    mount([item(), item({ relPath: "fapi.jar", sha1: "s3", name: "Fabric API", installedAs: "dependency" })]);
    const [plain, dep] = screen.getAllByRole("row") as [HTMLElement, HTMLElement];
    expect(within(dep).getByText("Dependency")).toBeTruthy();
    expect(within(plain).queryByText("Dependency")).toBeNull();
  });

  it("loads a local icon only when there is no icon URL", async () => {
    mount([
      item({ sha1: "local", hasLocalIcon: true }),
      item({ relPath: "b.jar", sha1: "remote", name: "B", iconUrl: "https://cdn.example/b.png", hasLocalIcon: true }),
    ]);
    await waitFor(() => expect(document.querySelector("img[src^='data:']")).not.toBeNull());
    expect(document.querySelector("img[src='https://cdn.example/b.png']")).not.toBeNull();
    expect(vi.mocked(getContentIcon).mock.calls).toEqual([["local"]]);
  });

  it("falls back to the local icon when the icon URL fails to load", async () => {
    vi.mocked(getContentIcon).mockClear();
    mount([item({ sha1: "offline", iconUrl: "https://cdn.example/a.png", hasLocalIcon: true })]);
    const remote = document.querySelector("img[src='https://cdn.example/a.png']");
    expect(remote).not.toBeNull();
    expect(getContentIcon).not.toHaveBeenCalled();
    fireEvent.error(remote!);
    await waitFor(() => expect(document.querySelector("img[src^='data:']")).not.toBeNull());
    expect(vi.mocked(getContentIcon).mock.calls).toEqual([["offline"]]);
    // jar の中のアイコンも読めなければ、既定の絵に戻す
    fireEvent.error(document.querySelector("img[src^='data:']")!);
    await waitFor(() => expect(document.querySelector("img")).toBeNull());
  });
  it("keeps rendering when a row's file name changes after a toggle", () => {
    const client = new QueryClient();
    const props = { onToggle: vi.fn(), onRemove: vi.fn(), busy: false };
    const view = (items: ContentItem[]) => (
      <QueryClientProvider client={client}>
        <ContentList items={items} {...props} />
      </QueryClientProvider>
    );
    const { rerender } = render(view([item({ relPath: "mods/sodium.jar", fileName: "sodium.jar" })]));
    // 無効にするとファイル名（= 行の id）が変わる。同じ位置の行で id が変わっても落ちないこと
    rerender(view([item({ relPath: "mods/sodium.jar.disabled", fileName: "sodium.jar.disabled", enabled: false })]));
    expect(screen.getByText("Sodium")).toBeTruthy();
  });

  it("keeps the same row and icon nodes across a toggle (no remount)", () => {
    const client = new QueryClient();
    const props = { onToggle: vi.fn(), onRemove: vi.fn(), busy: false };
    const view = (items: ContentItem[]) => (
      <QueryClientProvider client={client}>
        <ContentList items={items} {...props} />
      </QueryClientProvider>
    );
    const icon = "https://cdn.example/sodium.png";
    const other = item({ relPath: "mods/iris.jar", fileName: "iris.jar", sha1: "s2", name: "Iris" });
    const { rerender } = render(view([item({ relPath: "mods/sodium.jar", iconUrl: icon }), other]));
    const row = screen.getByRole("button", { name: "Enable Sodium" }).closest("[role=row]");
    const img = document.querySelector(`img[src='${icon}']`);
    expect(img).not.toBeNull();
    // キーボードで切り替えた直後も、フォーカスは同じトグルに残る
    fireEvent.keyDown(document.body, { key: "Tab" });
    act(() => screen.getByRole("button", { name: "Enable Sodium" }).focus());
    const disabled = item({ relPath: "mods/sodium.jar.disabled", fileName: "sodium.jar.disabled", enabled: false, iconUrl: icon });
    rerender(view([disabled, other]));
    const toggle = screen.getByRole("button", { name: "Enable Sodium" });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(toggle.closest("[role=row]")).toBe(row);
    expect(document.querySelector(`img[src='${icon}']`)).toBe(img);
    expect(document.activeElement).toBe(toggle);
    // 押したときは、いまの（名前の変わった）項目を渡す
    fireEvent.click(toggle);
    expect(props.onToggle).toHaveBeenCalledWith(disabled);
  });

  it("disables only the toggle of a row that is being toggled", () => {
    const client = new QueryClient();
    const items = [
      item({ relPath: "mods/sodium.jar.disabled", fileName: "sodium.jar.disabled", enabled: false }),
      item({ relPath: "mods/iris.jar", fileName: "iris.jar", sha1: "s2", name: "Iris" }),
    ];
    render(
      <QueryClientProvider client={client}>
        <ContentList items={items} onToggle={vi.fn()} onRemove={vi.fn()} busy={false} toggling={new Set(["mods/sodium.jar"])} />
      </QueryClientProvider>,
    );
    expect(screen.getByRole("button", { name: "Enable Sodium" }).hasAttribute("disabled")).toBe(true);
    expect(screen.getByRole("button", { name: "Enable Iris" }).hasAttribute("disabled")).toBe(false);
    expect(screen.getByRole("button", { name: "Remove Iris" }).hasAttribute("disabled")).toBe(false);
  });

  it("does not collide when both the enabled and the disabled file exist", () => {
    mount([
      item({ relPath: "mods/sodium.jar", fileName: "sodium.jar" }),
      item({ relPath: "mods/sodium.jar.disabled", fileName: "sodium.jar.disabled", sha1: "s2", name: "Sodium old", enabled: false }),
    ]);
    expect(screen.getAllByRole("row")).toHaveLength(2);
    expect(screen.getByText("Sodium old")).toBeTruthy();
  });
});
