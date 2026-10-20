# テスト用のサンプル

`versions/` は Mojang の version JSON の実物である（取得日：2026-10-15）。
`latest-release.json` は取得時点の最新の正式版で、ID は `26.3` である。

| ファイル | ID | sha1 |
|---|---|---|
| 1.8.9.json | 1.8.9 | d546f1707a3f2b7d034eece5ea2e311eda875787 |
| 1.12.2.json | 1.12.2 | 832d95b9f40699d4961394dcf6cf549e65f15dc5 |
| 1.13.json | 1.13 | c24c2fd37c8ca2e1c18721e2c77caf4d24c87f92 |
| 1.21.5.json | 1.21.5 | b341cd0e91419ab72b5d9dadad8d86a045d6b9a7 |
| latest-release.json | 26.3 | 11f1aefe473eefba0af62e3286c33b692807b2eb |

## loaders/

`loaders/` は Fabric と Quilt の公式メタデータの実物である（取得日：2026-10-20）。

| ファイル | 取得元 | 加工 |
|---|---|---|
| fabric-game.json | https://meta.fabricmc.net/v2/versions/game | 先頭の 6 件に切り詰めた |
| fabric-loader-1.20.1.json | https://meta.fabricmc.net/v2/versions/loader/1.20.1 | 先頭の 4 件に切り詰めた（先頭の 2 件は `stable: false`） |
| fabric-profile-1.20.1-0.16.10.json | https://meta.fabricmc.net/v2/versions/loader/1.20.1/0.16.10/profile/json | なし |
| quilt-game.json | https://meta.quiltmc.org/v3/versions/game | 先頭の 6 件に切り詰めた |
| quilt-loader-1.20.1.json | https://meta.quiltmc.org/v3/versions/loader/1.20.1 | 先頭の 6 件に、`0.31.0-beta.4`、`0.28.0`、`0.28.1` の要素を元の順で足した |
| quilt-profile-1.20.1-0.28.1.json | https://meta.quiltmc.org/v3/versions/loader/1.20.1/0.28.1/profile/json | なし |

Quilt の game 一覧は Fabric と同じ形（`version`、`stable`）である。
Quilt のゲームごとのローダー一覧は新しい順に並んでおらず、先頭は試験版（`0.20.0-beta.9`）である。

## forge/

`forge/` は Forge と NeoForge の公式インストーラ（jar）から取り出した `install_profile.json` と `version.json`、および Maven のメタデータの実物である（取得日：2026-10-20）。インストーラは実行しておらず、`unzip` で読んだだけである。各インストーラの `.sha1`（サイドカー）はダウンロードしたファイルの SHA-1 と一致することを確かめた。

| ファイル | 取得元 | 加工 |
|---|---|---|
| forge-1.16.5-install_profile.json、forge-1.16.5-version.json | https://maven.minecraftforge.net/net/minecraftforge/forge/1.16.5-36.2.39/forge-1.16.5-36.2.39-installer.jar | jar の中身をそのまま |
| forge-1.20.1-install_profile.json、forge-1.20.1-version.json | https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar | 同上 |
| neoforge-21.1-install_profile.json、neoforge-21.1-version.json | https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.172/neoforge-21.1.172-installer.jar | 同上（`net.neoforged:neoforge`、Minecraft 1.21.1） |
| neoforge-1.20.1-install_profile.json、neoforge-1.20.1-version.json | https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar | 同上（`net.neoforged:forge`） |
| forge-26.3-install_profile.json、forge-26.3-version.json | https://maven.minecraftforge.net/net/minecraftforge/forge/26.3-66.0.13/forge-26.3-66.0.13-installer.jar | jar の中身をそのまま（26.x の形の確認用） |
| neoforge-26.3-install_profile.json、neoforge-26.3-version.json | https://maven.neoforged.net/releases/net/neoforged/neoforge/26.3.0.95/neoforge-26.3.0.95-installer.jar | 同上（`net.neoforged:neoforge`、Minecraft 26.3） |
| forge-legacy-2847-install_profile.json | https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2847/forge-1.12.2-14.23.5.2847-installer.jar | `install_profile.json` のみ。旧形式（入れ子）で、断るテスト用 |
| forge-maven-metadata.xml | https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml | `<version>` を先頭 12 件、末尾 12 件、下記の代表 8 件に切り詰めた（元は 5093 件）。元の順のまま |
| neoforge-maven-metadata.xml | https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml | 先頭 12 件、末尾 12 件、代表 5 件に切り詰めた（元は 1846 件）。26.x の確認用に `26.1.0.19-beta`、`26.1.1.15-beta`、`26.1.2.122`、`26.2.0.93` を実物の一覧から追加 |
| neoforge-forge-maven-metadata.xml | https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml | 先頭 12 件、末尾 12 件に切り詰めた（元は 63 件） |
| forge-promotions_slim.json | https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json | `promos` を先頭 12 件、末尾 12 件、1.7.10 / 1.12.2 / 1.16.5 / 1.20.1 の latest と recommended に切り詰めた（元は 117 件） |

メタデータに残した代表的な版：
- Forge：`1.7.10-10.13.4.1614-1.7.10`（末尾に Minecraft の版が付く）、`1.12.2-14.23.4.2720-4627`（末尾が数字）、`1.11-13.19.0.2129-1.11.x`、`1.10.2-12.18.1.2016-failtests`（末尾が文字）、`1.12.2-14.23.5.2847`、`1.16.5-36.2.39`、`1.20.1-47.3.0`、`26.1-62.0.0`。元データでは 5093 件のうち 378 件が `<mc>-<数字の組>` の形ではない。
- NeoForge：`0.25w14craftmine.3-beta`、`20.2.3-beta`、`21.0.0-beta`（`-beta` 付き）、`26.1.0.0-alpha.1+snapshot-1`、`26.1.0.0-alpha.15+pre-3`（`+snapshot` / `+pre` 付き）、`20.4.237`、`21.1.172`、`26.3.0.9x`。
- メタデータの並びは新しい順でも版の順でもない（NeoForge の末尾に `21.1.263` が `26.3.0.92` の後ろに出る）。`<latest>` は Forge が `26.3-66.0.13`、NeoForge が `26.3.0.95`。

### インストーラの調査結果

| 項目 | Forge 1.16.5-36.2.39 | Forge 1.20.1-47.3.0 | NeoForge 21.1.172 | NeoForge(forge) 1.20.1-47.1.106 |
|---|---|---|---|---|
| `spec` | 0 | 1 | 1 | 1 |
| `json` | /version.json | /version.json | /version.json | /version.json |
| `path`（トップレベル） | あり（`net.minecraftforge:forge:1.16.5-36.2.39`） | なし | なし | なし |
| `processors` の数 | 4 | 10 | 10 | 10 |
| `sides` を持つ processor | 0 | 5（server 4、client 1） | 5（server 4、client 1） | 5（server 4、client 1） |
| `outputs` を持つ processor | 2 | 2（jarsplitter の client と server。`{MC_SLIM}`、`{MC_EXTRA}`） | 0 | 2（1.20.1 と同じ） |
| `data` の値の形 | `[..]` 5、`/..` 1、`'..'` 4（client と server で同数） | `[..]` 8、`/..` 1、`'..'` 4 | `[..]` 8、`/..` 1、`'..'` 1 | `[..]` 8、`/..` 1、`'..'` 4 |
| `PATCHED_SHA` / `MC_SLIM_SHA` | どちらもある | どちらもある（`PATCHED_SHA` は `outputs` で使われていない） | どちらも無い | どちらもある |
| `libraries`（install_profile）で `downloads.artifact.url` が空 | `net.minecraftforge:forge:1.16.5-36.2.39:universal` の 1 件 | なし | なし | なし |
| `libraries`（version.json）で `url` が空 | `net.minecraftforge:forge:1.16.5-36.2.39` の 1 件 | なし | なし | なし |
| jar 内の `maven/` | あり（`forge-1.16.5-36.2.39.jar` と `-universal.jar`） | なし | なし | あり（`-universal.jar` のみ） |
| jar 内の `data/*.lzma` | あり | あり | あり | あり |

- `data` の `/..` は `BINPATCH`（`/data/client.lzma`、`/data/server.lzma`）だけ。`'..'` は `MC_SLIM_SHA`、`MC_EXTRA_SHA`、`PATCHED_SHA`、`MCP_VERSION` の類で、NeoForge 21.1 は `MCP_VERSION` のみ。`[..]` は `MAPPINGS`、`PATCHED`、`MC_SRG`、`MC_SLIM`、`MC_EXTRA`、`MOJMAPS`、`MERGED_MAPPINGS`、`MC_UNPACKED`。
- Forge 1.16.5 は `sides` が無く、全 processor が client と server の両方で動く。1.16.5 の最終 jar（`forge-1.16.5-36.2.39.jar`）は jar 内の `maven/` からコピーするもので、processor の `PATCHED` 出力（`...:client`）は別の classifier である。
- Forge 1.20.1 と NeoForge 21.1 は最終 jar（クライアントは `...:client`）を processor が生成し、jar 内に `maven/` が無い。`universal` は install_profile の `libraries` に URL 付きである（NeoForge 1.20.1 も `universal` は URL 付き。ただし jar 内の `maven/` にも同じものがある）。
- processor の引数に出るトークン：`{BINPATCH}`、`{MAPPINGS}`、`{MC_EXTRA}`、`{MC_SLIM}`、`{MC_SRG}`、`{MINECRAFT_JAR}`、`{PATCHED}`。spec 1 では加えて `{INSTALLER}`、`{MC_UNPACKED}`、`{MERGED_MAPPINGS}`、`{MOJMAPS}`、`{ROOT}`、`{SIDE}`。
- processor の `jar` と、jar の `META-INF/MANIFEST.MF` の `Main-Class`（jar は Maven から取得し、manifest を読んだだけ。実行していない。取得した jar の SHA-1 は profile の値と一致）：

| processor の jar | Main-Class |
|---|---|
| net.minecraftforge:installertools:1.2.6 / 1.3.0 / 1.4.1 | net.minecraftforge.installertools.ConsoleTool |
| net.minecraftforge:jarsplitter:1.1.4 | net.minecraftforge.jarsplitter.ConsoleTool |
| net.minecraftforge:binarypatcher:1.0.12 / 1.1.1 | net.minecraftforge.binarypatcher.ConsoleTool |
| net.md-5:SpecialSource:1.8.5（1.16.5 のみ） | net.md_5.specialsource.SpecialSource |
| net.minecraftforge:ForgeAutoRenamingTool:0.1.22:all（1.20.1 の 2 つ） | net.minecraftforge.fart.Main |
| net.neoforged.installertools:installertools:2.1.2 | net.neoforged.installertools.ConsoleTool |
| net.neoforged.installertools:jarsplitter:2.1.2 | net.neoforged.jarsplitter.ConsoleTool |
| net.neoforged.installertools:binarypatcher:2.1.2:fatjar | net.neoforged.binarypatcher.ConsoleTool |
| net.neoforged:AutoRenamingTool:2.0.3:all | net.neoforged.art.Main |

NeoForge(forge) 1.20.1 の processor は `net.minecraftforge:installertools:1.3.0`、`jarsplitter:1.1.4`、`ForgeAutoRenamingTool:0.1.22:all`、`binarypatcher:1.1.1` で、Forge 1.20.1 と installertools の版だけ違う。URL は `https://maven.neoforged.net/releases/net/minecraftforge/...` である。

### version.json の調査結果

- `jar` フィールドは 4 つとも無い。`inheritsFrom` は Minecraft の版（`1.16.5`、`1.20.1`、`1.21.1`）で、`id` は `1.16.5-forge-36.2.39`、`1.20.1-forge-47.3.0`、`neoforge-21.1.172`、`1.20.1-forge-47.1.106`。`mainClass` は 1.16.5 が `cpw.mods.modlauncher.Launcher`、他は `cpw.mods.bootstraplauncher.BootstrapLauncher`。
- `arguments.game` の要素はすべて文字列で、`${...}` のトークンを 1 つも含まない（`--launchTarget`、`--fml.*` の固定値だけ）。`minecraftArguments` は無い。NeoForge 21.1 は `logging` も無い。
- `arguments.jvm` のトークンは `${library_directory}`、`${classpath_separator}`、`${version_name}`（1.16.5 はトークン無し）。`${classpath_separator}` と `${library_directory}` は Mojang の version JSON には無く、ランチャーが与える必要がある。`-DignoreList` の値の中にも `${version_name}` がある。`-p` の値は 1 つの文字列で、`${classpath_separator}` で連結されている。
- version.json の `libraries` で `url` が空なのは Forge 1.16.5 の `net.minecraftforge:forge:1.16.5-36.2.39` のみ（jar 内 `maven/` から配置する）。

### 旧形式（forge-legacy-2847）

トップレベルは `install`、`versionInfo`、`optionals` で、`spec` は無い。`versionInfo` は `id`、`time`、`releaseTime`、`type`、`minecraftArguments`、`mainClass`、`inheritsFrom`、`jar`、`logging`、`libraries`。この形式は `jar` と `minecraftArguments` を持つ。jar の中に `forge-1.12.2-14.23.5.2847-universal.jar` がある。
