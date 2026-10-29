import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  getInstanceSync,
  getSyncProfileValues,
  joinSyncProfile,
  leaveSyncProfile,
  listInstances,
  listRunning,
  listSyncProfiles,
  pendingSyncProposals,
  previewJoinSyncProfile,
  resolveSyncProposal,
  setSyncCategories,
} from "../api/commands";
import type { Instance } from "../bindings/Instance";
import type { InstanceSync } from "../bindings/InstanceSync";
import type { SyncProfile } from "../bindings/SyncProfile";
import { clearToasts, useToastList } from "../components/toast/useToast";
import i18n from "../i18n";
import SyncProfiles from "./SyncProfiles";

vi.mock("../api/commands", () => ({
  listInstances: vi.fn(),
  listRunning: vi.fn(),
  listSyncProfiles: vi.fn(),
  getInstanceSync: vi.fn(),
  createSyncProfileFromInstance: vi.fn(),
  previewJoinSyncProfile: vi.fn(),
  joinSyncProfile: vi.fn(),
  leaveSyncProfile: vi.fn(),
  renameSyncProfile: vi.fn(),
  setSyncCategories: vi.fn(),
  deleteSyncProfile: vi.fn(),
  pendingSyncProposals: vi.fn(),
  getSyncProfileValues: vi.fn(),
  resolveSyncProposal: vi.fn(),
}));

const instance = (id: string, name: string): Instance => ({
  id, name, dirName: name, mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
});

const instances = [instance("i1", "Survival"), instance("i2", "Creative"), instance("i3", "Modded")];

const profile = (id: string, name: string, members: string[]): SyncProfile => ({
  id,
  name,
  revision: 1,
  categories: ["keys"],
  members: members.map((m) => ({ id: m, name: instances.find((i) => i.id === m)?.name ?? m })),
});

const notJoined: InstanceSync = {
  profile: null, members: [], lastApplied: null, pendingCount: 0, profilePendingCount: 0, excluded: [],
  hasOptions: true, applicability: "ready", upToDate: false, overrides: [],
};

function Toasts() {
  const list = useToastList();
  return (
    <ul data-testid="toasts">
      {list.map((t) => (
        <li key={t.id}>{t.message}</li>
      ))}
    </ul>
  );
}

function mount({
  list = instances,
  profiles = [] as SyncProfile[],
  running = [] as string[],
} = {}) {
  vi.mocked(listInstances).mockResolvedValue(list);
  vi.mocked(listSyncProfiles).mockResolvedValue(profiles);
  vi.mocked(listRunning).mockResolvedValue(running.map((id) => ({ instanceId: id }) as never));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <SyncProfiles />
        <Toasts />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const press = (el: HTMLElement) =>
  act(async () => {
    fireEvent.click(el);
  });

const toastText = () => screen.getByTestId("toasts").textContent ?? "";

beforeEach(async () => {
  await i18n.changeLanguage("ja");
  clearToasts();
  vi.mocked(getInstanceSync).mockResolvedValue(notJoined);
  vi.mocked(previewJoinSyncProfile).mockResolvedValue(0);
  vi.mocked(joinSyncProfile).mockResolvedValue({ differing: 0 });
  vi.mocked(leaveSyncProfile).mockResolvedValue(null);
  vi.mocked(pendingSyncProposals).mockResolvedValue({ single: [], conflicts: [] });
  vi.mocked(getSyncProfileValues).mockResolvedValue([]);
  vi.mocked(resolveSyncProposal).mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  document.body.style.cursor = "";
});

describe("SyncProfiles empty states", () => {
  it("asks to create an instance first when there is none", async () => {
    mount({ list: [] });
    expect(await screen.findByRole("button", { name: "インスタンスを作成" })).toBeTruthy();
    // 説明の文は置かず、操作だけを出す
    expect(screen.queryByText(/揃える仕組み/)).toBeNull();
    expect(screen.getByRole("button", { name: "インスタンスを作成" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "共通設定を作る" })).toBeNull();
  });

  it("explains and offers to create one when there are no shared settings", async () => {
    mount();
    expect(await screen.findByRole("button", { name: "共通設定を作る" })).toBeTruthy();
    expect(screen.queryByText(/元にするインスタンスを選んで/)).toBeNull();
    await press(screen.getByRole("button", { name: "共通設定を作る" }));
    const menu = await screen.findByRole("menu");
    expect(within(menu).getAllByRole("menuitem").map((m) => m.textContent)).toEqual(["Survival", "Creative", "Modded"]);
  });

  it("opens the dialog for the chosen instance and shows why it cannot be the source", async () => {
    vi.mocked(getInstanceSync).mockResolvedValue({ ...notJoined, hasOptions: false });
    mount();
    await press(await screen.findByRole("button", { name: "共通設定を作る" }));
    await press(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Creative" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Creative の設定を揃える")).toBeTruthy();
    const create = within(dialog).getByRole("radio", { name: "このインスタンスの設定から新しい共通設定を作る" });
    expect((create as HTMLInputElement).disabled).toBe(true);
    expect(within(dialog).getByText("一度起動すると、設定を読み取れるようになります。")).toBeTruthy();
    expect(getInstanceSync).toHaveBeenCalledWith("i2");
  });
});

describe("SyncProfiles", () => {
  const two = [profile("p1", "いつもの", ["i1"]), profile("p2", "PvP", [])];

  it("shows a card per shared setting and only the instances that have not joined on the side", async () => {
    mount({ profiles: two });
    expect(await screen.findByRole("region", { name: "いつもの" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "PvP" })).toBeTruthy();
    const side = screen.getByRole("complementary", { name: "未参加のインスタンス" });
    expect(within(side).queryByText("Survival")).toBeNull();
    expect(within(side).getByText("Creative")).toBeTruthy();
    expect(within(side).getByText("Modded")).toBeTruthy();
    // 上にも脇にも、ドラッグや反映の時期を説明する文は置かない
    expect(screen.queryByText(/次の起動時に反映されます。/)).toBeNull();
    expect(within(side).queryByText("カードへドラッグすると参加します。")).toBeNull();
  });

  it("says when every instance has joined", async () => {
    mount({ profiles: [profile("p1", "いつもの", ["i1", "i2", "i3"])] });
    const side = await screen.findByRole("complementary", { name: "未参加のインスタンス" });
    expect(within(side).getByText("すべてのインスタンスが、どれかの共通設定に参加しています。")).toBeTruthy();
  });

  it("joins from the add menu without asking when nothing differs", async () => {
    mount({ profiles: two });
    const card = await screen.findByRole("region", { name: "PvP" });
    await press(within(card).getByRole("button", { name: "インスタンスを追加" }));
    await press(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Modded" }));
    await waitFor(() => expect(joinSyncProfile).toHaveBeenCalledWith("i3", "p2", "keepProfile"));
    expect(previewJoinSyncProfile).toHaveBeenCalledWith("i3", "p2");
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(toastText()).toContain("Modded を「PvP」に参加させました。次の起動時に反映されます。"));
  });

  it("says the change waits for the end of the play when the instance is running", async () => {
    mount({ profiles: two, running: ["i3"] });
    const card = await screen.findByRole("region", { name: "PvP" });
    await press(within(card).getByRole("button", { name: "インスタンスを追加" }));
    await press(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Modded" }));
    await waitFor(() => expect(toastText()).toContain("いまのプレイが終わった後に反映されます。"));
  });

  it("asks which side to keep when settings differ", async () => {
    vi.mocked(previewJoinSyncProfile).mockResolvedValue(3);
    mount({ profiles: two });
    const card = await screen.findByRole("region", { name: "いつもの" });
    await press(within(card).getByRole("button", { name: "インスタンスを追加" }));
    await press(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Creative" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("3 件の設定が違います");
    expect(joinSyncProfile).not.toHaveBeenCalled();
    await press(within(dialog).getByRole("radio", { name: "このインスタンスの値で共通設定を更新する" }));
    await press(within(dialog).getByRole("button", { name: "参加する" }));
    await waitFor(() => expect(joinSyncProfile).toHaveBeenCalledWith("i2", "p1", "adoptInstance"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("cancels the join from the question about differences", async () => {
    vi.mocked(previewJoinSyncProfile).mockResolvedValue(2);
    mount({ profiles: two });
    const card = await screen.findByRole("region", { name: "いつもの" });
    await press(within(card).getByRole("button", { name: "インスタンスを追加" }));
    await press(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Creative" }));
    const dialog = await screen.findByRole("dialog");
    await press(within(dialog).getByRole("button", { name: "キャンセル" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(joinSyncProfile).not.toHaveBeenCalled();
  });

  describe("dragging", () => {
    let hit: Element | null = null;
    beforeEach(() => {
      document.elementFromPoint = vi.fn(() => hit);
    });

    const drag = async (from: HTMLElement, to: Element | null) => {
      hit = to;
      await act(async () => {
        fireEvent.pointerDown(from, { button: 0, clientX: 10, clientY: 10, pointerId: 1 });
        fireEvent.pointerMove(window, { clientX: 40, clientY: 40, pointerId: 1, buttons: 1 });
      });
      await act(async () => {
        fireEvent.pointerUp(window, { clientX: 40, clientY: 40, pointerId: 1, buttons: 1 });
      });
    };

    it("joins when a chip is dragged onto a card", async () => {
      mount({ profiles: two });
      const card = await screen.findByRole("region", { name: "PvP" });
      const side = screen.getByRole("complementary", { name: "未参加のインスタンス" });
      await drag(within(side).getByText("Modded"), within(card).getByText("参加中のインスタンス"));
      await waitFor(() => expect(joinSyncProfile).toHaveBeenCalledWith("i3", "p2", "keepProfile"));
    });

    it("leaves when a member is dragged out of its card", async () => {
      mount({ profiles: two });
      const card = await screen.findByRole("region", { name: "いつもの" });
      const side = screen.getByRole("complementary", { name: "未参加のインスタンス" });
      await drag(within(card).getByText("Survival"), side);
      await waitFor(() => expect(leaveSyncProfile).toHaveBeenCalledWith("i1"));
    });

    it("shows the dragged chip and where it will land while dragging", async () => {
      mount({ profiles: two });
      const card = await screen.findByRole("region", { name: "PvP" });
      const side = screen.getByRole("complementary", { name: "未参加のインスタンス" });
      hit = card;
      await act(async () => {
        fireEvent.pointerDown(within(side).getByText("Modded"), { button: 0, clientX: 10, clientY: 10, pointerId: 1 });
        fireEvent.pointerMove(window, { clientX: 40, clientY: 40, pointerId: 1, buttons: 1 });
      });
      expect(card.getAttribute("data-drop-active")).toBe("true");
      expect(screen.getByTestId("drag-ghost").textContent).toContain("Modded");
      expect(screen.getByTestId("drag-ghost").textContent).toContain("ここで離すと参加します。");
      await act(async () => {
        fireEvent.keyDown(window, { key: "Escape" });
      });
      expect(screen.queryByTestId("drag-ghost")).toBeNull();
      await act(async () => {
        fireEvent.pointerUp(window, { clientX: 40, clientY: 40, pointerId: 1, buttons: 1 });
      });
      expect(joinSyncProfile).not.toHaveBeenCalled();
      expect(previewJoinSyncProfile).not.toHaveBeenCalled();
    });

    it("does nothing for a click without movement", async () => {
      mount({ profiles: two });
      const side = await screen.findByRole("complementary", { name: "未参加のインスタンス" });
      hit = screen.getByRole("region", { name: "PvP" });
      await act(async () => {
        fireEvent.pointerDown(within(side).getByText("Modded"), { button: 0, clientX: 10, clientY: 10, pointerId: 1 });
        fireEvent.pointerUp(window, { clientX: 11, clientY: 10, pointerId: 1 });
      });
      expect(previewJoinSyncProfile).not.toHaveBeenCalled();
    });
  });
});

describe("SyncProfiles proposals", () => {
  it("reviews the changes made in game on the card of the shared settings", async () => {
    vi.mocked(pendingSyncProposals).mockImplementation(async (profileId) =>
      profileId === "p1"
        ? { single: [{ instanceIds: ["i2"], fieldId: "key_key.jump", value: "key.keyboard.j" }], conflicts: [] }
        : { single: [], conflicts: [] },
    );
    vi.mocked(getSyncProfileValues).mockResolvedValue([{ fieldId: "key_key.jump", value: "key.keyboard.space" }]);
    mount({ profiles: [profile("p1", "いつもの", ["i1", "i2"]), profile("p2", "別", ["i3"])] });
    const card = await screen.findByRole("region", { name: "いつもの" });
    expect(await within(card).findByText("参加中のインスタンスで変えた設定が 1 件あります")).toBeTruthy();
    const other = screen.getByRole("region", { name: "別" });
    expect(within(other).queryByTestId("proposal-badge")).toBeNull();
    await press(within(card).getByRole("button", { name: "確認する" }));
    const row = await within(card).findByRole("listitem", { name: "ジャンプ" });
    expect(row.textContent).toContain("スペース");
    expect(row.textContent).toContain("J");
    expect(row.textContent).toContain("Creative");
    await press(within(row).getByRole("button", { name: "全部に反映する" }));
    await waitFor(() =>
      expect(resolveSyncProposal).toHaveBeenCalledWith("i2", "key_key.jump", "applyToProfile", undefined),
    );
  });
});

describe("SyncProfiles category chips", () => {
  it("keeps a chip in its new state until the profiles are fetched again, so it does not flash back", async () => {
    const p = profile("p1", "いつもの", ["i1"]);
    vi.mocked(setSyncCategories).mockResolvedValue(null);
    mount({ profiles: [p] });
    const card = await screen.findByRole("region", { name: "いつもの" });
    const chip = within(card).getByRole("button", { name: "マウス" });
    expect(chip.getAttribute("aria-pressed")).toBe("false");
    // 保存の後の取り直しを止めておく
    let release: (v: SyncProfile[]) => void = () => {};
    vi.mocked(listSyncProfiles).mockReturnValue(new Promise((r) => (release = r)));
    const seen: (string | null)[] = [];
    const observer = new MutationObserver(() => seen.push(chip.getAttribute("aria-pressed")));
    observer.observe(chip, { attributes: true, attributeFilter: ["aria-pressed"] });
    await press(chip);
    await waitFor(() => expect(setSyncCategories).toHaveBeenCalled());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });
    expect(chip.getAttribute("aria-pressed")).toBe("true");
    await act(async () => {
      release([{ ...p, categories: ["keys", "mouse"] }]);
    });
    observer.disconnect();
    // 押した後は true になるだけで、false に戻らない
    expect(seen).not.toContain("false");
    expect(chip.getAttribute("aria-pressed")).toBe("true");
  });
});
