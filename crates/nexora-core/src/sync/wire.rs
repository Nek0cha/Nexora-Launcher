//! UI に渡す共通設定の型。保存層の型とは分け、camelCase の JSON と TypeScript の型にする。

use serde::Serialize;
use ts_rs::TS;

use super::apply::{Applicability, ApplyOutcome};
use super::proposals::PendingProposals;
use super::registry::Category;
use super::store::{SnapshotState, SyncProfile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SyncProfile")]
pub struct SyncProfileView {
    pub id: String,
    pub name: String,
    pub revision: i64,
    pub categories: Vec<Category>,
    /// 参加しているインスタンス。
    pub members: Vec<SyncMember>,
}

impl SyncProfileView {
    pub fn new(p: SyncProfile, members: Vec<SyncMember>) -> Self {
        SyncProfileView {
            id: p.id,
            name: p.name,
            revision: p.revision,
            categories: p.categories,
            members,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncMember {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LastApplied {
    pub state: SnapshotState,
    /// 記録した時刻（unix ミリ秒）。時刻を持たない古い記録では `null`。
    pub at_ms: Option<i64>,
    /// スキップや失敗の理由の分類（`noOptionsFile`、`writeFailed` など）。適用できたときは `null`。
    pub reason: Option<String>,
}

/// 項目 1 つの値。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncFieldValue {
    pub field_id: String,
    pub value: String,
}

/// 登録表の項目。表示名は UI が ID から決める。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncField {
    pub id: String,
    pub category: Category,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstanceSync {
    pub profile: Option<SyncProfileView>,
    pub members: Vec<SyncMember>,
    pub last_applied: Option<LastApplied>,
    /// このインスタンスに関わる未解決の提案の件数（競合は 1 項目を 1 件と数える）。
    pub pending_count: u32,
    /// 参加中の共通設定全体の、未解決の提案の件数（競合は 1 件と数える）。
    pub profile_pending_count: u32,
    pub excluded: Vec<String>,
    /// インスタンスに options.txt があるか（参加の有無によらない）。
    pub has_options: bool,
    /// 今のファイルと版で、共通設定を適用できるか（参加の有無によらない）。
    pub applicability: Applicability,
    /// 最後に適用した revision が、共通設定の今の revision と等しいか。参加していなければ `false`。
    pub up_to_date: bool,
    /// このインスタンスだけの値（例外）。項目の ID の順。
    pub overrides: Vec<SyncFieldValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JoinResult {
    pub differing: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SingleProposal {
    /// 同じ値を提案しているインスタンス（1 つ以上）。
    pub instance_ids: Vec<String>,
    pub field_id: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConflictEntry {
    pub instance_id: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConflictView {
    pub field_id: String,
    pub entries: Vec<ConflictEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PendingView {
    pub single: Vec<SingleProposal>,
    pub conflicts: Vec<ConflictView>,
}

impl From<PendingProposals> for PendingView {
    fn from(p: PendingProposals) -> Self {
        PendingView {
            single: p
                .single
                .into_iter()
                .map(|(instance_ids, proposal)| SingleProposal {
                    instance_ids,
                    field_id: proposal.field_id,
                    value: proposal.value,
                })
                .collect(),
            conflicts: p
                .conflicts
                .into_iter()
                .map(|group| ConflictView {
                    field_id: group.field_id,
                    entries: group
                        .entries
                        .into_iter()
                        .map(|(instance_id, value)| ConflictEntry { instance_id, value })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// 起動前の適用の結果の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SyncApplyOutcome {
    Applied,
    UpToDate,
    Skipped,
    Failed,
}

/// `sync-apply-finished` のイベント。参加中のインスタンスの起動前の適用が終わったときに送る。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncApplyFinishedEvent {
    pub instance_id: String,
    pub outcome: SyncApplyOutcome,
    /// スキップや失敗の理由の分類。`lastApplied.reason` と同じ文字列。
    pub reason: Option<String>,
}

impl SyncApplyFinishedEvent {
    /// 適用の結果からイベントを作る。参加していなければ送らないので `None`。
    pub fn from_outcome(instance_id: &str, outcome: &ApplyOutcome) -> Option<Self> {
        let (outcome, reason) = match outcome {
            ApplyOutcome::NotMember => return None,
            ApplyOutcome::Applied { .. } => (SyncApplyOutcome::Applied, None),
            ApplyOutcome::UpToDate => (SyncApplyOutcome::UpToDate, None),
            ApplyOutcome::Skipped(reason) => (SyncApplyOutcome::Skipped, Some(reason.code())),
            ApplyOutcome::Failed(failure) => {
                (SyncApplyOutcome::Failed, Some(failure.reason.code()))
            }
        };
        Some(SyncApplyFinishedEvent {
            instance_id: instance_id.to_string(),
            outcome,
            reason: reason.map(str::to_string),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::apply::{ApplyFailure, FailReason, SkipReason};

    #[test]
    fn finished_event_maps_each_outcome() {
        let event = |o: ApplyOutcome| SyncApplyFinishedEvent::from_outcome("i1", &o);
        assert_eq!(event(ApplyOutcome::NotMember), None);
        let pair = |o: ApplyOutcome| {
            let e = event(o).unwrap();
            assert_eq!(e.instance_id, "i1");
            (e.outcome, e.reason)
        };
        assert_eq!(
            pair(ApplyOutcome::Applied { changed: 2 }),
            (SyncApplyOutcome::Applied, None)
        );
        assert_eq!(
            pair(ApplyOutcome::UpToDate),
            (SyncApplyOutcome::UpToDate, None)
        );
        assert_eq!(
            pair(ApplyOutcome::Skipped(SkipReason::VersionMismatch)),
            (
                SyncApplyOutcome::Skipped,
                Some("versionMismatch".to_string())
            )
        );
        assert_eq!(
            pair(ApplyOutcome::Failed(ApplyFailure {
                reason: FailReason::ReadFailed,
                message: "x".into(),
            })),
            (SyncApplyOutcome::Failed, Some("readFailed".to_string()))
        );
    }
}
