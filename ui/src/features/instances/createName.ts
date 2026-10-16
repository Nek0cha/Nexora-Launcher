/** バージョンを選び直したときの名前。空、または前のバージョン名のままなら、新しいバージョン名にする。 */
export function nextName(current: string, previousVersion: string | null, nextVersion: string): string {
  const trimmed = current.trim();
  if (trimmed === "" || trimmed === previousVersion) return nextVersion;
  return current;
}
