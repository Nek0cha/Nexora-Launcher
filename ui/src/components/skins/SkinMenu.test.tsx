import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { applySavedSkin, deleteSavedSkin, listSavedSkins, updateSavedSkin } from "../../api/commands";
import type { SavedSkin } from "../../bindings/SavedSkin";
import { useSavedSkins, useSkinActions } from "../../features/skins/useSkins";
import i18n from "../../i18n";
import { clearToasts, useToastList } from "../toast/useToast";
import { SkinCard } from "./SkinCard";
import { SkinMenu } from "./SkinMenu";

vi.mock("../../api/commands", () => ({
  applySavedSkin: vi.fn(),
  deleteSavedSkin: vi.fn(),
  listSavedSkins: vi.fn(),
  updateSavedSkin: vi.fn(),
}));

const skin = (id: string, patch: Partial<SavedSkin> = {}): SavedSkin => ({
  id, name: `Skin ${id}`, variant: "classic", sha256: `h-${id}`, addedAtMs: 1, ...patch,
});

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

const pressed = vi.fn();

// 実際の画面と同じく、保存済みの一覧・共通の操作・カードを組み合わせる
function Harness({ worn, canApply = true, adding = false }: { worn: string | null; canApply?: boolean; adding?: boolean }) {
  const saved = useSavedSkins();
  const actions = useSkinActions("acc");
  const [working, setWorking] = useState(false);
  return (
    <div>
      {(saved.data ?? []).map((s) => (
        <SkinCard
          key={s.id}
          image={null}
          name={s.name}
          variant={s.variant}
          selected={s.id === worn}
          pending={false}
          disabled={false}
          onPress={() => pressed(s.id)}
          menu={
            <SkinMenu
              skin={s}
              worn={s.id === worn}
              canApply={canApply}
              actions={actions}
              disabled={actions.busy || adding || working}
              onWorking={setWorking}
            />
          }
        />
      ))}
      <Toasts />
    </div>
  );
}

function mount(props: Parameters<typeof Harness>[0]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const tree = (p: Parameters<typeof Harness>[0]) => (
    <QueryClientProvider client={client}>
      <Harness {...p} />
    </QueryClientProvider>
  );
  const view = render(tree(props));
  return { rerender: (p: Parameters<typeof Harness>[0]) => view.rerender(tree(p)) };
}

const open = async (id: string) => {
  const btn = screen.getAllByRole("button", { name: "Skin menu" })[Number(id) - 1] as HTMLElement;
  fireEvent.click(btn);
  await screen.findByRole("menu");
};
const items = () => [...screen.getAllByRole("menuitem"), ...screen.getAllByRole("menuitemradio")];
const item = (name: string) => items().find((e) => e.textContent === name) as HTMLElement;
const choose = async (name: string) => {
  fireEvent.click(item(name));
};
const nameInput = () => screen.queryByRole("textbox", { name: "Name" }) as HTMLInputElement | null;
const picker = () => document.querySelector("input[type=file]") as HTMLInputElement;
const toasts = () => screen.getByTestId("toasts").querySelectorAll("li");

let list: SavedSkin[];

beforeEach(async () => {
  await i18n.changeLanguage("en");
  clearToasts();
  list = [skin("1"), skin("2")];
  vi.mocked(listSavedSkins).mockImplementation(async () => list);
  vi.mocked(updateSavedSkin).mockResolvedValue(list[0] as SavedSkin);
  vi.mocked(applySavedSkin).mockResolvedValue(null);
  vi.mocked(deleteSavedSkin).mockResolvedValue(undefined as never);
  pressed.mockReset();
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

const ready = async () => {
  await screen.findByRole("button", { name: "Skin 1" });
};

describe("SkinMenu rename", () => {
  it("trims the name and sends it", async () => {
    mount({ worn: null });
    await ready();
    await open("1");
    await choose("Rename");
    const input = await waitFor(() => {
      expect(nameInput()).not.toBeNull();
      return nameInput() as HTMLInputElement;
    });
    expect(input.value).toBe("Skin 1");
    fireEvent.change(input, { target: { value: "  New name  " } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledWith("1", { name: "New name", variant: null, png: null }));
    expect(updateSavedSkin).toHaveBeenCalledTimes(1);
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("does not send an empty or whitespace-only name and keeps the old one", async () => {
    mount({ worn: null });
    await ready();
    await open("1");
    await choose("Rename");
    const input = (await waitFor(() => {
      expect(nameInput()).not.toBeNull();
      return nameInput();
    })) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.blur(input);
    expect(updateSavedSkin).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Skin 1" })).toBeTruthy();
    expect(toasts()).toHaveLength(0);
  });

  it("accepts a name that another card already has", async () => {
    mount({ worn: null });
    await ready();
    await open("1");
    await choose("Rename");
    const input = (await waitFor(() => {
      expect(nameInput()).not.toBeNull();
      return nameInput();
    })) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Skin 2" } });
    fireEvent.blur(input);
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledWith("1", { name: "Skin 2", variant: null, png: null }));
    expect(toasts()).toHaveLength(0);
  });

  it("cancels with Escape without a call", async () => {
    mount({ worn: null });
    await ready();
    await open("1");
    await choose("Rename");
    const input = (await waitFor(() => {
      expect(nameInput()).not.toBeNull();
      return nameInput();
    })) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Other" } });
    // ブラウザは、フォーカスのある入力が外れるときに blur を送る。それを DOM の取り外しに合わせて再現する
    const remove = Node.prototype.removeChild;
    const spy = vi.spyOn(Node.prototype, "removeChild").mockImplementation(function <T extends Node>(this: Node, node: T): T {
      if (node.contains(input)) fireEvent.blur(input);
      return remove.call(this, node) as T;
    });
    try {
      fireEvent.keyDown(input, { key: "Escape" });
      await act(async () => new Promise((r) => setTimeout(r, 30)));
    } finally {
      spy.mockRestore();
    }
    expect(nameInput()).toBeNull();
    expect(updateSavedSkin).not.toHaveBeenCalled();
  });

  it("does not upload the worn skin again when only its name changes", async () => {
    mount({ worn: "1" });
    await ready();
    await open("1");
    await choose("Rename");
    const input = (await waitFor(() => {
      expect(nameInput()).not.toBeNull();
      return nameInput();
    })) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Renamed" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledWith("1", { name: "Renamed", variant: null, png: null }));
    await waitFor(() => expect(nameInput()).toBeNull());
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("sends exactly once when Enter is followed by a blur", async () => {
    mount({ worn: null });
    await ready();
    await open("1");
    await choose("Rename");
    const input = (await waitFor(() => {
      expect(nameInput()).not.toBeNull();
      return nameInput();
    })) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Once" } });
    act(() => {
      fireEvent.keyDown(input, { key: "Enter" });
      fireEvent.blur(input);
    });
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledTimes(1));
    await act(async () => {});
    expect(updateSavedSkin).toHaveBeenCalledTimes(1);
  });

  it("shows the old name until the refetch ends, then the new one", async () => {
    mount({ worn: null });
    await ready();
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    vi.mocked(updateSavedSkin).mockImplementation(async (id, patch) => {
      list = list.map((s) => (s.id === id ? { ...s, name: patch.name ?? s.name } : s));
      return list[0] as SavedSkin;
    });
    // 保存の後の取り直しを止めておく
    vi.mocked(listSavedSkins).mockImplementation(async () => {
      await gate;
      return list;
    });
    await open("1");
    await choose("Rename");
    const input = (await waitFor(() => {
      expect(nameInput()).not.toBeNull();
      return nameInput();
    })) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Renamed" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalled());
    // 取り直しの途中：カードは古い名前のまま
    expect(screen.getByRole("button", { name: "Skin 1" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Renamed" })).toBeNull();
    await act(async () => release());
    await screen.findByRole("button", { name: "Renamed" });
    expect(screen.queryByRole("button", { name: "Skin 1" })).toBeNull();
    expect(nameInput()).toBeNull();
  });
});

describe("SkinMenu model", () => {
  it("marks the current model", async () => {
    list = [skin("1", { variant: "slim" }), skin("2")];
    mount({ worn: null });
    await ready();
    await open("1");
    expect(item("Slim").getAttribute("aria-checked")).toBe("true");
    expect(item("Classic").getAttribute("aria-checked")).toBe("false");
  });

  it("updates then applies when the skin is worn and can be applied", async () => {
    const order: string[] = [];
    vi.mocked(updateSavedSkin).mockImplementation(async () => {
      order.push("update");
      return list[0] as SavedSkin;
    });
    vi.mocked(applySavedSkin).mockImplementation(async () => {
      order.push("apply");
      return null;
    });
    mount({ worn: "1" });
    await ready();
    await open("1");
    await choose("Slim");
    await waitFor(() => expect(applySavedSkin).toHaveBeenCalledWith("acc", "1"));
    expect(updateSavedSkin).toHaveBeenCalledWith("1", { name: null, variant: "slim", png: null });
    expect(order).toEqual(["update", "apply"]);
  });

  it("only updates when the skin is not worn", async () => {
    mount({ worn: "2" });
    await ready();
    await open("1");
    await choose("Slim");
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledTimes(1));
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("only updates when the account cannot apply", async () => {
    mount({ worn: "1", canApply: false });
    await ready();
    await open("1");
    await choose("Slim");
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledTimes(1));
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("does nothing for the model it already has", async () => {
    mount({ worn: "1" });
    await ready();
    await open("1");
    await choose("Classic");
    expect(updateSavedSkin).not.toHaveBeenCalled();
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("does not apply when the update fails, and reports once", async () => {
    vi.mocked(updateSavedSkin).mockRejectedValue({ code: "skin.not_found", params: {}, detail: null });
    mount({ worn: "1" });
    await ready();
    await open("1");
    await choose("Slim");
    await waitFor(() => expect(toasts()).toHaveLength(1));
    expect(applySavedSkin).not.toHaveBeenCalled();
  });
});

describe("SkinMenu replace file", () => {
  const png = (bytes: number[]) => new File([new Uint8Array(bytes)], "new.png", { type: "image/png" });
  const choosePng = async (f: File) => {
    await act(async () => {
      fireEvent.change(picker(), { target: { files: [f] } });
    });
  };

  it("opens the picker for png only", async () => {
    mount({ worn: null });
    await ready();
    expect(picker().accept).toBe("image/png");
    const click = vi.spyOn(picker(), "click");
    await open("1");
    await choose("Replace file");
    expect(click).toHaveBeenCalled();
  });

  it("sends the bytes and applies again when worn", async () => {
    mount({ worn: "1" });
    await ready();
    await choosePng(png([1, 2, 3]));
    await waitFor(() => expect(applySavedSkin).toHaveBeenCalledWith("acc", "1"));
    expect(updateSavedSkin).toHaveBeenCalledWith("1", { name: null, variant: null, png: [1, 2, 3] });
  });

  it("only updates when not worn", async () => {
    mount({ worn: "2" });
    await ready();
    await choosePng(png([9]));
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledTimes(1));
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("rejects an oversized file before reading it, with one toast", async () => {
    mount({ worn: "1" });
    await ready();
    const big = png([1]);
    Object.defineProperty(big, "size", { value: 70 * 1024 });
    const read = vi.spyOn(big, "arrayBuffer");
    await choosePng(big);
    await waitFor(() => expect(toasts()).toHaveLength(1));
    expect(read).not.toHaveBeenCalled();
    expect(updateSavedSkin).not.toHaveBeenCalled();
    expect(applySavedSkin).not.toHaveBeenCalled();
  });
});

describe("SkinMenu delete", () => {
  it("deletes the library entry only, with no confirmation and no apply", async () => {
    mount({ worn: "1" });
    await ready();
    await open("1");
    await choose("Delete");
    await waitFor(() => expect(deleteSavedSkin).toHaveBeenCalledWith("1"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(applySavedSkin).not.toHaveBeenCalled();
    expect(updateSavedSkin).not.toHaveBeenCalled();
  });

  it("puts delete last", async () => {
    mount({ worn: null });
    await ready();
    await open("1");
    const names = Array.from(document.querySelectorAll("[role^=menuitem]")).map((e) => e.textContent);
    expect(names).toEqual(["Rename", "Classic", "Slim", "Replace file", "Delete"]);
  });
});

describe("SkinMenu on the card", () => {
  it("is disabled while the shared actions are busy or the card is adding", async () => {
    let release!: () => void;
    vi.mocked(deleteSavedSkin).mockImplementation(() => new Promise((r) => (release = () => r(undefined as never))));
    const view = mount({ worn: null });
    await ready();
    await open("2");
    await choose("Delete");
    await waitFor(() => expect((screen.getAllByRole("button", { name: "Skin menu" })[0] as HTMLButtonElement).disabled).toBe(true));
    expect((screen.getAllByRole("button", { name: "Skin menu" })[1] as HTMLButtonElement).disabled).toBe(true);
    await act(async () => release());
    await waitFor(() => expect((screen.getAllByRole("button", { name: "Skin menu" })[0] as HTMLButtonElement).disabled).toBe(false));
    view.rerender({ worn: null, adding: true });
    expect((screen.getAllByRole("button", { name: "Skin menu" })[0] as HTMLButtonElement).disabled).toBe(true);
  });

  it("keeps the card and menu button size classes in every state", async () => {
    const size = (c: string) => c.split(" ").filter((x) => /^(p|m|w|h|size|aspect|border(-\d)?$|inset|gap)/.test(x));
    mount({ worn: null });
    await ready();
    const card = screen.getByRole("button", { name: "Skin 1" });
    const btn = screen.getAllByRole("button", { name: "Skin menu" })[0] as HTMLElement;
    const before = [size(card.className), size(btn.className), size(card.parentElement!.className)];
    await open("1");
    await choose("Rename");
    await waitFor(() => expect(nameInput()).not.toBeNull());
    const after = [
      size(screen.getByRole("button", { name: "Skin 1" }).className),
      size((screen.getAllByRole("button", { name: "Skin menu" })[0] as HTMLElement).className),
      size(screen.getByRole("button", { name: "Skin 1" }).parentElement!.className),
    ];
    expect(after).toEqual(before);
  });
});
