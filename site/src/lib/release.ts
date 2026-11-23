/**
 * 最新のリリースの配布物を、ビルドの時点で GitHub から読む。
 * 配布物そのものは GitHub Releases から配信し、サイトには URL だけを埋め込む。
 */

export type Platform = "windows" | "macos-arm" | "macos-intel" | "linux-appimage" | "linux-deb";

export type ReleaseLinks = {
  /** `v` を外したバージョン。読めなかったときは null */
  version: string | null;
  /** そのリリースのページ。読めなかったときは Releases の一覧 */
  pageUrl: string;
  assets: Partial<Record<Platform, string>>;
};

export const RELEASES_URL = "https://github.com/Nek0cha/Nexora-Launcher/releases";
const LATEST_API = "https://api.github.com/repos/Nek0cha/Nexora-Launcher/releases/latest";

// 配布物の名前の末尾で見分ける（`.sig`、`.app.tar.gz`、`latest.json` は自動更新用なので当たらない）
const patterns: [Platform, RegExp][] = [
  ["windows", /_x64-setup\.exe$/],
  ["macos-arm", /_aarch64\.dmg$/],
  ["macos-intel", /_x64\.dmg$/],
  ["linux-appimage", /_amd64\.AppImage$/],
  ["linux-deb", /_amd64\.deb$/],
];

/** 配布物の一覧から、OS ごとのインストーラーを 1 つずつ選ぶ。見つからない OS は含めない。 */
export function pickAssets(assets: { name: string; url: string }[]): Partial<Record<Platform, string>> {
  const out: Partial<Record<Platform, string>> = {};
  for (const [platform, pattern] of patterns) {
    const found = assets.find((a) => pattern.test(a.name));
    if (found) out[platform] = found.url;
  }
  return out;
}

/** GitHub が応答しないとき、ビルドを止めずに諦めるまでの時間（ミリ秒） */
const TIMEOUT_MS = 10_000;

// 落ちたことに気づけるよう、理由をビルドのログに残す（ボタンは Releases の一覧になる）
const fallback = (reason: string): ReleaseLinks => {
  console.warn(`[release] using the releases page instead of direct links: ${reason}`);
  return { version: null, pageUrl: RELEASES_URL, assets: {} };
};

/** 最新のリリースを読む。失敗しても throw せず、Releases の一覧へのリンクだけを返す（ビルドを止めない）。 */
export async function loadRelease(fetchImpl: typeof fetch = fetch): Promise<ReleaseLinks> {
  try {
    const headers: Record<string, string> = { Accept: "application/vnd.github+json" };
    const token = typeof process !== "undefined" ? process.env.GITHUB_TOKEN : undefined;
    if (token) headers.Authorization = `Bearer ${token}`;
    const res = await fetchImpl(LATEST_API, { headers, signal: AbortSignal.timeout(TIMEOUT_MS) });
    if (!res.ok) return fallback(`HTTP ${res.status}`);
    const body = (await res.json()) as {
      tag_name?: unknown;
      html_url?: unknown;
      assets?: { name?: unknown; browser_download_url?: unknown }[];
    };
    if (typeof body.tag_name !== "string" || !Array.isArray(body.assets)) return fallback("unexpected response");
    const assets = body.assets.flatMap((a) =>
      typeof a.name === "string" && typeof a.browser_download_url === "string"
        ? [{ name: a.name, url: a.browser_download_url }]
        : [],
    );
    return {
      version: body.tag_name.replace(/^v/, ""),
      pageUrl: typeof body.html_url === "string" ? body.html_url : RELEASES_URL,
      assets: pickAssets(assets),
    };
  } catch (error) {
    return fallback(error instanceof Error ? error.message : String(error));
  }
}
