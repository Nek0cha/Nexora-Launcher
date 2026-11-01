# 公式ランチャーの保存形式

フェーズ 8 の読み取り部分（`import/official.rs`）のために、公開情報から確かめた事実と、確かめられなかった推測を分けて記す。利用者の PC のフォルダ（`%APPDATA%\.minecraft` など）は読んでいない。実装は合成したテスト用のデータだけで試験しており、本物の公式ランチャーのデータでの動作は確認していない。確認日は 2026-11-01。

関連：`docs/research/05-import-formats.md` の「1. Official Minecraft Launcher」（場所、フィールド、ローダーの判定規則の下調べ）。

## 公開情報で確かめたこと

- **場所**：Windows は `%APPDATA%\.minecraft`、macOS は `~/Library/Application Support/minecraft`、Linux は `~/.minecraft`（Flatpak は `~/.var/app/com.mojang.Minecraft/.minecraft`）。Fabric Installer の探索処理による（05 の記述）。
- **ファイル**：`.minecraft` 直下の `launcher_profiles.json`。Microsoft Store 版では `launcher_profiles_microsoft_store.json` の名前が使われる例がある（Fabric Installer、Forge の告知）。
- **構造**：最上位の `profiles` オブジェクトが、プロファイルの ID（キー）から定義への対応を持つ（Minecraft Wiki「Launcher profiles.json」）。
- **プロファイルのフィールド**（Minecraft Wiki）：`name`、`type`、`lastVersionId`、`gameDir`、`javaArgs`。`type` は `custom`（利用者が作ったもの）、`latest-release`、`latest-snapshot`。`lastVersionId` は `versions/` の下の版の ID。
- **NeoForge の版の番号**（NeoForged の公式ドキュメント）：`<Minecraft の 2 番目>.<3 番目>.<ビルド>`。`21.1.50` は Minecraft 1.21.1 向け。Minecraft 26.1 以降は表記が変わり、`26.1.0.5-beta` のように Minecraft の版全体を含む。
- **版の ID の慣習**（05 に記した Fabric Installer、Forge の資料）：Fabric は `fabric-loader-<ローダー>-<Minecraft>`、Quilt は同じ形で `quilt-loader-`、Forge は `<Minecraft>-forge-<ビルド>`、NeoForge は `neoforge-<版>`。
- **版の JSON**：`versions/<id>/<id>.json` に `inheritsFrom`（元の Minecraft の版）と `libraries[].name`（Maven 座標）がある。ローダーのライブラリは `net.fabricmc:fabric-loader:<v>`、`org.quiltmc:quilt-loader:<v>`、`net.minecraftforge:forge:<mc>-<v>`、`net.neoforged:neoforge:<v>`（05 の記述）。

## 確かめられなかったこと

- Wiki のこの節は「最新のランチャーでは古い可能性がある」と注記されている。現行のランチャーが書く JSON と食い違う可能性がある。
- `gameDir` が相対パスのとき、ランチャーが何を基準に解釈するか。Wiki に記載がない。
- `gameDir` が空または無いときの既定値。Wiki に記載がない。`.minecraft` 自身を使う、というのは広く知られた挙動だが、公式の文書では確かめていない。
- `latest-release` と `latest-snapshot` の `lastVersionId` の保存値。`type` と同じ文字列だと想定している。
- 古い Forge の版の ID の表記ゆれ（`1.7.10-Forge10.13.4.1614-1.7.10` など）。
- 1.20.1 の NeoForge（`net.neoforged:forge:1.20.1-47.1.x`）の版の ID。この読み取り部分は扱わない（`neoforge-` で始まるものだけ読む）。
- Quilt と NeoForge の判定規則の、歴代のインストーラーすべてでの網羅。
- Microsoft Store 版の Java の保存場所（05 を参照。推測のまま）。

## 実装での判断

- **1 プロファイルを 1 行にする**。`id` はプロファイルのキー。`versions/` のフォルダではなくキーを数える。
- **フォルダの共有**：`gameDir` が無い、または空白だけのプロファイルは、データフォルダ（`.minecraft`）そのものを使う。このため複数のプロファイルが同じ `mods`、`saves` を指すことがある。各行は自分のフォルダをそれぞれコピーするので、同じ中身を 2 つの Nexora インスタンスに取り込むことになりうる。重複を避けるかどうかは利用者が行を選ぶ段階で決める。
- **`gameDir` の検査**：読むだけの元データで、利用者が選んだ絶対パスなので、データフォルダの中に収まることは求めない。ただし、相対パス（基準が確かめられない）、`..` を含むもの、リンクのものは断り、その行を `unsupported` にする。コピー先の封じ込めはコピー側の共通処理が担う。
- **キーの検査**：キーはフォルダ名として使わず、`profiles` を引くだけである。それでも他の読み取り部分と揃え、`check_rel_path` を通らないキー（`../x` など）は `unsupported` にし、`source_path` は `mrpack.unsafe_path` を返す。
- **プロファイルの種類**：`type` では除外しない。公式の文書で、Java 版のゲーム以外のプロファイルを示す種類は確かめられなかった。`latest-release`、`latest-snapshot` は版が固定されないので `unsupported`（理由つき）にする。オブジェクトではない項目も `unsupported` の行にする。
- **版の読み取り順**：`versions/<id>/<id>.json` があり、`inheritsFrom` が正式版でローダーのライブラリが見つかれば、それを優先する。無い、壊れている、見つからなければ、ID の名前の形で読む。
- **対応する版**：`1.21` と `1.21.1` の形の正式版だけ。スナップショット、プレリリース、旧版（`b1.7.3`）は `unsupported`。版の一覧（マニフェスト）は読まないので、正式版かどうかは形だけで判断する。
- **ローダーの版の表記**：Forge と NeoForge はビルドだけ（`47.2.0`、`21.1.50`）。`mrpack::loader_version_for` が `<Minecraft>-<ビルド>` の形にも対応するため、導入処理にそのまま渡せる。
- **NeoForge の Minecraft の版**：`neoforge-<a>.<b>.<c>` から `1.<a>.<b>`（`b` が 0 なら `1.<a>`）を作る。`a` が 20 から 25 の範囲外、または 3 つに分かれないものは `unsupported`。JSON が使えればその `inheritsFrom` が優先される。
- **読み方の安全**：`launcher_profiles.json` は 64 MiB、版の JSON は 16 MiB を上限にする。認証情報（`authenticationDatabase`、`clientToken` など）は読まず、保存もしない。`name` と `lastVersionId` と `gameDir` だけを取り出す。
- **並び**：名前（小文字にして）、次にキーの順。名前が無ければキーを名前に使う。
- **見つかる場所**：`locate()` は上記の既定の場所のうち、`launcher_profiles*.json` があるものを返す。
