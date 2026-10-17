import type { ErrorPayload } from "../../bindings/ErrorPayload";

/** プレイを押した結果、ログインのダイアログを出す理由。 */
export type LoginPrompt =
  | { kind: "loginRequired"; accountId: string }
  | { kind: "noAccount" }
  | { kind: "offline"; offlineAvailable: boolean };

/** 起動の失敗を、ダイアログを出す理由に変える。auth.login_required、auth.no_account、auth.offline 以外は null。 */
export function loginPrompt(error: ErrorPayload | null): LoginPrompt | null {
  if (!error) return null;
  switch (error.code) {
    case "auth.login_required": {
      const id = error.params.accountId;
      return { kind: "loginRequired", accountId: typeof id === "string" ? id : "" };
    }
    case "auth.no_account":
      return { kind: "noAccount" };
    case "auth.offline":
      return { kind: "offline", offlineAvailable: error.params.offlineAvailable === true };
    default:
      return null;
  }
}

// 外部のページ（固定）。spec 7 節
export const PURCHASE_URL = "https://www.minecraft.net/store/minecraft-java-bedrock-edition-pc";
export const TRIAL_URL = "https://www.minecraft.net/free-trial";
export const PROFILE_URL = "https://www.minecraft.net/msaprofile/mygames/editprofile";

/** デバイスコードの verification_uri のうち、開いてよいもの。これ以外は文字として表示するだけにする。 */
const VERIFICATION_URIS = new Set(["https://www.microsoft.com/link", "https://microsoft.com/devicelogin"]);

export function isAllowedVerificationUri(uri: string): boolean {
  return VERIFICATION_URIS.has(uri);
}

/** アカウントの ID が同じか。ハイフンの有無と大文字小文字は問わない。空の ID はどれとも一致しない。 */
export function sameAccount(a: string, b: string): boolean {
  const norm = (id: string) => id.replaceAll("-", "").toLowerCase();
  return norm(a) !== "" && norm(a) === norm(b);
}
