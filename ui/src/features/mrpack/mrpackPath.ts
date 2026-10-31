/** `.mrpack` のパスか。拡張子だけで判定し、大文字小文字は問わない。 */
export function isMrpackPath(path: string): boolean {
  return /\.mrpack$/i.test(path);
}
