import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listLoaderGameVersions, listVersions, previewUpgrade, upgradeInstance } from "../../api/commands";
import { IpcError } from "../../api/ipc";
import type { UpgradePreview } from "../../bindings/UpgradePreview";
import i18n from "../../i18n";
import { ToastRegion } from "../toast/ToastRegion";
import { clearToasts } from "../toast/useToast";
import { UpgradeInstanceDialog } from "./UpgradeInstanceDialog";

vi.mock("../../api/commands", () => ({
  listVersions: vi.fn(),
  listLoaderGameVersions: vi.fn(),
  previewUpgrade: vi.fn(),
  upgradeInstance: vi.fn(),
}));

const entries = (...ids: string[]) => ids.map((id) => ({ id, releaseTime: "2030-01-01T00:00:00+00:00" }));

const preview = (patch: Partial<UpgradePreview> = {}): UpgradePreview => ({
  loaderVersion: null,
  compatible: [],
  incompatible: [],
  unknown: [],
  ...patch,
});

function mount(props: Partial<Parameters<typeof UpgradeInstanceDialog>[0]> = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <UpgradeInstanceDialog instanceId="i1" mcVersion="1.20.1" loader="vanilla" {...props} />
      <ToastRegion />
    </QueryClientProvider>,
  );
}

async function open() {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Upgrade version" }));
  });
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(listVersions).mockResolvedValue(entries("1.21.5", "1.21.1", "1.20.1", "1.8.9"));
  vi.mocked(listLoaderGameVersions).mockResolvedValue(["1.21.1", "1.20.1"]);
});

afterEach(() => {
  act(() => clearToasts());
  cleanup();
  vi.resetAllMocks();
});

const submit = () => screen.getByRole("button", { name: "Upgrade" });

describe("UpgradeInstanceDialog", () => {
  it("offers only newer releases and previews the newest one first", async () => {
    vi.mocked(previewUpgrade).mockResolvedValue(preview());
    mount();
    await open();

    await waitFor(() => expect(previewUpgrade).toHaveBeenCalledWith("i1", "1.21.5"));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Target/ }));
    });
    const options = within(await screen.findByRole("listbox")).getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["1.21.5", "1.21.1"]);
  });

  it("limits the choices to the versions the loader supports", async () => {
    vi.mocked(previewUpgrade).mockResolvedValue(preview({ loaderVersion: "0.16.10" }));
    mount({ loader: "fabric" });
    await open();

    await waitFor(() => expect(previewUpgrade).toHaveBeenCalledWith("i1", "1.21.1"));
    expect(listLoaderGameVersions).toHaveBeenCalledWith("fabric");
  });

  it("shows the counts and the names of the mods that need attention", async () => {
    vi.mocked(previewUpgrade).mockResolvedValue(
      preview({
        compatible: ["mods/a.jar", "mods/b.jar"],
        incompatible: ["mods/gone.jar"],
        unknown: ["mods/mine.jar"],
      }),
    );
    mount();
    await open();

    expect(await screen.findByText("gone.jar")).toBeTruthy();
    expect(screen.getByText("mine.jar")).toBeTruthy();
    expect(screen.queryByText("a.jar")).toBeNull();
    expect(screen.getByLabelText("Compatible: 2")).toBeTruthy();
    expect(screen.getByLabelText("No compatible version: 1")).toBeTruthy();
    expect(screen.getByLabelText("Can't check: 1")).toBeTruthy();
  });

  it("upgrades to the chosen version and closes", async () => {
    vi.mocked(previewUpgrade).mockResolvedValue(preview());
    vi.mocked(upgradeInstance).mockResolvedValue("t1");
    mount();
    await open();
    await waitFor(() => expect((submit() as HTMLButtonElement).disabled).toBe(false));

    await act(async () => {
      fireEvent.click(submit());
    });

    expect(upgradeInstance).toHaveBeenCalledWith("i1", "1.21.5");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("keeps the dialog open and shows the error when the upgrade is refused", async () => {
    vi.mocked(previewUpgrade).mockResolvedValue(preview());
    vi.mocked(upgradeInstance).mockRejectedValue(new IpcError({ code: "instance.busy", params: {}, detail: null }));
    mount();
    await open();
    await waitFor(() => expect((submit() as HTMLButtonElement).disabled).toBe(false));

    await act(async () => {
      fireEvent.click(submit());
    });

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("lets the user upgrade without the check when the preview fails", async () => {
    vi.mocked(previewUpgrade).mockRejectedValue(new IpcError({ code: "net.offline", params: {}, detail: null }));
    vi.mocked(upgradeInstance).mockResolvedValue("t1");
    mount();
    await open();

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect((submit() as HTMLButtonElement).disabled).toBe(true);
    await act(async () => {
      fireEvent.click(screen.getByRole("checkbox", { name: "Upgrade without checking" }));
    });
    await act(async () => {
      fireEvent.click(submit());
    });

    expect(upgradeInstance).toHaveBeenCalledWith("i1", "1.21.5");
  });

  it("does not offer to skip the check when the version cannot be used", async () => {
    vi.mocked(previewUpgrade).mockRejectedValue(
      new IpcError({ code: "upgrade.unavailable", params: { reason: "no build" }, detail: null }),
    );
    mount({ loader: "fabric" });
    await open();

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.queryByRole("checkbox", { name: "Upgrade without checking" })).toBeNull();
    expect((submit() as HTMLButtonElement).disabled).toBe(true);
  });

  it("cannot upgrade when there is no newer version", async () => {
    mount({ mcVersion: "1.21.5" });
    await open();

    expect(await screen.findByText("No newer version")).toBeTruthy();
    expect((submit() as HTMLButtonElement).disabled).toBe(true);
    expect(previewUpgrade).not.toHaveBeenCalled();
  });

  it("disables the opening button while the instance is busy", () => {
    mount({ isDisabled: true });
    expect((screen.getByRole("button", { name: "Upgrade version" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
