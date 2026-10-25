//! UI に渡す共通設定の型。保存層の型とは分け、camelCase の JSON と TypeScript の型にする。

use serde::Serialize;
use ts_rs::TS;

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
}

impl From<SyncProfile> for SyncProfileView {
    fn from(p: SyncProfile) -> Self {
        SyncProfileView {
            id: p.id,
            name: p.name,
            revision: p.revision,
            categories: p.categories,
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
    pub instance_id: String,
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
                .map(|(instance_id, proposal)| SingleProposal {
                    instance_id,
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
