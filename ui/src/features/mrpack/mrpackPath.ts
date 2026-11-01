/** `.mrpack` のパスか。拡張子だけで判定し、大文字小文字は問わない。 */
export function isMrpackPath(path: string): boolean {
  return /\.mrpack$/i.test(path);
}

/** `.mrpack` を除いたパス。パックの読み込みは `MrpackDropHost` が受けるので、各タブの受け口は触らない。 */
export function withoutMrpack(paths: string[]): string[] {
  return paths.filter((path) => !isMrpackPath(path));
}
