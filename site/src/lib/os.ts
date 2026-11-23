import type { Platform } from "./release";

/**
 * 閲覧者の OS を UA から推定する。推定できなければ null（主ボタンは Releases の一覧のままにする）。
 * Mac は Apple Silicon と Intel をブラウザから確実に見分けられないため、新しい Mac に多い Apple Silicon にする。
 * iPadOS は Mac と同じ UA を名乗るので、タッチ点の数（`navigator.maxTouchPoints`）で外す。
 */
export function detectPlatform(ua: string, maxTouchPoints = 0): Platform | null {
  if (/Android|iPhone|iPad|CrOS/.test(ua)) return null;
  if (/Windows NT/.test(ua)) return "windows";
  if (/Macintosh/.test(ua)) return maxTouchPoints > 1 ? null : "macos-arm";
  if (/Linux x86_64|X11; Ubuntu|Fedora/.test(ua) && !/aarch64|arm/i.test(ua)) return "linux-appimage";
  return null;
}
