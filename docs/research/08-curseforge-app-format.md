# CurseForge アプリの保存形式

フェーズ 8 の読み取り部分（`import/curseforge.rs`）のために、公開情報から確かめた事実と、推測を分けて記す。**CurseForge アプリは開発者の PC に入っておらず、本物のインストールでは何も確かめていない。** 利用者の PC のフォルダも読んでいない。確認日は 2026-11-01。根拠は次の公開情報だけである。

- CurseForge の公式サポート記事（インストール先の説明、`support.curseforge.com/support/solutions/articles/9000218572`）。`docs/research/05-import-formats.md` 第 4 節に引用済み。
- Modrinth App の CurseForge 読み取り（`modrinth/code`、`packages/app-lib/src/api/pack/import/curseforge.rs`）。
- GDLauncher Carbon の CurseForge 読み取り（`crates/carbon_app/src/managers/instance/importer/curseforge.rs`）。
- 公開リポジトリにある実物の `minecraftinstance.json`（Create Together、`05-import-formats.md` に固定リビジョンあり）。

## 公開情報から確かめたこと

- **場所**：Minecraft のフォルダの既定は、Windows が `%USERPROFILE%\curseforge\minecraft`、macOS と Linux が `~/Documents/curseforge/minecraft`（公式記事、GDLauncher の既定の探索先も同じ）。インスタンスは `<root>/Instances/<名前>/`。
- **インスタンスの判定**：`Instances` の直下のフォルダのうち、`minecraftinstance.json` があるもの（GDLauncher）。そのフォルダ自身がゲームのフォルダで、`mods`、`saves` などが直下にある。
- **読まれているフィールド**（両者に共通）：`name`、`gameVersion`（文字列、Minecraft の版）、`baseModLoader`（無い、または null ならバニラ）の `name`。ほかに `projectID`、`fileID`、`installedModpack.thumbnailUrl`、`profileImagePath`、`lastPlayed`、`isUnlocked`（GDLauncher）。
- **ローダー名の形**：`baseModLoader.name` をハイフンで分けて判定している（Modrinth）。`forge-<版>`（例：`forge-47.2.0`）、`fabric-<ローダーの版>-<Minecraft の版>`（例：`fabric-0.16.10-1.21.1`）。GDLauncher は `fabric`、`quilt` で始まる名前の末尾の `-<版>` を落としているので、Quilt も同じ形（`quilt-<ローダーの版>-<Minecraft の版>`）である。Modrinth は上記以外をバニラにしてしまうが、Nexora は黙ってバニラにせず `unsupported` にする。
- **設定画面でインストール先を変えられる**（公式記事）。

## 推測

- **NeoForge の名前**：`neoforge-<版>`（例：`neoforge-21.1.77`）と推測した。公開ソースでは確かめられていない。違う形だった場合は `unsupported`（未対応のローダー）になるだけで、取り込みは壊れない。
- **Forge の版**：`forge-` の後ろ全体を版とした。古い版で `forge-<Minecraft の版>-<版>` のような形があり得るかは確かめていない。
- **インストール先を変えたときの記録場所**：公開情報からは、設定ファイルの名前もキーも分からなかった。自動では探さない。見つからない（既定の場所に `Instances` が無い）ときは、spec 5.1 のとおり利用者にフォルダを選んでもらう。
- **macOS と Linux の既定**：`~/Documents/curseforge/minecraft`。公式記事と GDLauncher の記述に基づくが、実機では確かめていない。Overwolf 版や古い Twitch 版の場所は扱わない。
- **`minecraftinstance.json` の型**：`gameVersion` と `baseModLoader.name` は文字列と推測。文字列でなければ無いものとして扱う（版が無ければ `unsupported`）。

## 実装での判断

- **フォルダの選び方**：選ばれたフォルダの下に `Instances` があればそれを使う。無くても、選ばれたフォルダの直下に `minecraftinstance.json` を持つフォルダがあれば、そのフォルダを `Instances` として扱う。どちらでもなければ `ImportUnavailable`。
- **インスタンス ID**：`Instances` の下のフォルダ名。`check_rel_path` で確かめ、`source_path` でも同じ確認をする。表示名は `name`、無ければフォルダ名。
- **`minecraftinstance.json` が読めない／壊れている**：一覧全体を失敗にはせず、その行だけ `unsupported`（理由付き）にする。ほかのインスタンスは使えるようにするため。メタデータの無いフォルダは、インスタンスではないので無視する。
- **ローダー**：`baseModLoader` が無い、null、`name` が空ならバニラ。`gameVersion` が空なら `unsupported`。知らない名前、版のない名前（`forge` だけ）は `unsupported`。
- **リンク**：リンクやジャンクションのインスタンスのフォルダ、部分はたどらない（無いものとして扱う）。
- **読むのは `minecraftinstance.json` だけ**：アカウントや認証に関わるファイル、`installPath` などの絶対パスは取り込まない。元のファイルは書き換えない。
