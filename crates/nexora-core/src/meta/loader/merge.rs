//! ローダーのプロファイルを Vanilla の version JSON に重ね、1 つの version JSON にする。

use std::collections::HashSet;

use crate::error::CoreError;
use crate::meta::loader::LoaderProfile;
use crate::meta::mojang::version::{Arguments, VersionJson};

/// `child` を `parent` に重ねる。`child.inherits_from` が `parent.id` と違えば `InvalidMetadata`。
/// id と mainClass は子のものにし、クライアントの jar は親のものを使う。引数は親のあとに子を続け、
/// ライブラリは子を先に並べ、子と同じもの（`group:artifact[:classifier]`）を親から除く。
/// それ以外は親のまま。
pub fn merge_profile(
    parent: &VersionJson,
    child: &LoaderProfile,
) -> Result<VersionJson, CoreError> {
    if child.inherits_from != parent.id {
        return Err(CoreError::InvalidMetadata {
            message: format!(
                "loader profile {} inherits from {}, not {}",
                child.id, child.inherits_from, parent.id
            ),
        });
    }

    let arguments = match (&parent.arguments, &child.arguments) {
        (Some(p), Some(c)) => Some(Arguments {
            game: p.game.iter().chain(&c.game).cloned().collect(),
            jvm: p.jvm.iter().chain(&c.jvm).cloned().collect(),
        }),
        (p, c) => p.clone().or_else(|| c.clone()),
    };

    let child_keys: HashSet<String> = child
        .libraries
        .iter()
        .map(|l| library_key(&l.name))
        .collect();
    let libraries = child
        .libraries
        .iter()
        .chain(
            parent
                .libraries
                .iter()
                .filter(|l| !child_keys.contains(&library_key(&l.name))),
        )
        .cloned()
        .collect();

    Ok(VersionJson {
        id: child.id.clone(),
        main_class: child.main_class.clone(),
        inherits_from: None,
        jar: Some(parent.id.clone()),
        arguments,
        libraries,
        ..parent.clone()
    })
}

/// ライブラリの同一性。`group:artifact:version[:classifier][@ext]` から版と拡張子を除く。
fn library_key(name: &str) -> String {
    let coords = name.split_once('@').map_or(name, |(c, _)| c);
    let parts: Vec<&str> = coords.split(':').collect();
    match parts.as_slice() {
        [group, artifact, _version] => format!("{group}:{artifact}"),
        [group, artifact, _version, classifier] => format!("{group}:{artifact}:{classifier}"),
        _ => coords.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::mojang::version::{ArgValue, Arguments, Library};
    use crate::meta::parse_json;

    const FABRIC_PROFILE: &str =
        include_str!("../../../tests/fixtures/loaders/fabric-profile-1.20.1-0.16.10.json");

    /// 1.21.5 のサンプルの id を 1.20.1 に書き換えたもの（Fabric のサンプルの親にする）。
    fn parent() -> VersionJson {
        let path = format!(
            "{}/tests/fixtures/versions/1.21.5.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let mut v: VersionJson =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        v.id = "1.20.1".into();
        v
    }

    fn fabric() -> LoaderProfile {
        parse_json(FABRIC_PROFILE.as_bytes(), "loader profile").unwrap()
    }

    fn library(name: &str) -> Library {
        serde_json::from_value(serde_json::json!({ "name": name })).unwrap()
    }

    fn plain(args: &[ArgValue]) -> Vec<String> {
        args.iter()
            .filter_map(|a| match a {
                ArgValue::Plain(s) => Some(s.clone()),
                ArgValue::Conditional { .. } => None,
            })
            .collect()
    }

    #[test]
    fn merge_overrides_main_class_and_keeps_vanilla_fields() {
        let parent = parent();
        let merged = merge_profile(&parent, &fabric()).unwrap();
        assert_eq!(merged.id, "fabric-loader-0.16.10-1.20.1");
        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert!(merged.inherits_from.is_none());
        assert_eq!(merged.jar.as_deref(), Some("1.20.1"));
        // 型は PartialEq を持たないため、Debug の表現で比べる
        assert_eq!(
            format!("{:?}", merged.asset_index),
            format!("{:?}", parent.asset_index)
        );
        assert_eq!(
            format!("{:?}", merged.downloads),
            format!("{:?}", parent.downloads)
        );
        assert_eq!(
            format!("{:?}", merged.java_version),
            format!("{:?}", parent.java_version)
        );
        assert_eq!(
            format!("{:?}", merged.logging),
            format!("{:?}", parent.logging)
        );
        assert_eq!(merged.kind, parent.kind);
    }

    #[test]
    fn child_libraries_come_first_and_win_conflicts() {
        let mut parent = parent();
        parent.libraries.push(library("org.ow2.asm:asm:9.6"));
        parent
            .libraries
            .push(library("org.ow2.asm:asm:9.6:natives-windows"));
        let child = fabric();
        let merged = merge_profile(&parent, &child).unwrap();

        let names: Vec<&str> = merged.libraries.iter().map(|l| l.name.as_str()).collect();
        let child_names: Vec<&str> = child.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(&names[..child_names.len()], child_names.as_slice());

        let asm: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| n.starts_with("org.ow2.asm:asm:"))
            .collect();
        assert_eq!(
            asm,
            vec![
                "org.ow2.asm:asm:9.7.1",
                "org.ow2.asm:asm:9.6:natives-windows"
            ]
        );
        // 衝突しない親のライブラリは順を保って残る
        let parent_rest: Vec<&str> = parent
            .libraries
            .iter()
            .map(|l| l.name.as_str())
            .filter(|n| *n != "org.ow2.asm:asm:9.6")
            .collect();
        assert_eq!(&names[child_names.len()..], parent_rest.as_slice());
    }

    #[test]
    fn arguments_are_concatenated_parent_first() {
        let parent = parent();
        let parent_args = parent.arguments.clone().unwrap();
        let mut child = fabric();
        child.arguments = Some(Arguments {
            game: vec![ArgValue::Plain("--child-game".into())],
            jvm: vec![ArgValue::Plain("-Dchild=jvm".into())],
        });
        let merged = merge_profile(&parent, &child).unwrap();
        let args = merged.arguments.unwrap();
        assert_eq!(args.game.len(), parent_args.game.len() + 1);
        assert_eq!(args.jvm.len(), parent_args.jvm.len() + 1);
        assert_eq!(
            format!("{:?}", &args.game[..parent_args.game.len()]),
            format!("{:?}", parent_args.game)
        );
        assert_eq!(
            format!("{:?}", &args.jvm[..parent_args.jvm.len()]),
            format!("{:?}", parent_args.jvm)
        );
        assert_eq!(plain(&args.game).last().unwrap(), "--child-game");
        assert_eq!(plain(&args.jvm).last().unwrap(), "-Dchild=jvm");

        // 子に arguments が無ければ親のまま
        child.arguments = None;
        let merged = merge_profile(&parent, &child).unwrap();
        assert_eq!(
            format!("{:?}", merged.arguments),
            format!("{:?}", parent.arguments)
        );
    }

    #[test]
    fn jvm_argument_with_spaces_stays_one_argument() {
        let merged = merge_profile(&parent(), &fabric()).unwrap();
        let jvm = plain(&merged.arguments.unwrap().jvm);
        assert_eq!(
            jvm.iter()
                .filter(|a| a.as_str() == "-DFabricMcEmu= net.minecraft.client.main.Main ")
                .count(),
            1
        );
    }

    #[test]
    fn mismatched_parent_is_invalid_metadata() {
        let mut parent = parent();
        parent.id = "1.21.5".into();
        let err = merge_profile(&parent, &fabric()).unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }

    #[test]
    fn library_keys_ignore_version_and_extension() {
        assert_eq!(library_key("org.ow2.asm:asm:9.7.1"), "org.ow2.asm:asm");
        assert_eq!(
            library_key("org.lwjgl:lwjgl:3.3.3:natives-windows"),
            "org.lwjgl:lwjgl:natives-windows"
        );
        assert_eq!(library_key("a.b:c:1@zip"), "a.b:c");
    }

    /// 座標の数が合わない名前は、拡張子だけを除いた名前そのものを鍵にする。
    /// そのため、同じ名前どうしでなければ衝突せず、親の側も消されない。
    #[test]
    fn malformed_library_names_fall_back_to_the_whole_name() {
        assert_eq!(library_key("broken"), "broken");
        assert_eq!(library_key("a:b"), "a:b");
        assert_eq!(library_key("a:b:c:d:e@zip"), "a:b:c:d:e");
        assert_eq!(library_key(""), "");
    }
}
