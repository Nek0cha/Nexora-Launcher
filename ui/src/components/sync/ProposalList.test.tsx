import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getSyncProfileValues, pendingSyncProposals, resolveSyncProposal } from "../../api/commands";
import type { PendingView } from "../../bindings/PendingView";
import type { SyncMember } from "../../bindings/SyncMember";
import i18n from "../../i18n";
import { ProposalsSection } from "./ProposalList";

vi.mock("../../api/commands", () => ({
  pendingSyncProposals: vi.fn(),
  getSyncProfileValues: vi.fn(),
  resolveSyncProposal: vi.fn(),
}));

const members: SyncMember[] = [
  { id: "i1", name: "Survival" },
  { id: "i2", name: "Creative" },
  { id: "i3", name: "Modded" },
];

const pending: PendingView = {
  single: [
    { instanceIds: ["i1"], fieldId: "fov", value: "0.375" },
    { instanceIds: ["i1", "i2"], fieldId: "autoJump", value: "true" },
    { instanceIds: ["i3"], fieldId: "key_key.jump", value: "key.keyboard.j" },
  ],
  conflicts: [
    {
      fieldId: "soundCategory_master",
      entries: [
        { instanceId: "i1", value: "0.25" },
        { instanceId: "i2", value: "0.75" },
      ],
    },
  ],
};

function mount(props: { instanceId?: string; count?: number; view?: PendingView }) {
  vi.mocked(pendingSyncProposals).mockResolvedValue(props.view ?? pending);
  vi.mocked(getSyncProfileValues).mockResolvedValue([
    { fieldId: "fov", value: "0.0" },
    { fieldId: "autoJump", value: "false" },
    { fieldId: "soundCategory_master", value: "1.0" },
    { fieldId: "key_key.jump", value: "key.keyboard.space" },
  ]);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const invalidate = vi.spyOn(client, "invalidateQueries");
  const view = render(
    <QueryClientProvider client={client}>
      <ProposalsSection profileId="p1" members={members} instanceId={props.instanceId} count={props.count} />
    </QueryClientProvider>,
  );
  return { ...view, invalidate };
}

async function open() {
  fireEvent.click(await screen.findByRole("button", { name: "確認する" }));
  return screen.findByRole("list", { name: "変えた設定" });
}

const item = (list: HTMLElement, name: string) =>
  within(list)
    .getAllByRole("listitem")
    .find((li) => li.querySelector("h4")?.textContent === name)!;

beforeEach(async () => {
  await i18n.changeLanguage("ja");
  vi.mocked(resolveSyncProposal).mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.restoreAllMocks();
});

describe("ProposalsSection on the instance card", () => {
  it("stays quiet with no badge when nothing changed", async () => {
    const { container } = mount({ instanceId: "i1", count: 0 });
    await act(async () => {});
    expect(container.textContent).toBe("");
    expect(screen.queryByTestId("proposal-badge")).toBeNull();
    expect(pendingSyncProposals).not.toHaveBeenCalled();
  });

  it("says how many settings changed in this instance and shows a badge", async () => {
    mount({ instanceId: "i1", count: 3 });
    expect(await screen.findByText("このインスタンスで変えた設定が 3 件あります")).toBeTruthy();
    expect(screen.getByTestId("proposal-badge").textContent).toBe("3");
  });

  it("lists this instance's changes with the shared value and this instance's value", async () => {
    mount({ instanceId: "i1", count: 3 });
    const list = await open();
    const fov = item(list, "視野角");
    expect(fov.textContent).toContain("いまの共通設定");
    expect(fov.textContent).toContain("70°");
    expect(fov.textContent).toContain("このインスタンス");
    expect(fov.textContent).toContain("85°");
    expect(item(list, "自動ジャンプ").textContent).toContain("オン");
    // ほかのインスタンスだけの提案（Modded のジャンプ）は出さない
    expect(within(list).getAllByRole("listitem").map((li) => li.querySelector("h4")?.textContent)).toEqual([
      "視野角",
      "自動ジャンプ",
      "主音量",
    ]);
  });

  it("has no note under the choices", async () => {
    mount({ instanceId: "i1", count: 3 });
    const list = await open();
    const ignore = within(item(list, "視野角")).getByRole("button", { name: "無視" });
    expect(ignore.getAttribute("aria-describedby")).toBeNull();
    expect(screen.queryByText(/次の起動で共通設定の値に戻ります/)).toBeNull();
  });

  it.each([
    ["全部に反映する", "applyToProfile"],
    ["このインスタンスだけ別にする", "keepForInstance"],
    ["無視", "ignore"],
  ] as const)("calls the resolution for %s", async (label, resolution) => {
    const { invalidate } = mount({ instanceId: "i1", count: 3 });
    const list = await open();
    await act(async () => {
      fireEvent.click(within(item(list, "視野角")).getByRole("button", { name: label }));
    });
    await waitFor(() => expect(resolveSyncProposal).toHaveBeenCalledTimes(1));
    expect(resolveSyncProposal).toHaveBeenCalledWith("i1", "fov", resolution, undefined);
    await waitFor(() => expect(invalidate).toHaveBeenCalledWith({ queryKey: ["sync"] }));
  });

  it("keeps or ignores only for this instance when others proposed the same value", async () => {
    mount({ instanceId: "i1", count: 3 });
    const list = await open();
    await act(async () => {
      fireEvent.click(within(item(list, "自動ジャンプ")).getByRole("button", { name: "このインスタンスだけ別にする" }));
    });
    await waitFor(() => expect(resolveSyncProposal).toHaveBeenCalledTimes(1));
    expect(resolveSyncProposal).toHaveBeenCalledWith("i1", "autoJump", "keepForInstance", undefined);
  });

  it("lets the user pick one value of a conflict and passes it as chosenValue", async () => {
    mount({ instanceId: "i1", count: 3 });
    const list = await open();
    const conflict = item(list, "主音量");
    expect(conflict.textContent).toContain("インスタンスによって値が分かれています");
    const apply = within(conflict).getByRole("button", { name: "選んだ値を全部に反映する" });
    expect(apply.hasAttribute("disabled") || apply.getAttribute("aria-disabled") === "true").toBe(true);
    const creative = within(conflict).getByRole("radio", { name: /75%.*Creative/ });
    expect(within(conflict).getByRole("radio", { name: /25%.*Survival/ })).toBeTruthy();
    // このインスタンスだけ別にする、は競合には出さない
    expect(within(conflict).queryByRole("button", { name: "このインスタンスだけ別にする" })).toBeNull();
    fireEvent.click(creative);
    await act(async () => {
      fireEvent.click(within(conflict).getByRole("button", { name: "選んだ値を全部に反映する" }));
    });
    await waitFor(() => expect(resolveSyncProposal).toHaveBeenCalledTimes(2));
    expect(vi.mocked(resolveSyncProposal).mock.calls).toEqual([
      ["i2", "soundCategory_master", "applyToProfile", "0.75"],
      ["i1", "soundCategory_master", "ignore", undefined],
    ]);
  });

  it("applies all single changes one after another and leaves the conflicts out", async () => {
    const order: string[] = [];
    let finishFirst: () => void = () => {};
    vi.mocked(resolveSyncProposal).mockImplementation((instanceId, fieldId) => {
      order.push(`start ${instanceId}/${fieldId}`);
      if (order.length === 1)
        return new Promise((resolve) => {
          finishFirst = () => {
            order.push(`end ${instanceId}/${fieldId}`);
            resolve(null);
          };
        });
      return Promise.resolve(null);
    });
    mount({ instanceId: "i1", count: 3 });
    const list = await open();
    expect(screen.getByText(/値が分かれている項目は、まとめての反映に含めません/)).toBeTruthy();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "まとめて全部に反映する（2 件）" }));
    });
    await waitFor(() => expect(order).toEqual(["start i1/fov"]));
    // 1 件目が終わるまで次を呼ばない
    expect(within(list).getAllByRole("button", { name: "無視" })[0]!.hasAttribute("disabled")).toBe(true);
    await act(async () => finishFirst());
    await waitFor(() => expect(resolveSyncProposal).toHaveBeenCalledTimes(3));
    expect(order).toEqual(["start i1/fov", "end i1/fov", "start i1/autoJump", "start i2/autoJump"]);
    expect(vi.mocked(resolveSyncProposal).mock.calls.every((c) => c[1] !== "soundCategory_master")).toBe(true);
  });

  it("does not offer bulk apply when there is only one change to apply", async () => {
    mount({
      instanceId: "i1",
      count: 1,
      view: { single: [pending.single[0]!], conflicts: [] },
    });
    await open();
    expect(screen.queryByRole("button", { name: /まとめて全部に反映する/ })).toBeNull();
  });

  it("never shows a raw id or key", async () => {
    mount({
      instanceId: "i1",
      count: 2,
      view: {
        single: [
          { instanceIds: ["i1"], fieldId: "key_some_mod.action", value: "key.keyboard.r" },
          { instanceIds: ["i1"], fieldId: "key_key.sneak", value: "key.keyboard.left.shift" },
        ],
        conflicts: [],
      },
    });
    const list = await open();
    expect(list.textContent).toContain("その他の設定");
    expect(list.textContent).toContain("スニーク");
    expect(list.textContent).toContain("左 Shift");
    for (const raw of ["key_", "key.keyboard", "some_mod"]) expect(list.textContent).not.toContain(raw);
  });
});

describe("ProposalsSection on the shared settings screen", () => {
  it("counts every change of the shared settings and names the instances", async () => {
    mount({});
    expect(await screen.findByText("参加中のインスタンスで変えた設定が 4 件あります")).toBeTruthy();
    const list = await open();
    expect(item(list, "自動ジャンプ").textContent).toContain("Survival、Creative");
    expect(item(list, "ジャンプ").textContent).toContain("Modded");
    expect(within(item(list, "ジャンプ")).getByRole("button", { name: "Modded だけ別にする" })).toBeTruthy();
  });

  it("stays quiet when no instance changed anything", async () => {
    const { container } = mount({ view: { single: [], conflicts: [] } });
    await waitFor(() => expect(pendingSyncProposals).toHaveBeenCalled());
    await act(async () => {});
    expect(container.textContent).toBe("");
  });
});
