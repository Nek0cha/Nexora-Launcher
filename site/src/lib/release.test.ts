import { describe, expect, it, vi } from "vitest";
import { loadRelease, pickAssets, RELEASES_URL } from "./release";

const base = "https://github.com/Nek0cha/Nexora-Launcher/releases/download/v0.1.2/";
const names = [
  "latest.json",
  "Nexora_0.1.2_aarch64.dmg",
  "Nexora_0.1.2_amd64.AppImage",
  "Nexora_0.1.2_amd64.AppImage.sig",
  "Nexora_0.1.2_amd64.deb",
  "Nexora_0.1.2_amd64.deb.sig",
  "Nexora_0.1.2_x64-setup.exe",
  "Nexora_0.1.2_x64-setup.exe.sig",
  "Nexora_0.1.2_x64.dmg",
  "Nexora_aarch64.app.tar.gz",
  "Nexora_aarch64.app.tar.gz.sig",
  "Nexora_x64.app.tar.gz",
  "Nexora_x64.app.tar.gz.sig",
];
const assets = names.map((name) => ({ name, url: base + name }));

describe("pickAssets", () => {
  it("picks one installer per platform and skips signatures and updater files", () => {
    expect(pickAssets(assets)).toEqual({
      windows: `${base}Nexora_0.1.2_x64-setup.exe`,
      "macos-arm": `${base}Nexora_0.1.2_aarch64.dmg`,
      "macos-intel": `${base}Nexora_0.1.2_x64.dmg`,
      "linux-appimage": `${base}Nexora_0.1.2_amd64.AppImage`,
      "linux-deb": `${base}Nexora_0.1.2_amd64.deb`,
    });
  });

  it("leaves out platforms it cannot find", () => {
    expect(pickAssets([{ name: "Nexora_0.2.0_x64-setup.exe", url: "u" }])).toEqual({ windows: "u" });
  });
});

const ok = (body: unknown) =>
  vi.fn(async () => new Response(JSON.stringify(body), { status: 200 })) as unknown as typeof fetch;

describe("loadRelease", () => {
  it("returns the version without the v and the picked assets", async () => {
    const fetchImpl = ok({
      tag_name: "v0.1.2",
      html_url: "https://github.com/Nek0cha/Nexora-Launcher/releases/tag/v0.1.2",
      assets: names.map((name) => ({ name, browser_download_url: base + name })),
    });
    const release = await loadRelease(fetchImpl);
    expect(release.version).toBe("0.1.2");
    expect(release.pageUrl).toBe("https://github.com/Nek0cha/Nexora-Launcher/releases/tag/v0.1.2");
    expect(release.assets.windows).toBe(`${base}Nexora_0.1.2_x64-setup.exe`);
  });

  it.each([403, 404, 500])("falls back to the releases page on HTTP %i", async (status) => {
    const fetchImpl = vi.fn(async () => new Response("{}", { status })) as unknown as typeof fetch;
    expect(await loadRelease(fetchImpl)).toEqual({ version: null, pageUrl: RELEASES_URL, assets: {} });
  });

  it("falls back when the request throws or the body is not a release", async () => {
    const throwing = vi.fn(async () => {
      throw new Error("offline");
    }) as unknown as typeof fetch;
    expect(await loadRelease(throwing)).toEqual({ version: null, pageUrl: RELEASES_URL, assets: {} });
    expect(await loadRelease(ok({ message: "x" }))).toEqual({ version: null, pageUrl: RELEASES_URL, assets: {} });
  });
});
