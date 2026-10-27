import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  deleteSyncProfile,
  getSyncProfileValues,
  leaveSyncProfile,
  pendingSyncProposals,
  renameSyncProfile,
  setSyncCategories,
} from "../../api/commands";
import type { SyncMember } from "../../bindings/SyncMember";
import type { SyncProfile } from "../../bindings/SyncProfile";
import i18n from "../../i18n";
import { ProfileCard } from "./ProfileCard";

vi.mock("../../api/commands", () => ({
  renameSyncProfile: vi.fn(),
  setSyncCategories: vi.fn(),
  leaveSyncProfile: vi.fn(),
  deleteSyncProfile: vi.fn(),
  pendingSyncProposals: vi.fn(),
  getSyncProfileValues: vi.fn(),
  resolveSyncProposal: vi.fn(),
}));

const profile: SyncProfile = {
  id: "p1",
  name: "いつもの",
  revision: 2,
  categories: ["keys", "graphics"],
  members: [
    { id: "i1", name: "Survival" },
    { id: "i2", name: "Creative" },
  ],
};

const onJoin = vi.fn();

function mount({ p = profile, unjoined = [{ id: "i3", name: "Modded" }] as SyncMember[] } = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ProfileCard profile={p} unjoined={unjoined} onJoin={onJoin} />
    </QueryClientProvider>,
  );
}

const press = (el: HTMLElement) =>
  act(async () => {
    fireEvent.click(el);
  });

beforeEach(async () => {
  await i18n.changeLanguage("ja");
  vi.mocked(renameSyncProfile).mockResolvedValue(null);
  vi.mocked(setSyncCategories).mockResolvedValue(null);
  vi.mocked(leaveSyncProfile).mockResolvedValue(null);
  vi.mocked(deleteSyncProfile).mockResolvedValue(null);
  vi.mocked(pendingSyncProposals).mockResolvedValue({ single: [], conflicts: [] });
  vi.mocked(getSyncProfileValues).mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("ProfileCard", () => {
  it("shows the name, the categories that are on and the instances that joined", () => {
    mount();
    const card = screen.getByRole("region", { name: "いつもの" });
    expect((within(card).getByRole("textbox", { name: "共通設定の名前" }) as HTMLInputElement).value).toBe("いつもの");
    const pressed = (name: string) => within(card).getByRole("button", { name }).getAttribute("aria-pressed");
    expect(pressed("キー割り当て")).toBe("true");
    expect(pressed("グラフィック")).toBe("true");
    expect(pressed("マウス")).toBe("false");
    expect(pressed("音量")).toBe("false");
    expect(pressed("チャット")).toBe("false");
    expect(pressed("アクセシビリティ")).toBe("false");
    expect(pressed("言語")).toBe("false");
    expect(within(card).getByText("Survival")).toBeTruthy();
    expect(within(card).getByText("Creative")).toBeTruthy();
    expect(within(card).queryByText("Modded")).toBeNull();
  });

  it("has no proposal badge when no instance changed anything", async () => {
    mount();
    await waitFor(() => expect(pendingSyncProposals).toHaveBeenCalledWith("p1"));
    await act(async () => {});
    expect(screen.queryByTestId("proposal-badge")).toBeNull();
    expect(document.body.textContent).not.toContain("変えた設定");
  });

  it("says how many settings the instances changed in game", async () => {
    vi.mocked(pendingSyncProposals).mockResolvedValue({
      single: [{ instanceIds: ["i1"], fieldId: "fov", value: "0.5" }],
      conflicts: [{ fieldId: "gamma", entries: [{ instanceId: "i1", value: "0.1" }, { instanceId: "i2", value: "0.9" }] }],
    });
    mount();
    const card = screen.getByRole("region", { name: "いつもの" });
    expect(await within(card).findByText("参加中のインスタンスで変えた設定が 2 件あります")).toBeTruthy();
    expect(within(card).getByTestId("proposal-badge").textContent).toBe("2");
  });

  it("never shows the words of the mechanism", () => {
    mount();
    const text = document.body.textContent ?? "";
    for (const word of ["同期プロファイル", "options.txt", "DataVersion", "codec"]) expect(text).not.toContain(word);
  });

  it("joins an instance picked from the add menu", async () => {
    mount({ unjoined: [{ id: "i3", name: "Modded" }, { id: "i4", name: "Hardcore" }] });
    await press(screen.getByRole("button", { name: "インスタンスを追加" }));
    const menu = await screen.findByRole("menu");
    expect(within(menu).getAllByRole("menuitem").map((m) => m.textContent)).toEqual(["Modded", "Hardcore"]);
    await press(within(menu).getByRole("menuitem", { name: "Hardcore" }));
    expect(onJoin).toHaveBeenCalledWith("i4", "p1");
  });

  it("disables the add button and says why when every instance has joined", () => {
    mount({ unjoined: [] });
    const button = screen.getByRole("button", { name: "インスタンスを追加" });
    expect(button.hasAttribute("disabled")).toBe(true);
    expect(screen.getByText("追加できる未参加のインスタンスはありません。")).toBeTruthy();
  });

  it("removes an instance with the button on its chip", async () => {
    mount();
    await press(screen.getByRole("button", { name: "Creative を外す" }));
    await waitFor(() => expect(leaveSyncProfile).toHaveBeenCalledWith("i2"));
  });

  it("says so when no instance has joined", () => {
    mount({ p: { ...profile, members: [] } });
    expect(screen.getByText("まだ参加しているインスタンスはありません。")).toBeTruthy();
  });

  it("renames in place when Enter is pressed", async () => {
    mount();
    const input = screen.getByRole("textbox", { name: "共通設定の名前" });
    fireEvent.change(input, { target: { value: "  PvP 用  " } });
    await act(async () => {
      fireEvent.keyDown(input, { key: "Enter" });
    });
    await waitFor(() => expect(renameSyncProfile).toHaveBeenCalledWith("p1", "PvP 用"));
  });

  it("sends the name once when Enter is followed by leaving the field", async () => {
    mount();
    const input = screen.getByRole("textbox", { name: "共通設定の名前" });
    fireEvent.change(input, { target: { value: "建築" } });
    await act(async () => {
      fireEvent.keyDown(input, { key: "Enter" });
      fireEvent.blur(input);
    });
    await waitFor(() => expect(renameSyncProfile).toHaveBeenCalledTimes(1));
  });

  it("renames when the field loses focus", async () => {
    mount();
    const input = screen.getByRole("textbox", { name: "共通設定の名前" });
    fireEvent.change(input, { target: { value: "建築" } });
    await act(async () => {
      fireEvent.blur(input);
    });
    await waitFor(() => expect(renameSyncProfile).toHaveBeenCalledWith("p1", "建築"));
  });

  it("puts the old name back for an empty name or Escape", async () => {
    mount();
    const input = screen.getByRole("textbox", { name: "共通設定の名前" }) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "   " } });
    await act(async () => {
      fireEvent.blur(input);
    });
    expect(input.value).toBe("いつもの");
    fireEvent.change(input, { target: { value: "別の名前" } });
    await act(async () => {
      fireEvent.keyDown(input, { key: "Escape" });
    });
    expect(input.value).toBe("いつもの");
    await act(async () => {
      fireEvent.blur(input);
    });
    expect(renameSyncProfile).not.toHaveBeenCalled();
  });

  it("turns a category on and keeps the categories in a fixed order", async () => {
    mount();
    await press(screen.getByRole("button", { name: "マウス" }));
    await waitFor(() => expect(setSyncCategories).toHaveBeenCalledWith("p1", ["keys", "mouse", "graphics"]));
  });

  it("turns the language category on", async () => {
    mount();
    await press(screen.getByRole("button", { name: "言語" }));
    await waitFor(() => expect(setSyncCategories).toHaveBeenCalledWith("p1", ["keys", "graphics", "language"]));
  });

  it("turns a category off", async () => {
    mount();
    await press(screen.getByRole("button", { name: "キー割り当て" }));
    await waitFor(() => expect(setSyncCategories).toHaveBeenCalledWith("p1", ["graphics"]));
  });

  it("asks before deleting and says the members go back and the files stay", async () => {
    mount();
    await press(screen.getByRole("button", { name: "いつもの を削除" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText("「いつもの」を削除しますか？")).toBeTruthy();
    expect(dialog.textContent).toContain("参加中のインスタンスは未参加に戻ります。設定ファイルは変わりません。");
    expect(deleteSyncProfile).not.toHaveBeenCalled();
    await press(within(dialog).getByRole("button", { name: "削除" }));
    await waitFor(() => expect(deleteSyncProfile).toHaveBeenCalledWith("p1"));
  });

  it("does not delete when the confirmation is cancelled", async () => {
    mount();
    await press(screen.getByRole("button", { name: "いつもの を削除" }));
    const dialog = await screen.findByRole("alertdialog");
    await press(within(dialog).getByRole("button", { name: "キャンセル" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteSyncProfile).not.toHaveBeenCalled();
  });

  it("lays the category switches out in a wrapping row and never wraps inside one", () => {
    mount();
    const buttons = screen.getAllByRole("button").filter((b) => b.hasAttribute("aria-pressed"));
    expect(buttons).toHaveLength(7);
    expect((buttons[0]?.parentElement as HTMLElement).className).toContain("flex-wrap");
    for (const b of buttons) {
      expect(b.className).toContain("whitespace-nowrap");
      expect(b.className).toContain("shrink-0");
    }
  });
});
