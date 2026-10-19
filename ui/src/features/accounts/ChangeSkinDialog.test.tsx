import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { changeSkin, getSkinVariant } from "../../api/commands";
import { IpcError } from "../../api/ipc";
import i18n from "../../i18n";
const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;
const submitButton = () => screen.getByRole("button", { name: "Change" }) as HTMLButtonElement;
import { ChangeSkinDialog } from "./ChangeSkinDialog";
import { ACCOUNTS_KEY, SKIN_KEY } from "./useAccounts";

vi.mock("../../api/commands", () => ({ changeSkin: vi.fn(), getSkinVariant: vi.fn() }));

let client: QueryClient;

function mount() {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ChangeSkinDialog accountId="a1" />
    </QueryClientProvider>,
  );
}

async function openDialog() {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Change skin" }));
    await Promise.resolve();
  });
}

// jsdom の File に arrayBuffer がない場合に備え、読み出しを持つ File を作る
const PNG = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];
function pngFile() {
  const file = new File([new Uint8Array(PNG)], "skin.png", { type: "image/png" });
  if (typeof file.arrayBuffer !== "function") {
    Object.defineProperty(file, "arrayBuffer", { value: async () => new Uint8Array(PNG).buffer });
  }
  return file;
}

async function chooseFile() {
  const input = screen.getByLabelText("Skin image (PNG)") as HTMLInputElement;
  await act(async () => {
    fireEvent.change(input, { target: { files: [pngFile()] } });
    await Promise.resolve();
  });
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(getSkinVariant).mockResolvedValue("classic");
  vi.mocked(changeSkin).mockResolvedValue(undefined as never);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ChangeSkinDialog", () => {
  it("variant_defaults_to_current_skin_model", async () => {
    vi.mocked(getSkinVariant).mockResolvedValue("slim");
    mount();
    await openDialog();
    await waitFor(() => expect(radio("Slim").checked).toBe(true));
    expect(radio("Classic").checked).toBe(false);
  });

  it("no_file_selected_disables_submit", async () => {
    mount();
    await openDialog();
    expect(submitButton().disabled).toBe(true);
    await chooseFile();
    expect(submitButton().disabled).toBe(false);
  });

  it("submit_sends_bytes_and_variant", async () => {
    mount();
    await openDialog();
    await waitFor(() => expect(radio("Classic").checked).toBe(true));
    fireEvent.click(screen.getByRole("radio", { name: "Slim" }));
    await chooseFile();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Change" }));
      await Promise.resolve();
    });
    await waitFor(() => expect(changeSkin).toHaveBeenCalledWith("a1", "slim", PNG));
  });

  it("success_refreshes_skin_query", async () => {
    mount();
    client.setQueryData([...SKIN_KEY, "a1"], null);
    client.setQueryData(ACCOUNTS_KEY, { accounts: [], devOffline: null });
    await openDialog();
    await chooseFile();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Change" }));
      await Promise.resolve();
    });
    await waitFor(() => expect(client.getQueryState([...SKIN_KEY, "a1"])?.isInvalidated).toBe(true));
    expect(client.getQueryState(ACCOUNTS_KEY)?.isInvalidated).toBe(true);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("invalid_png_error_is_shown", async () => {
    vi.mocked(changeSkin).mockRejectedValue(new IpcError({ code: "skin.invalid", message: "bad", params: {} } as never));
    mount();
    await openDialog();
    await chooseFile();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Change" }));
      await Promise.resolve();
    });
    await screen.findByRole("alert");
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("oversized_file_is_rejected_without_calling_the_command", async () => {
    mount();
    await openDialog();
    const big = new File([new Uint8Array(64 * 1024 + 1)], "big.png", { type: "image/png" });
    const input = screen.getByLabelText("Skin image (PNG)") as HTMLInputElement;
    await act(async () => {
      fireEvent.change(input, { target: { files: [big] } });
      await Promise.resolve();
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Change" }));
      await Promise.resolve();
    });
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("64 KiB");
    expect(changeSkin).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});
