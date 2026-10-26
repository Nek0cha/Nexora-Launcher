import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createSyncProfileFromInstance,
  joinSyncProfile,
  listSyncProfiles,
  previewJoinSyncProfile,
} from "../../api/commands";
import { IpcError } from "../../api/ipc";
import type { SyncProfile } from "../../bindings/SyncProfile";
import i18n from "../../i18n";
import { JoinSyncDialog } from "./JoinSyncDialog";

vi.mock("../../api/commands", () => ({
  listSyncProfiles: vi.fn(),
  createSyncProfileFromInstance: vi.fn(),
  previewJoinSyncProfile: vi.fn(),
  joinSyncProfile: vi.fn(),
}));

const profile = (id: string, name: string): SyncProfile => ({
  id,
  name,
  revision: 1,
  categories: ["keys"],
  members: [{ id: "i9", name: "Other" }],
});

const onOpenChange = vi.fn();

function mount({
  profiles = [] as SyncProfile[],
  hasOptions = true,
  running = false,
} = {}) {
  vi.mocked(listSyncProfiles).mockResolvedValue(profiles);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <JoinSyncDialog
        instanceId="i1"
        hasOptions={hasOptions}
        running={running}
        isOpen
        onOpenChange={onOpenChange}
      />
    </QueryClientProvider>,
  );
}

const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;
const press = (name: string) =>
  act(async () => {
    fireEvent.click(screen.getByRole("button", { name }));
  });

beforeEach(async () => {
  await i18n.changeLanguage("ja");
  vi.mocked(createSyncProfileFromInstance).mockResolvedValue(profile("p9", "共通設定"));
  vi.mocked(joinSyncProfile).mockResolvedValue({ differing: 0 });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("JoinSyncDialog", () => {
  it("offers only creating when no shared settings exist", async () => {
    mount();
    await screen.findByRole("dialog");
    await waitFor(() => expect(listSyncProfiles).toHaveBeenCalled());
    expect(screen.queryByRole("radio", { name: "既存の共通設定に参加する" })).toBeNull();
    expect(radio("このインスタンスの設定から新しい共通設定を作る").checked).toBe(true);
  });

  it("creates with a numbered default name when the name is taken", async () => {
    mount({ profiles: [profile("p1", "共通設定")] });
    await screen.findByRole("radio", { name: "既存の共通設定に参加する" });
    fireEvent.click(radio("このインスタンスの設定から新しい共通設定を作る"));
    const name = (await screen.findByRole("textbox", { name: "名前" })) as HTMLInputElement;
    expect(name.value).toBe("共通設定 2");
    await press("作成する");
    await waitFor(() => expect(createSyncProfileFromInstance).toHaveBeenCalledWith("i1", "共通設定 2"));
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("starts on joining when the instance has no settings to create from", async () => {
    mount({ profiles: [profile("p1", "いつもの")], hasOptions: false });
    await screen.findByRole("radio", { name: "既存の共通設定に参加する" });
    await waitFor(() => expect(radio("既存の共通設定に参加する").checked).toBe(true));
    expect(radio("このインスタンスの設定から新しい共通設定を作る").disabled).toBe(true);
  });

  it("promises nothing when there is nothing to choose", async () => {
    mount({ hasOptions: false });
    const dialog = await screen.findByRole("dialog");
    await waitFor(() => expect(listSyncProfiles).toHaveBeenCalled());
    expect(radio("このインスタンスの設定から新しい共通設定を作る").disabled).toBe(true);
    expect(dialog.textContent).toContain("一度起動すると、設定を読み取れるようになります。");
    expect(dialog.textContent).not.toContain("次の起動時に反映されます。");
    expect((screen.getByRole("button", { name: "作成する" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("joins right away when nothing differs", async () => {
    vi.mocked(previewJoinSyncProfile).mockResolvedValue(0);
    mount({ profiles: [profile("p1", "いつもの")] });
    fireEvent.click(await screen.findByRole("radio", { name: "既存の共通設定に参加する" }));
    await press("参加する");
    await waitFor(() => expect(previewJoinSyncProfile).toHaveBeenCalledWith("i1", "p1"));
    await waitFor(() => expect(joinSyncProfile).toHaveBeenCalledWith("i1", "p1", "keepProfile"));
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("asks which side to keep when settings differ", async () => {
    vi.mocked(previewJoinSyncProfile).mockResolvedValue(3);
    mount({ profiles: [profile("p1", "いつもの"), profile("p2", "PvP")] });
    fireEvent.click(await screen.findByRole("radio", { name: "既存の共通設定に参加する" }));
    fireEvent.click(await screen.findByRole("radio", { name: "PvP" }));
    await press("参加する");
    const dialog = screen.getByRole("dialog");
    await within(dialog).findByText("このインスタンスと共通設定で、3 件の設定が違います。どちらに揃えますか。");
    expect(joinSyncProfile).not.toHaveBeenCalled();
    expect(dialog.textContent).toContain("次の起動時に反映されます。");
    expect(radio("共通設定に合わせる").checked).toBe(true);
    fireEvent.click(radio("このインスタンスの値で共通設定を更新する"));
    await press("参加する");
    await waitFor(() => expect(joinSyncProfile).toHaveBeenCalledWith("i1", "p2", "adoptInstance"));
  });

  it("says it applies after the current session while the game runs", async () => {
    mount({ running: true });
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("いまのプレイが終わった後に反映されます。");
    expect(dialog.textContent).not.toContain("次の起動時に反映されます。");
  });

  it("keeps the dialog open and shows the error when joining fails", async () => {
    vi.mocked(previewJoinSyncProfile).mockResolvedValue(0);
    vi.mocked(joinSyncProfile).mockRejectedValue(new IpcError({ code: "app.unexpected", params: {}, detail: null }));
    mount({ profiles: [profile("p1", "いつもの")] });
    fireEvent.click(await screen.findByRole("radio", { name: "既存の共通設定に参加する" }));
    await press("参加する");
    await screen.findByRole("alert");
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
  });
});
