//! 実行している機械の情報。

use sysinfo::System;

/// 物理メモリの合計（MB）。`u32` に収まるよう丸める。
pub fn total_memory_mb() -> u32 {
    let mb = System::new_with_specifics(
        sysinfo::RefreshKind::nothing()
            .with_memory(sysinfo::MemoryRefreshKind::nothing().with_ram()),
    )
    .total_memory()
        / (1024 * 1024);
    u32::try_from(mb).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_memory_mb_is_positive() {
        assert!(total_memory_mb() > 0);
    }
}
