import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * 外部のページを既定のブラウザで開く。呼び出し側は、UI に固定で持つ URL か、
 * 許可リストで確かめた URL だけを渡す（サーバーが返した URL をそのまま渡さない）。
 */
export function openExternal(url: string): Promise<void> {
  return openUrl(url);
}
