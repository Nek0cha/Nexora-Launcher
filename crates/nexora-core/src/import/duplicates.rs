//! すでにあるインスタンスと重複する読み込みを見つける。

use std::collections::HashSet;

use super::SourceInstance;
use crate::instance::Instance;
use crate::meta::loader::LoaderKind;

type Key = (String, String, &'static str);

fn key(name: &str, mc_version: &str, loader: LoaderKind) -> Key {
    (name.trim().to_lowercase(), mc_version.to_string(), loader.as_str())
}

/// 選ばれた行のうち、Nexora のインスタンスか、選択の中の先の行と
/// 名前（大文字小文字と前後の空白は無視）・Minecraft の版・ローダーが同じものの `id`。選択の順で返す。
pub fn find_duplicates(existing: &[Instance], picked: &[SourceInstance]) -> Vec<String> {
    let mut seen: HashSet<Key> = existing
        .iter()
        .map(|i| key(&i.name, &i.mc_version, i.loader))
        .collect();
    picked
        .iter()
        .filter(|p| !seen.insert(key(&p.name, &p.mc_version, p.loader)))
        .map(|p| p.id.clone())
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{Instance, InstanceState, JavaMode, MemoryMode};
    use crate::meta::loader::LoaderKind;

    fn existing(name: &str, mc: &str, loader: LoaderKind) -> Instance {
        Instance {
            id: "i".into(),
            name: name.into(),
            dir_name: name.into(),
            mc_version: mc.into(),
            state: InstanceState::Ready,
            created_at: 0,
            last_played_at: None,
            play_time_ms: 0,
            memory_mode: MemoryMode::Auto,
            memory_mb: None,
            java_mode: JavaMode::Auto,
            java_path: None,
            jvm_args: String::new(),
            loader,
            loader_version: None,
        }
    }

    fn picked(id: &str, name: &str, mc: &str, loader: LoaderKind) -> SourceInstance {
        SourceInstance {
            id: id.into(),
            name: name.into(),
            mc_version: mc.into(),
            loader,
            loader_version: None,
            available: vec![],
            unsupported: None,
        }
    }

    #[test]
    fn an_existing_instance_with_the_same_name_version_and_loader_is_a_duplicate() {
        let have = [existing("Survival", "1.21.1", LoaderKind::Fabric)];
        let dup = find_duplicates(&have, &[picked("a", "Survival", "1.21.1", LoaderKind::Fabric)]);
        assert_eq!(dup, ["a"]);
    }

    #[test]
    fn a_different_version_or_loader_is_not_a_duplicate() {
        let have = [existing("Survival", "1.21.1", LoaderKind::Fabric)];
        let none = find_duplicates(
            &have,
            &[
                picked("a", "Survival", "1.20.1", LoaderKind::Fabric),
                picked("b", "Survival", "1.21.1", LoaderKind::Forge),
            ],
        );
        assert!(none.is_empty());
    }

    #[test]
    fn the_second_of_two_identical_rows_in_the_selection_is_a_duplicate() {
        let dup = find_duplicates(
            &[],
            &[
                picked("a", "Pack", "1.21", LoaderKind::Vanilla),
                picked("b", "Pack", "1.21", LoaderKind::Vanilla),
            ],
        );
        assert_eq!(dup, ["b"]);
    }

    #[test]
    fn names_are_compared_without_case_or_surrounding_spaces() {
        let have = [existing("survival", "1.21", LoaderKind::Vanilla)];
        let dup = find_duplicates(&have, &[picked("a", "  SURVIVAL ", "1.21", LoaderKind::Vanilla)]);
        assert_eq!(dup, ["a"]);
    }
}
