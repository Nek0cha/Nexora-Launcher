import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  getInstanceSync,
  leaveSyncProfile,
  listSyncFields,
  listSyncProfiles,
  setSyncOverride,
} from "../../api/commands";
import type { Instance } from "../../bindings/Instance";
import type { InstanceSync } from "../../bindings/InstanceSync";
import type { SyncProfile } from "../../bindings/SyncProfile";
import i18n from "../../i18n";
import { SyncCard } from "./SyncCard";

vi.mock("../../api/commands", () => ({
  getInstanceSync: vi.fn(),
  listSyncProfiles: vi.fn(),
  listSyncFields: vi.fn(),
  createSyncProfileFromInstance: vi.fn(),
  previewJoinSyncProfile: vi.fn(),
  joinSyncProfile: vi.fn(),
  leaveSyncProfile: vi.fn(),
  setSyncOverride: vi.fn(),
}));

const instance: Instance = {
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
};

const profile: SyncProfile = {
  id: "p1",
  name: "いつもの",
  revision: 3,
  categories: ["keys", "graphics"],
  members: [
    { id: "i1", name: "Survival" },
    { id: "i2", name: "Creative" },
    { id: "i3", name: "Modded" },
  ],
};

const notJoined: InstanceSync = {
  profile: null,
  members: [],
  lastApplied: null,
  pendingCount: 0,
  profilePendingCount: 0,
  excluded: [],
  hasOptions: true,
  applicability: "ready",
  upToDate: false,
  overrides: [],
};

const joined: InstanceSync = {
  ...notJoined,
  profile,
  members: profile.members,
  lastApplied: { state: "applied", atMs: Date.UTC(2026, 9, 25, 5, 3), reason: null },
  upToDate: true,
};

function mount(sync: InstanceSync, { running = false, profiles = [] as SyncProfile[] } = {}) {
  vi.mocked(getInstanceSync).mockResolvedValue(sync);
  vi.mocked(listSyncProfiles).mockResolvedValue(profiles);
  vi.mocked(listSyncFields).mockResolvedValue([
    { id: "fov", category: "graphics" },
    { id: "key_key.jump", category: "keys" },
  ]);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <SyncCard instance={instance} running={running} />
    </QueryClientProvider>,
  );
  return client;
}

const card = () => screen.getByRole("region", { name: "設定の同期" });

beforeEach(async () => {
  await i18n.changeLanguage("ja");
  vi.mocked(leaveSyncProfile).mockResolvedValue(null);
  vi.mocked(setSyncOverride).mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("SyncCard when not joined", () => {
  it("shows a single button to match settings", async () => {
    mount(notJoined);
    const button = await screen.findByRole("button", { name: "他のインスタンスと設定を揃える" });
    expect(button).toBeTruthy();
    expect(within(card()).getAllByRole("button")).toHaveLength(1);
    expect(card().textContent).not.toContain("共通設定から外れる");
  });

  it("opens the dialog with both choices when shared settings exist", async () => {
    mount(notJoined, { profiles: [profile] });
    fireEvent.click(await screen.findByRole("button", { name: "他のインスタンスと設定を揃える" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("radio", { name: "このインスタンスの設定から新しい共通設定を作る" })).toBeTruthy();
    expect(await within(dialog).findByRole("radio", { name: "既存の共通設定に参加する" })).toBeTruthy();
  });

  it("disables creating from an instance without settings and says why", async () => {
    mount({ ...notJoined, hasOptions: false, applicability: "needsFirstLaunch" }, { profiles: [profile] });
    fireEvent.click(await screen.findByRole("button", { name: "他のインスタンスと設定を揃える" }));
    const dialog = await screen.findByRole("dialog");
    const create = within(dialog).getByRole("radio", {
      name: "このインスタンスの設定から新しい共通設定を作る",
    }) as HTMLInputElement;
    expect(create.disabled).toBe(true);
    expect(dialog.textContent).toContain("一度起動すると、設定を読み取れるようになります。");
  });
});

describe("SyncCard when joined", () => {
  it("shows the name, the other members and the last time it was applied", async () => {
    mount(joined);
    expect(await screen.findByText("共通設定「いつもの」を使っています")).toBeTruthy();
    expect(card().textContent).toContain("一緒に使っているインスタンス：Creative、Modded");
    const date = new Intl.DateTimeFormat("ja", { dateStyle: "medium", timeStyle: "short" }).format(
      joined.lastApplied!.atMs!,
    );
    expect(card().textContent).toContain(`最後に反映：${date}`);
    // 反映済みなら「次の起動時に」とは言わない
    expect(card().textContent).not.toContain("次の起動時に反映されます");
  });

  it("says it applies on the next launch only when it is not up to date", async () => {
    mount({ ...joined, upToDate: false, lastApplied: null });
    expect(await screen.findByText("次の起動時に反映されます。")).toBeTruthy();
    expect(card().textContent).toContain("まだ反映していません。");
  });

  it("says it applies after the current session while the game runs", async () => {
    mount({ ...joined, upToDate: false }, { running: true });
    expect(await screen.findByText("いまのプレイが終わった後に反映されます。")).toBeTruthy();
    expect(card().textContent).not.toContain("次の起動時に反映されます");
  });

  it("says when no other instance uses the shared settings", async () => {
    mount({ ...joined, members: [{ id: "i1", name: "Survival" }] });
    expect(await screen.findByText("ほかに参加しているインスタンスはまだありません。")).toBeTruthy();
  });

  it.each([
    ["needsFirstLaunch", "一度起動すると、設定を読み取れるようになります。"],
    ["versionUnreadable", "設定ファイルか、ゲームのバージョンを読み取れません。"],
    ["versionMismatch", "設定ファイルが、このゲームのバージョンのものと合っていません。"],
  ] as const)("explains why it cannot apply (%s)", async (applicability, text) => {
    mount({ ...joined, applicability });
    await screen.findByText("共通設定「いつもの」を使っています");
    expect(card().textContent).toContain("このインスタンスにはまだ適用できません。");
    expect(card().textContent).toContain(text);
  });

  it("does not promise the next launch when it cannot apply", async () => {
    mount({ ...joined, upToDate: false, applicability: "needsFirstLaunch" });
    await screen.findByText("共通設定「いつもの」を使っています");
    expect(card().textContent).not.toContain("次の起動時に反映されます");
  });

  it("does not show a reason when it can apply", async () => {
    mount(joined);
    await screen.findByText("共通設定「いつもの」を使っています");
    expect(card().textContent).not.toContain("まだ適用できません");
  });

  it("shows why the last launch did not apply", async () => {
    mount({ ...joined, lastApplied: { state: "failed", atMs: 1, reason: "writeFailed" } });
    expect(
      await screen.findByText("前回の起動で反映に失敗しました。設定ファイルに書き込めませんでした。"),
    ).toBeTruthy();
  });

  it("lists the values only for this instance and releases a known one with null", async () => {
    mount({
      ...joined,
      overrides: [
        { fieldId: "fov", value: "0.8" },
        { fieldId: "lastServer", value: "x" },
      ],
    });
    await screen.findByText("このインスタンスだけの値");
    const release = await screen.findAllByRole("button", { name: /を共通設定に戻す$/ });
    // 未知の項目には解除のボタンを出さない
    expect(release).toHaveLength(1);
    await act(async () => {
      fireEvent.click(release[0]!);
    });
    await waitFor(() => expect(setSyncOverride).toHaveBeenCalledWith("i1", "fov", null));
    expect(card().textContent).not.toContain("lastServer");
    expect(card().textContent).not.toContain("fov");
  });

  it("names each value only for this instance and shows the value", async () => {
    mount({
      ...joined,
      overrides: [
        { fieldId: "fov", value: "1.0" },
        { fieldId: "key_key.jump", value: "key.keyboard.j" },
        { fieldId: "lastServer", value: "x" },
      ],
    });
    await screen.findByText("このインスタンスだけの値");
    expect(await screen.findByRole("button", { name: "視野角を共通設定に戻す" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "ジャンプを共通設定に戻す" })).toBeTruthy();
    const rows = within(screen.getByRole("list", { name: "このインスタンスだけの値" })).getAllByRole("listitem");
    expect(rows[0]!.textContent).toContain("110°");
    expect(rows[1]!.textContent).toContain("J");
    expect(rows[2]!.textContent).toContain("その他の設定");
    expect(rows[2]!.textContent).not.toContain("x");
  });

  it("has no badge or proposal text when nothing changed in game", async () => {
    mount({ ...joined, pendingCount: 0 });
    await screen.findByText("共通設定「いつもの」を使っています");
    expect(screen.queryByTestId("proposal-badge")).toBeNull();
    expect(card().textContent).not.toContain("変えた設定");
  });

  it("says how many settings changed in this instance", async () => {
    mount({ ...joined, pendingCount: 2 });
    expect(await screen.findByText("このインスタンスで変えた設定が 2 件あります")).toBeTruthy();
    expect(screen.getByTestId("proposal-badge").textContent).toBe("2");
  });

  it("leaves without a confirmation and notes that it can be undone", async () => {
    mount(joined);
    await screen.findByText("共通設定「いつもの」を使っています");
    expect(card().textContent).toContain(
      "外れても、このインスタンスの設定はそのまま残ります。あとでまた参加できます。",
    );
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "共通設定から外れる" }));
    });
    await waitFor(() => expect(leaveSyncProfile).toHaveBeenCalledWith("i1"));
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("never shows the internal words", async () => {
    mount({ ...joined, upToDate: false, applicability: "versionMismatch" });
    await screen.findByText("共通設定「いつもの」を使っています");
    for (const word of ["同期プロファイル", "options.txt", "DataVersion", "codec"]) {
      expect(card().textContent).not.toContain(word);
    }
  });
});
