import type { CrashCandidate } from "../../bindings/CrashCandidate";

/** ダイアログを開いたときに、最初からチェックを付けておく候補。確度が低い候補（スタックトレースだけが手がかり）は外しておく。 */
export function initialChecked(candidates: CrashCandidate[]): string[] {
  return candidates.filter((c) => c.confidence === "high").map((c) => c.relPath);
}
