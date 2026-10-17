import type { Account } from "../../bindings/Account";

export type AccountStatus = "active" | "needsLogin" | "unsaved" | "ownershipUnknown" | "ok";

/** アカウント 1 件の表示上の状態。優先順は needsLogin、unsaved、ownershipUnknown、active、ok。 */
export function accountStatus(account: Account): AccountStatus {
  if (account.needsLogin) return "needsLogin";
  if (!account.loginSaved) return "unsaved";
  if (account.entitlement === "unknown") return "ownershipUnknown";
  if (account.isActive) return "active";
  return "ok";
}
