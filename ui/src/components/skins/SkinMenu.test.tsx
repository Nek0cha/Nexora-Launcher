import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { applySavedSkin, deleteSavedSkin, listSavedSkins, updateSavedSkin } from "../../api/commands";
import type { SavedSkin } from "../../bindings/SavedSkin";
import { useSavedSkins, useSkinActions, resetCooldown } from "../../features/skins/useSkins";
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

const menuButtons = () => screen.getAllByRole("button", { name: "Skin menu", hidden: true }) as HTMLButtonElement[];
const open = async (id: string) => {
  fireEvent.click(menuButtons()[Number(id) - 1] as HTMLElement);
  await screen.findByRole("dialog");
};
const nameInput = () => screen.getByRole("textbox", { name: "Name" }) as HTMLInputElement;
const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;
const saveButton = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;
const picker = () => document.querySelector("input[type=file]") as HTMLInputElement;
const png = (bytes: number[]) => new File([new Uint8Array(bytes)], "new.png", { type: "image/png" });
const choosePng = async (f: File) => {
  await act(async () => {
    fireEvent.change(picker(), { target: { files: [f] } });
  });
};
const save = () => fireEvent.click(saveButton());
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
  resetCooldown();
  vi.resetAllMocks();
});

const ready = async () => {
  await screen.findByRole("button", { name: "Skin 1" });
};

describe("SkinMenu dialog", () => {
  it("opens with the current values and sends nothing until Save", async () => {
    list = [skin("1", { variant: "slim" }), skin("2")];
    mount({ worn: "1" });
    await ready();
    await open("1");
    expect(nameInput().value).toBe("Skin 1");
    expect(radio("Slim").checked).toBe(true);
    expect(radio("Classic").checked).toBe(false);
    expect(saveButton().disabled).toBe(true);
    fireEvent.change(nameInput(), { target: { value: "Changed" } });
    fireEvent.click(radio("Classic"));
    await choosePng(png([1]));
    expect(updateSavedSkin).not.toHaveBeenCalled();
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("starts from the current values again after cancelling", async () => {
    mount({ worn: null });
    await ready();
    await open("1");
    fireEvent.change(nameInput(), { target: { value: "Changed" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await open("1");
    expect(nameInput().value).toBe("Skin 1");
    expect(updateSavedSkin).not.toHaveBeenCalled();
  });

  it("trims the name and sends only the name, without applying the worn skin", async () => {
    mount({ worn: "1" });
    await ready();
    await open("1");
    fireEvent.change(nameInput(), { target: { value: "  New name  " } });
    save();
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledWith("1", { name: "New name", variant: null, png: null }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(updateSavedSkin).toHaveBeenCalledTimes(1);
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("cannot save an empty name or an unchanged form", async () => {
    mount({ worn: null });
    await ready();
    await open("1");
    fireEvent.change(nameInput(), { target: { value: "   " } });
    expect(saveButton().disabled).toBe(true);
    fireEvent.change(nameInput(), { target: { value: "Skin 1" } });
    expect(saveButton().disabled).toBe(true);
    save();
    expect(updateSavedSkin).not.toHaveBeenCalled();
  });

  it("accepts a name that another card already has", async () => {
    mount({ worn: null });
    await ready();
    await open("1");
    fireEvent.change(nameInput(), { target: { value: "Skin 2" } });
    save();
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledWith("1", { name: "Skin 2", variant: null, png: null }));
    expect(toasts()).toHaveLength(0);
  });

  it("updates then applies when the model of the worn skin changes", async () => {
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
    fireEvent.click(radio("Slim"));
    save();
    await waitFor(() => expect(applySavedSkin).toHaveBeenCalledWith("acc", "1"));
    expect(updateSavedSkin).toHaveBeenCalledWith("1", { name: null, variant: "slim", png: null });
    expect(order).toEqual(["update", "apply"]);
  });

  it("sends name, model and file in one update", async () => {
    mount({ worn: "1" });
    await ready();
    await open("1");
    fireEvent.change(nameInput(), { target: { value: "All" } });
    fireEvent.click(radio("Slim"));
    await choosePng(png([1, 2, 3]));
    save();
    await waitFor(() => expect(applySavedSkin).toHaveBeenCalledTimes(1));
    expect(updateSavedSkin).toHaveBeenCalledTimes(1);
    expect(updateSavedSkin).toHaveBeenCalledWith("1", { name: "All", variant: "slim", png: [1, 2, 3] });
  });

  it("only updates when the skin is not worn", async () => {
    mount({ worn: "2" });
    await ready();
    await open("1");
    fireEvent.click(radio("Slim"));
    save();
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledTimes(1));
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("only updates when the account cannot apply", async () => {
    mount({ worn: "1", canApply: false });
    await ready();
    await open("1");
    fireEvent.click(radio("Slim"));
    save();
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledTimes(1));
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("stays open and does not apply when the update fails, reporting once", async () => {
    vi.mocked(updateSavedSkin).mockRejectedValue({ code: "skin.not_found", params: {}, detail: null });
    mount({ worn: "1" });
    await ready();
    await open("1");
    fireEvent.click(radio("Slim"));
    save();
    await waitFor(() => expect(toasts()).toHaveLength(1));
    expect(applySavedSkin).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("keeps the old name on the card until the refetch ends", async () => {
    mount({ worn: null });
    await ready();
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    vi.mocked(updateSavedSkin).mockImplementation(async (id, patch) => {
      list = list.map((s) => (s.id === id ? { ...s, name: patch.name ?? s.name } : s));
      return list[0] as SavedSkin;
    });
    vi.mocked(listSavedSkins).mockImplementation(async () => {
      await gate;
      return list;
    });
    await open("1");
    fireEvent.change(nameInput(), { target: { value: "Renamed" } });
    save();
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalled());
    expect(screen.getByRole("button", { name: "Skin 1", hidden: true })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Renamed", hidden: true })).toBeNull();
    await act(async () => release());
    await screen.findByRole("button", { name: "Renamed" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});

describe("SkinMenu replace file", () => {
  it("accepts png only", async () => {
    mount({ worn: null });
    await ready();
    await open("1");
    expect(picker().accept).toBe("image/png");
  });

  it("sends the bytes and applies again when worn", async () => {
    mount({ worn: "1" });
    await ready();
    await open("1");
    await choosePng(png([1, 2, 3]));
    save();
    await waitFor(() => expect(applySavedSkin).toHaveBeenCalledWith("acc", "1"));
    expect(updateSavedSkin).toHaveBeenCalledWith("1", { name: null, variant: null, png: [1, 2, 3] });
  });

  it("only updates when not worn", async () => {
    mount({ worn: "2" });
    await ready();
    await open("1");
    await choosePng(png([9]));
    save();
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledTimes(1));
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("rejects an oversized file before reading it, with one toast", async () => {
    mount({ worn: "1" });
    await ready();
    await open("1");
    const big = png([1]);
    Object.defineProperty(big, "size", { value: 70 * 1024 });
    const read = vi.spyOn(big, "arrayBuffer");
    await choosePng(big);
    await waitFor(() => expect(toasts()).toHaveLength(1));
    expect(read).not.toHaveBeenCalled();
    expect(saveButton().disabled).toBe(true);
  });
});

describe("SkinMenu delete", () => {
  it("cannot delete the skin that is being worn", async () => {
    mount({ worn: "1" });
    await ready();
    await open("1");
    const del = screen.getByRole("button", { name: "Delete" }) as HTMLButtonElement;
    expect(del.disabled).toBe(true);
    fireEvent.click(del);
    expect(deleteSavedSkin).not.toHaveBeenCalled();
  });

  it("deletes the library entry only, with no confirmation and no apply", async () => {
    mount({ worn: "2" });
    await ready();
    await open("1");
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(deleteSavedSkin).toHaveBeenCalledWith("1"));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(applySavedSkin).not.toHaveBeenCalled();
    expect(updateSavedSkin).not.toHaveBeenCalled();
  });
});

describe("SkinMenu on the card", () => {
  it("is disabled while the shared actions are busy or the card is adding", async () => {
    let release!: () => void;
    vi.mocked(deleteSavedSkin).mockImplementation(() => new Promise((r) => (release = () => r(undefined as never))));
    const view = mount({ worn: null });
    await ready();
    await open("2");
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(menuButtons()[0]?.disabled).toBe(true));
    expect(menuButtons()[1]?.disabled).toBe(true);
    await act(async () => release());
    await waitFor(() => expect(menuButtons()[0]?.disabled).toBe(false));
    view.rerender({ worn: null, adding: true });
    expect(menuButtons()[0]?.disabled).toBe(true);
  });

  it("keeps every other card disabled while a save is running", async () => {
    let releaseUpdate!: () => void;
    vi.mocked(updateSavedSkin).mockImplementation(async () => {
      await new Promise<void>((r) => (releaseUpdate = r));
      return list[0] as SavedSkin;
    });
    mount({ worn: null });
    await ready();
    await open("1");
    fireEvent.change(nameInput(), { target: { value: "Slow" } });
    save();
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalled());
    expect(menuButtons()[1]?.disabled).toBe(true);
    await act(async () => releaseUpdate());
  });
});
