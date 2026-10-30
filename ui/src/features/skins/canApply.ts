import type { ActiveAccount } from "../accounts/useAccounts";

/** スキンやマントを変えられるのは、ログインが有効な Microsoft アカウントのときだけ。 */
export function canApply(active: ActiveAccount | null): boolean {
  return active?.kind === "msa" && !active.account.needsLogin;
}
