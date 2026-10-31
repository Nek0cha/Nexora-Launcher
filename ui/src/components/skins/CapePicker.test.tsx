import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getCurrentSkin, setCape } from "../../api/commands";
import type { CapeView } from "../../bindings/CapeView";
import i18n from "../../i18n";
import { clearToasts, useToastList } from "../toast/useToast";
import { CapePicker } from "./CapePicker";
import { resetCooldown } from "../../features/skins/useSkins";

vi.mock("../../api/commands", () => ({
  getCurrentSkin: vi.fn(),
  setCape: vi.fn(),
}));

const PNG = "data:image/png;base64,AAAA";
const capes = (active: string | null): CapeView[] => [
  { id: "c1", alias: "Migrator", active: active === "c1", image: PNG },
  { id: "c2", alias: "Vanilla", active: active === "c2", image: PNG },
];
const skin = (c: CapeView[]) => ({ image: PNG, sha256: "h", variant: "classic" as const, capes: c });

function Toasts() {
  const list = useToastList();
  return <ul data-testid="toasts">{list.map((t) => <li key={t.id}>{t.message}</li>)}</ul>;
}

function mount(props: { canApply?: boolean; blocked?: boolean; onBusyChange?: (b: boolean) => void } = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <Toasts />
      <CapePicker accountId="a1" canApply={props.canApply ?? true} blocked={props.blocked} onBusyChange={props.onBusyChange} />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  clearToasts();
  vi.mocked(setCape).mockResolvedValue(null);
});
afterEach(() => {
  cleanup();
  resetCooldown();
  vi.clearAllMocks();
});

describe("CapePicker", () => {
  it("renders nothing when there are no capes", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin([]));
    const { container } = mount();
    await waitFor(() => expect(getCurrentSkin).toHaveBeenCalled());
    await waitFor(() => expect(container.querySelector("[data-testid=toasts]")?.nextSibling ?? null).toBeNull());
  });

  it("renders nothing when the fetch fails", async () => {
    vi.mocked(getCurrentSkin).mockRejectedValue(new Error("x"));
    const { container } = mount();
    await waitFor(() => expect(getCurrentSkin).toHaveBeenCalled());
    await waitFor(() => expect(container.querySelector("[data-testid=toasts]")?.nextSibling ?? null).toBeNull());
  });

  it("renders nothing and does not fetch when the account cannot apply", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin(capes("c1")));
    const { container } = mount({ canApply: false });
    await act(async () => {});
    expect(getCurrentSkin).not.toHaveBeenCalled();
    expect(container.querySelector("[data-testid=toasts]")?.nextSibling ?? null).toBeNull();
  });

  it("marks the active cape and selects another by id", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin(capes("c1")));
    mount();
    const first = await screen.findByRole("button", { name: "Migrator" });
    expect(first.getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "Vanilla" }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.getAllByTestId("cape-check")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Vanilla" }));
    await waitFor(() => expect(setCape).toHaveBeenCalledWith("a1", "c2"));
  });

  it("removes the cape with null", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin(capes("c1")));
    mount();
    await screen.findByRole("button", { name: "Migrator" });
    expect(screen.getByRole("button", { name: "Remove" }).getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(setCape).toHaveBeenCalledWith("a1", null));
  });

  it("marks the remove card when no cape is active, and pressing it does nothing", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin(capes(null)));
    mount();
    const remove = await screen.findByRole("button", { name: "Remove" });
    expect(remove.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(remove);
    await act(async () => {});
    expect(setCape).not.toHaveBeenCalled();
  });

  it("keeps the old mark and disables everything until the refetch ends", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin(capes("c1")));
    const busy = vi.fn();
    mount({ onBusyChange: busy });
    await screen.findByRole("button", { name: "Migrator" });
    let release: () => void = () => {};
    vi.mocked(getCurrentSkin).mockImplementation(
      () => new Promise((r) => { release = () => r(skin(capes("c2"))); }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Vanilla" }));
    await waitFor(() => expect(busy).toHaveBeenLastCalledWith(true));
    for (const name of ["Migrator", "Vanilla", "Remove"]) {
      expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(true);
    }
    expect(screen.getByRole("button", { name: "Migrator" }).getAttribute("aria-pressed")).toBe("true");
    await act(async () => release());
    await waitFor(() => expect(screen.getByRole("button", { name: "Vanilla" }).getAttribute("aria-pressed")).toBe("true"));
    expect(screen.getByRole("button", { name: "Migrator" }).getAttribute("aria-pressed")).toBe("false");
    // 取り直しが終わっても、冷却の間は止めたまま（途中で false にならない）
    expect(busy).toHaveBeenLastCalledWith(true);
    const values = busy.mock.calls.map((c) => c[0]);
    expect(values.slice(values.indexOf(true))).not.toContain(false);
    for (const name of ["Migrator", "Vanilla", "Remove"]) {
      expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(true);
    }
  });

  it("disables every control while blocked by another action", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin(capes("c1")));
    mount({ blocked: true });
    await screen.findByRole("button", { name: "Migrator" });
    for (const b of screen.getAllByRole("button")) expect((b as HTMLButtonElement).disabled).toBe(true);
  });

  it("reserves the row while loading, then collapses once zero capes is known", async () => {
    let release: () => void = () => {};
    vi.mocked(getCurrentSkin).mockImplementation(() => new Promise((r) => { release = () => r(skin([])); }));
    const { container } = mount();
    expect(screen.getByTestId("cape-reserve").className).toContain("h-20");
    expect(screen.queryAllByRole("button")).toHaveLength(0);
    await act(async () => release());
    await waitFor(() => expect(container.querySelector("[data-testid=cape-reserve]")).toBeNull());
  });

  it("keeps the old capes, disabled, while another account loads", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValueOnce(skin(capes("c1")));
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    const tree = (id: string) => (
      <QueryClientProvider client={client}>
        <CapePicker accountId={id} canApply />
      </QueryClientProvider>
    );
    const view = render(tree("a1"));
    await screen.findByRole("button", { name: "Migrator" });
    vi.mocked(getCurrentSkin).mockImplementation(() => new Promise(() => {}));
    view.rerender(tree("a2"));
    for (const b of screen.getAllByRole("button")) expect((b as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole("button", { name: "Migrator" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("shows a spinner on the pressed chip only", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin(capes("c1")));
    vi.mocked(setCape).mockImplementation(() => new Promise(() => {}));
    mount();
    await screen.findByRole("button", { name: "Vanilla" });
    expect(screen.queryByTestId("cape-progress")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Vanilla" }));
    const spinner = await screen.findByTestId("cape-progress");
    expect(spinner.closest("[aria-busy]")).toBe(screen.getByRole("button", { name: "Vanilla" }).parentElement);
    expect(screen.getAllByTestId("cape-progress")).toHaveLength(1);
  });

  it("shows an error toast when setting the cape fails", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin(capes("c1")));
    vi.mocked(setCape).mockRejectedValue(new Error("boom"));
    mount();
    await screen.findByRole("button", { name: "Vanilla" });
    fireEvent.click(screen.getByRole("button", { name: "Vanilla" }));
    await waitFor(() => expect(screen.getByTestId("toasts").children.length).toBe(1));
  });
});
