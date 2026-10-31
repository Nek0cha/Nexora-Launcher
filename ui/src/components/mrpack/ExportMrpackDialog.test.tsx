import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { exportMrpack, previewMrpackExport } from "../../api/commands";
import { pickMrpackSavePath } from "../../api/dialog";
import { IpcError } from "../../api/ipc";
import type { ExportPreview } from "../../bindings/ExportPreview";
import i18n from "../../i18n";
import { ToastRegion } from "../toast/ToastRegion";
import { clearToasts } from "../toast/useToast";
import { ExportMrpackDialog } from "./ExportMrpackDialog";

vi.mock("../../api/commands", () => ({
  previewMrpackExport: vi.fn(),
  exportMrpack: vi.fn(),
}));
vi.mock("../../api/dialog", () => ({ pickMrpackSavePath: vi.fn() }));

const preview = (patch: Partial<ExportPreview> = {}): ExportPreview => ({
  referenced: 3,
  unidentified: [],
  disabled: 0,
  ...patch,
});

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ExportMrpackDialog instanceId="i1" instanceName="Survival" />
      <ToastRegion />
    </QueryClientProvider>,
  );
}

async function open() {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Export as mrpack" }));
  });
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  act(() => clearToasts());
  cleanup();
  vi.resetAllMocks();
});

const submit = () => screen.getByRole("button", { name: "Export" });
const version = () => screen.findByRole("textbox", { name: "Version" }) as Promise<HTMLInputElement>;

describe("ExportMrpackDialog", () => {
  it("exports with the defaults when every file was identified", async () => {
    vi.mocked(previewMrpackExport).mockResolvedValue(preview());
    vi.mocked(pickMrpackSavePath).mockResolvedValue("C:\\out\\Survival.mrpack");
    vi.mocked(exportMrpack).mockResolvedValue(null);
    mount();
    await open();

    expect((await version()).value).toBe("1.0.0");
    expect(screen.queryByRole("checkbox", { name: "I checked" })).toBeNull();
    await act(async () => {
      fireEvent.click(submit());
    });

    expect(pickMrpackSavePath).toHaveBeenCalledWith("Survival.mrpack");
    await waitFor(() =>
      expect(exportMrpack).toHaveBeenCalledWith("i1", "C:\\out\\Survival.mrpack", {
        versionId: "1.0.0",
        includeConfig: true,
        acceptUnidentified: false,
      }),
    );
  });

  it("lists unidentified files and keeps exporting off until the user confirms", async () => {
    vi.mocked(previewMrpackExport).mockResolvedValue(preview({ unidentified: ["a.jar", "b.jar"] }));
    vi.mocked(pickMrpackSavePath).mockResolvedValue("C:\\out\\Survival.mrpack");
    vi.mocked(exportMrpack).mockResolvedValue(null);
    mount();
    await open();

    expect(await screen.findByText("a.jar")).toBeTruthy();
    expect(screen.getByText("b.jar")).toBeTruthy();
    expect(submit().hasAttribute("disabled")).toBe(true);

    await act(async () => {
      fireEvent.click(screen.getByRole("checkbox", { name: "I checked" }));
    });
    expect(submit().hasAttribute("disabled")).toBe(false);
    await act(async () => {
      fireEvent.click(submit());
    });

    await waitFor(() =>
      expect(exportMrpack).toHaveBeenCalledWith("i1", "C:\\out\\Survival.mrpack", {
        versionId: "1.0.0",
        includeConfig: true,
        acceptUnidentified: true,
      }),
    );
  });

  it("does not export when the save dialog is cancelled", async () => {
    vi.mocked(previewMrpackExport).mockResolvedValue(preview());
    vi.mocked(pickMrpackSavePath).mockResolvedValue(null);
    mount();
    await open();
    await version();

    await act(async () => {
      fireEvent.click(submit());
    });

    await waitFor(() => expect(pickMrpackSavePath).toHaveBeenCalled());
    expect(exportMrpack).not.toHaveBeenCalled();
  });

  it("does not allow a blank version", async () => {
    vi.mocked(previewMrpackExport).mockResolvedValue(preview());
    mount();
    await open();
    const input = await version();

    fireEvent.change(input, { target: { value: "   " } });

    await waitFor(() => expect(submit().hasAttribute("disabled")).toBe(true));
  });

  it("passes the config choice through", async () => {
    vi.mocked(previewMrpackExport).mockResolvedValue(preview());
    vi.mocked(pickMrpackSavePath).mockResolvedValue("C:\\o.mrpack");
    vi.mocked(exportMrpack).mockResolvedValue(null);
    mount();
    await open();
    await version();

    await act(async () => {
      fireEvent.click(screen.getByRole("checkbox", { name: "Include config" }));
    });
    await act(async () => {
      fireEvent.click(submit());
    });

    await waitFor(() =>
      expect(exportMrpack).toHaveBeenCalledWith("i1", "C:\\o.mrpack", {
        versionId: "1.0.0",
        includeConfig: false,
        acceptUnidentified: false,
      }),
    );
  });

  it("explains why it cannot export while offline and disables exporting", async () => {
    vi.mocked(previewMrpackExport).mockRejectedValue(
      new IpcError({ code: "net.offline", params: {}, detail: null }),
    );
    mount();
    await open();

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByText("Can't connect to the internet")).toBeTruthy();
    expect(submit().hasAttribute("disabled")).toBe(true);
  });

  it("shows a failure of the export itself", async () => {
    vi.mocked(previewMrpackExport).mockResolvedValue(preview());
    vi.mocked(pickMrpackSavePath).mockResolvedValue("C:\\o.mrpack");
    vi.mocked(exportMrpack).mockRejectedValue(
      new IpcError({ code: "io.failed", params: { path: "C:\\o.mrpack" }, detail: null }),
    );
    mount();
    await open();
    await version();

    await act(async () => {
      fireEvent.click(submit());
    });

    expect(await screen.findByRole("alert")).toBeTruthy();
  });
});
