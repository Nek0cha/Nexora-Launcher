import type { ConflictView } from "../../bindings/ConflictView";
import type { PendingView } from "../../bindings/PendingView";
import type { Resolution } from "../../bindings/Resolution";
import type { SingleProposal } from "../../bindings/SingleProposal";

/** resolveSyncProposal の 1 回分 */
export type ResolveCall = {
  instanceId: string;
  fieldId: string;
  resolution: Resolution;
  chosenValue?: string;
};

/**
 * 見せる提案を絞る。instanceId が null なら共通設定の全部（「共通設定」の画面）、
 * あればそのインスタンスが関わるものだけ（インスタンスのカード）。
 */
export function scopeProposals(view: PendingView, instanceId: string | null): PendingView {
  if (instanceId === null) return view;
  return {
    single: view.single.filter((s) => s.instanceIds.includes(instanceId)),
    conflicts: view.conflicts.filter((c) => c.entries.some((e) => e.instanceId === instanceId)),
  };
}

/**
 * 値が 1 通りの提案への選択を、呼び出しの列にする。
 * 「全部に反映する」は提案した全インスタンスの分を解決する（最初の 1 回で共通設定が変わり、残りは消えるだけ）。
 * 「このインスタンスだけ別にする」と「無視」は、カードではそのインスタンスだけ、画面では並んだ全インスタンス。
 */
export function singleCalls(item: SingleProposal, resolution: Resolution, instanceId: string | null): ResolveCall[] {
  const targets =
    resolution === "applyToProfile" || instanceId === null
      ? item.instanceIds
      : item.instanceIds.filter((id) => id === instanceId);
  return targets.map((id) => ({ instanceId: id, fieldId: item.fieldId, resolution }));
}

/**
 * 競合への選択を、呼び出しの列にする。
 * 値を選んだときは、その値を出したインスタンスで chosenValue を付けて共通設定へ反映し、
 * 残りのインスタンスの提案は無視にする（次の起動で、選んだ値になる）。
 */
export function conflictCalls(
  conflict: ConflictView,
  choice: { choose: string } | "ignore",
  instanceId: string | null,
): ResolveCall[] {
  const fieldId = conflict.fieldId;
  if (choice === "ignore") {
    return conflict.entries
      .filter((e) => instanceId === null || e.instanceId === instanceId)
      .map((e) => ({ instanceId: e.instanceId, fieldId, resolution: "ignore" }));
  }
  const first = conflict.entries.find((e) => e.value === choice.choose);
  if (!first) return [];
  return [
    { instanceId: first.instanceId, fieldId, resolution: "applyToProfile", chosenValue: choice.choose },
    ...conflict.entries
      .filter((e) => e !== first)
      .map((e): ResolveCall => ({ instanceId: e.instanceId, fieldId, resolution: "ignore" })),
  ];
}

/** 「まとめて全部に反映する」。値が 1 通りの提案だけを対象にし、競合は含めない */
export function bulkCalls(view: PendingView, instanceId: string | null): ResolveCall[] {
  return view.single.flatMap((s) => singleCalls(s, "applyToProfile", instanceId));
}

/** 呼び出しを 1 つずつ順に行う（並列にしない）。失敗したらそこで止める */
export async function runSerially(calls: ResolveCall[], resolve: (call: ResolveCall) => Promise<unknown>) {
  for (const call of calls) {
    await resolve(call);
  }
}
