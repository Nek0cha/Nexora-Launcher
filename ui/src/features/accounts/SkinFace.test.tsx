import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getSkin } from "../../api/commands";
import { SkinFace } from "./SkinFace";

vi.mock("../../api/commands", () => ({ getSkin: vi.fn() }));

let imageHeight = 64;
class FakeImage {
  naturalHeight = imageHeight;
  onload: (() => void) | null = null;
  set src(_: string) {
    queueMicrotask(() => this.onload?.());
  }
}

beforeEach(() => {
  vi.stubGlobal("Image", FakeImage);
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.resetAllMocks();
  imageHeight = 64;
});

function mount(accountId: string | null) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <SkinFace accountId={accountId} size={32} />
    </QueryClientProvider>,
  );
}

describe("SkinFace", () => {
  it("crops the face and hat layers with pixelated scaling", async () => {
    vi.mocked(getSkin).mockResolvedValue("data:image/png;base64,AAAA");
    mount("a");
    const face = await screen.findByTestId("skin-face-base");
    const hat = await screen.findByTestId("skin-face-hat");
    expect(face.style.backgroundPosition).toBe("-32px -32px");
    expect(hat.style.backgroundPosition).toBe("-160px -32px");
    expect(face.style.backgroundSize).toMatch(/^256px/);
    expect(face.style.imageRendering).toBe("pixelated");
    expect(hat.style.imageRendering).toBe("pixelated");
  });

  it("shows only the face for a legacy 64x32 skin", async () => {
    imageHeight = 32;
    vi.mocked(getSkin).mockResolvedValue("data:image/png;base64,AAAA");
    mount("a");
    await screen.findByTestId("skin-face-base");
    expect(screen.queryByTestId("skin-face-hat")).toBeNull();
  });

  it("shows the default icon without a skin", async () => {
    vi.mocked(getSkin).mockResolvedValue(null);
    mount("a");
    await waitFor(() => expect(getSkin).toHaveBeenCalled());
    expect(screen.getByTestId("skin-face-default")).toBeTruthy();
  });

  it("shows the default icon without an account and does not fetch", () => {
    mount(null);
    expect(screen.getByTestId("skin-face-default")).toBeTruthy();
    expect(getSkin).not.toHaveBeenCalled();
  });
});
