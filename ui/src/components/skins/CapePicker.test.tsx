import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getCurrentSkin, setCape } from "../../api/commands";
import type { CapeView } from "../../bindings/CapeView";
import i18n from "../../i18n";
import { CapePicker } from "./CapePicker";

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

function mount(props: { canApply?: boolean; blocked?: boolean; onBusyChange?: (b: boolean) => void } = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <CapePicker accountId="a1" canApply={props.canApply ?? true} blocked={props.blocked} onBusyChange={props.onBusyChange} />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(setCape).mockResolvedValue(null);
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("CapePicker", () => {
  it("renders nothing when there are no capes", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin([]));
    const { container } = mount();
    await waitFor(() => expect(getCurrentSkin).toHaveBeenCalled());
    await act(async () => {});
    expect(container.innerHTML).toBe("");
  });

  it("renders nothing when the fetch fails", async () => {
    vi.mocked(getCurrentSkin).mockRejectedValue(new Error("x"));
    const { container } = mount();
    await waitFor(() => expect(getCurrentSkin).toHaveBeenCalled());
    await act(async () => {});
    expect(container.innerHTML).toBe("");
  });

  it("renders nothing and does not fetch when the account cannot apply", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin(capes("c1")));
    const { container } = mount({ canApply: false });
    await act(async () => {});
    expect(getCurrentSkin).not.toHaveBeenCalled();
    expect(container.innerHTML).toBe("");
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
    await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  });

  it("disables every control while blocked by another action", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(skin(capes("c1")));
    mount({ blocked: true });
    await screen.findByRole("button", { name: "Migrator" });
    for (const b of screen.getAllByRole("button")) expect((b as HTMLButtonElement).disabled).toBe(true);
  });
});
