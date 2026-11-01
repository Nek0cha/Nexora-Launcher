import { useQuery } from "@tanstack/react-query";
import { appInfo } from "../api/commands";

/** ブラウザの情報から Windows かを推し量る。app_info が届く前や、失敗したときの代わりに使う。 */
export function guessWindows(): boolean {
  if (typeof navigator === "undefined") return false;
  const uaData = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData;
  const source = uaData?.platform || navigator.platform || navigator.userAgent || "";
  return /^win|windows/i.test(source) || /windows/i.test(navigator.userAgent ?? "");
}

/** ブラウザの情報から macOS かを推し量る（app_info が届く前や失敗したときの代わり） */
export function guessMac(): boolean {
  if (typeof navigator === "undefined") return false;
  const uaData = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData;
  const source = uaData?.platform || navigator.platform || navigator.userAgent || "";
  return /^mac/i.test(source);
}

/**
 * Windows で動いているか。Windows ではウィンドウ全体の枠を外しているので、
 * 画面のどの状態でもウィンドウボタンを出すためにこれで決める。
 * app_info の os を優先し、まだ届いていないか失敗したときはブラウザの情報で決める。
 */
export function useIsWindows(): boolean {
  const { data: info } = useQuery({ queryKey: ["appInfo"], queryFn: appInfo });
  return info?.os ? info.os === "windows" : guessWindows();
}

/**
 * macOS で動いているか。macOS では OS のタイトルバーを消して、信号のボタン（赤黄緑）を
 * アプリのバーに重ねているので、バーの左端をその分だけ空ける。
 */
export function useIsMac(): boolean {
  const { data: info } = useQuery({ queryKey: ["appInfo"], queryFn: appInfo });
  return info?.os ? info.os === "macos" : guessMac();
}
