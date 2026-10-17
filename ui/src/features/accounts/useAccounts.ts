import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { listAccounts } from "../../api/commands";
import { onAccountsChanged } from "../../api/events";
import type { Account } from "../../bindings/Account";
import type { AccountList } from "../../bindings/AccountList";

export const ACCOUNTS_KEY = ["accounts"] as const;

export type ActiveAccount =
  | { kind: "msa"; account: Account }
  | { kind: "devOffline"; username: string };

/** 一覧から、使用中のアカウントを決める。 */
export function pickActive(list: AccountList | undefined): ActiveAccount | null {
  if (!list) return null;
  if (list.devOffline?.active) return { kind: "devOffline", username: list.devOffline.username };
  const account = list.accounts.find((a) => a.isActive);
  return account ? { kind: "msa", account } : null;
}

/** アカウントの一覧。`accounts://changed` で取り直す。 */
export function useAccounts() {
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: ACCOUNTS_KEY, queryFn: listAccounts });

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    onAccountsChanged(() => {
      void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
    })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((e) => console.warn("accounts listen failed", e));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient]);

  return { data: query.data, error: query.error, active: pickActive(query.data) };
}
