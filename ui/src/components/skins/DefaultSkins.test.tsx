import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { addSavedSkin, addSavedSkinFromPath, applyDefaultSkin, listDefaultSkins } from "../../api/commands";
import type { DefaultSkin } from "../../bindings/DefaultSkin";
import i18n from "../../i18n";
import { clearToasts, useToastList } from "../toast/useToast";
import { DefaultSkins } from "./DefaultSkins";

vi.mock("../../api/commands", () => ({
  addSavedSkin: vi.fn(),
  addSavedSkinFromPath: vi.fn(),
  applyDefaultSkin: vi.fn(),
  listDefaultSkins: vi.fn(),
}));

const PNG = "data:image/png;base64,AAAA";
const skins: DefaultSkin[] = [
  { key: "classic-steve", name: "Steve", variant: "classic", image: PNG },
  { key: "slim-alex", name: "Alex", variant: "slim", image: PNG },
  { key: "classic-ari", name: "Ari", variant: "classic", image: PNG },
  { key: "slim-ari", name: "Ari (slim)", variant: "slim", image: PNG },
  { key: "classic-kai", name: "Kai", variant: "classic", image: PNG },
];

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

function mount(props: { canApply?: boolean; blocked?: boolean } = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <DefaultSkins accountId="a1" canApply={props.canApply ?? true} blocked={props.blocked} />
      <Toasts />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  vi.useRealTimers();
  await i18n.changeLanguage("en");
  clearToasts();
  vi.mocked(listDefaultSkins).mockResolvedValue(skins);
  vi.mocked(applyDefaultSkin).mockResolvedValue(undefined as never);
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.clearAllMocks();
});

const retryButton = () => screen.getByRole("button", { name: "Retry" });
const toasts = () => screen.getByTestId("toasts").children.length;

describe("DefaultSkins", () => {
  it("renders every returned skin and applies one by key without touching the library", async () => {
    mount();
    for (const s of skins) await screen.findByRole("button", { name: s.name });
    expect(screen.getAllByRole("button")).toHaveLength(5);
    fireEvent.click(screen.getByRole("button", { name: "Alex" }));
    await waitFor(() => expect(applyDefaultSkin).toHaveBeenCalledWith("a1", "slim-alex"));
    expect(addSavedSkin).not.toHaveBeenCalled();
    expect(addSavedSkinFromPath).not.toHaveBeenCalled();
    expect(screen.queryByTestId("skin-card-check")).toBeNull();
  });

  it("shows one toast when applying fails", async () => {
    vi.mocked(applyDefaultSkin).mockRejectedValue({ kind: "network", message: "x" });
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Steve" }));
    await waitFor(() => expect(toasts()).toBe(1));
    expect(screen.getAllByRole("button")).toHaveLength(5);
  });

  it("does not apply when canApply is false or blocked", async () => {
    const view = mount({ canApply: false });
    const steve = (await screen.findByRole("button", { name: "Steve" })) as HTMLButtonElement;
    expect(steve.disabled).toBe(true);
    fireEvent.click(steve);
    expect(applyDefaultSkin).not.toHaveBeenCalled();
    view.unmount();
    mount({ blocked: true });
    const again = (await screen.findByRole("button", { name: "Steve" })) as HTMLButtonElement;
    expect(again.disabled).toBe(true);
    fireEvent.click(again);
    expect(applyDefaultSkin).not.toHaveBeenCalled();
  });

  it("shows a single retry card on failure, retries on press and then shows the grid, without toasts", async () => {
    vi.mocked(listDefaultSkins).mockRejectedValueOnce({ kind: "network", message: "x" });
    mount();
    await screen.findByRole("button", { name: "Retry" });
    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(toasts()).toBe(0);

    vi.mocked(listDefaultSkins).mockRejectedValueOnce({ kind: "network", message: "x" });
    fireEvent.click(retryButton());
    await waitFor(() => expect(listDefaultSkins).toHaveBeenCalledTimes(2));
    await screen.findByRole("button", { name: "Retry" });
    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(toasts()).toBe(0);

    fireEvent.click(retryButton());
    await screen.findByRole("button", { name: "Steve" });
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    expect(screen.getAllByRole("button")).toHaveLength(5);
  });

  it("marks the retry card busy and unpressable while the refetch is in flight, same size as a skin card", async () => {
    vi.mocked(listDefaultSkins).mockRejectedValueOnce({ kind: "network", message: "x" });
    mount();
    const idle = await screen.findByRole("button", { name: "Retry" });
    const idleClass = idle.parentElement?.className;
    let release!: (v: DefaultSkin[]) => void;
    vi.mocked(listDefaultSkins).mockReturnValueOnce(new Promise((r) => { release = r; }));
    fireEvent.click(idle);
    await waitFor(() => expect(retryButton().parentElement?.getAttribute("aria-busy")).toBe("true"));
    expect(screen.getByTestId("default-skins-retry-progress")).toBeTruthy();
    expect(retryButton().parentElement?.className).toBe(idleClass);
    expect(idleClass).toContain("aspect-[2/3] w-full");
    fireEvent.click(retryButton());
    expect(listDefaultSkins).toHaveBeenCalledTimes(2);
    await act(async () => release(skins));
    const steve = await screen.findByRole("button", { name: "Steve" });
    expect(steve.parentElement?.className).toContain("aspect-[2/3] w-full");
  });

  it("shows no skeleton for a quick fetch and a same-size skeleton after 150 ms for a slow one", async () => {
    vi.useFakeTimers();
    let release!: (v: DefaultSkin[]) => void;
    vi.mocked(listDefaultSkins).mockReturnValue(new Promise((r) => { release = r; }));
    mount();
    await act(async () => { await vi.advanceTimersByTimeAsync(100); });
    expect(screen.queryAllByTestId("default-skin-skeleton")).toHaveLength(0);
    await act(async () => { await vi.advanceTimersByTimeAsync(100); });
    const sk = screen.getAllByTestId("default-skin-skeleton");
    expect(sk.length).toBeGreaterThan(0);
    expect(sk[0]!.className).toContain("aspect-[2/3]");
    expect(sk[0]!.className).toContain("w-full");
    await act(async () => { release(skins); await vi.advanceTimersByTimeAsync(0); });
    expect(screen.queryAllByTestId("default-skin-skeleton")).toHaveLength(0);
    expect(screen.getByRole("button", { name: "Steve" }).parentElement?.className).toContain("aspect-[2/3]");
  });
});
