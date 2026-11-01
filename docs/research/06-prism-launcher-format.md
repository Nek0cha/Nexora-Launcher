# Prism Launcher の保存形式

フェーズ 8 の読み取り部分（`import/prism.rs`）のために、Prism Launcher の公開リポジトリ（`PrismLauncher/PrismLauncher`、`develop` ブランチ）のソースから確かめた事実を記す。利用者の PC のフォルダは読んでいない。確認日は 2026-11-01。

## 確かめたこと

- **データフォルダ**：`QStandardPaths::AppDataLocation` の 1 つ上（`launcher/Application.cpp`）。組織名もアプリ名も `PrismLauncher`（`program_info/CMakeLists.txt` の `Launcher_CommonName`）なので、Windows は `%APPDATA%\PrismLauncher`、macOS は `~/Library/Application Support/PrismLauncher`、Linux は `$XDG_DATA_HOME/PrismLauncher`（既定 `~/.local/share/PrismLauncher`）になる。`dirs::data_dir()` の下の `PrismLauncher` と一致する。
- **場所を変える仕組み**：コマンドラインの `--dir`、環境変数 `PRISMLAUNCHER_DATA_DIR`、ポータブル版（実行ファイルの隣の `UserData` フォルダか `portable.txt`、macOS を除く）で、データフォルダが変わる。起動時にこのフォルダをカレントディレクトリにする。
- **設定ファイル**：データフォルダ直下の `prismlauncher.cfg`（`Launcher_ConfigFile` は `<バイナリ名>.cfg`、バイナリ名は `prismlauncher`）。旧名の `polymc.cfg`、`multimc.cfg` も読む。
- **`InstanceDir`**：設定キー。既定値は `instances`。相対パスはカレントディレクトリ（＝データフォルダ）からの相対になる。絶対パスも使える。
- **`AdditionalInstanceDirs`**：`develop` には追加のインスタンスフォルダを指定できる設定もある（文字列のリスト）。
- **インスタンスの見つけ方**（`launcher/InstanceList.cpp` の `discoverInstances`）：インスタンスのフォルダの直下のフォルダのうち、**中に `instance.cfg` があるもの**がインスタンスである。隠しフォルダも対象。`_LAUNCHER_TEMP`、`.tmp` などは `instance.cfg` を持たないので読まれない。インスタンスのフォルダ名がそのまま ID になる。
- **インスタンスの種類**：`instance.cfg` の `InstanceType` が空か `OneSix` のとき以外は、Prism は読み込まない。
- **`instance.cfg`**：キーと値の INI ファイル。表示名は `name`（未設定時の既定値は `Unnamed Instance`、`launcher/BaseInstance.cpp`）。新しい形式は Qt の `QSettings` の INI 形式（`[General]` 見出しがあり、`ConfigVersion` を持つ）で、`ConfigVersion` が無い古い形式は見出しの無い `key=value` の行で、`#` 以降がコメント、`\#`、`\n`、`\t` のエスケープがある（`launcher/settings/INIFile.cpp`）。値に `;`、`=`、`,` を含むと全体が `"` で囲まれることがある。
- **ゲームのフォルダ**（`MinecraftInstance::gameRoot`）：`.minecraft` があって `minecraft` が無いときだけ `.minecraft`、それ以外は `minecraft`。`mods`、`saves`、`config` などはその直下にある。
- **`mmc-pack.json`**（`launcher/minecraft/PackProfile.cpp`）：インスタンスのフォルダ直下。`formatVersion`（現在 1）と `components` の配列を持ち、各要素は `uid`、`version`（ほかに `important` などの項目がある）。Minecraft 本体は `uid` が `net.minecraft`。
- **ローダーの `uid`**（`launcher/minecraft/Component.cpp`）：`net.neoforged`（NeoForge）、`net.minecraftforge`（Forge）、`net.fabricmc.fabric-loader`（Fabric）、`org.quiltmc.quilt-loader`（Quilt）、`com.mumfrey.liteloader`（LiteLoader）。互いに同時には入らない扱い（衝突する組として定義されている）。Fabric は `net.fabricmc.intermediary`、Quilt は `org.quiltmc.hashed` を補助の部品として持つが、これはローダー本体ではない。

## 確かめられなかったこと

- **`AdditionalInstanceDirs` の保存形式**：INI の中でリストがどう書かれるか（カンマ区切りか、別の形か）は確かめていない。このため読み取り部分は `InstanceDir` の 1 つだけを読み、追加のフォルダは扱わない。
- **MultiMC 本体**：リポジトリは公開されているが、今回は読んでいない。Prism は MultiMC の形式を引き継いでいる（`polymc.cfg`、`multimc.cfg` を読み込む処理がある）ので、同じ形式なら読めるはずだが、確認していない。MultiMC のデータフォルダは配布先の隣（ポータブル）なので、自動では見つからず、利用者がフォルダを選ぶことになる。
- **Linux の Flatpak 版・Snap 版のデータフォルダ**：Snap は `SNAP_USER_COMMON` を使うことしか読んでおらず、Flatpak は読んでいない。自動では探さない（見つからなければ利用者がフォルダを選ぶ）。
- **ローダーの版の書式**：`net.minecraftforge` や `net.neoforged` の `version` の文字列が、Nexora のローダー版の書式（`.mrpack` の `dependencies` の値）と同じかは確かめていない。値はそのまま `loader_version` に渡す。
- **`instance.cfg` の細かい引用・エスケープ**：`QSettings` の INI 形式が書き出す `\xABCD` などの 16 進エスケープや、複数行の値は、実装では最低限しか扱っていない。名前に使われる範囲（普通の文字、`,`、`=`、`#`）だけを確かめた。
- **実際のデータ**：実際の Prism のデータでの動作確認はしていない。テストは、ソースから読み取った形に合わせて作った合成データだけである。

## 実装で決めたこと

- `PrismLauncher::at(root)` の `root` はデータフォルダ（`prismlauncher.cfg` があるところ）。インスタンスのフォルダは `root` から `InstanceDir`（既定 `instances`）で引く。
- 設定ファイルが無い、または `InstanceDir` が無いときは既定の `instances`。インスタンスのフォルダが無い、または読めないときは `ImportUnavailable`。
- インスタンスのフォルダの下に `instance.cfg` が無いものと、`InstanceType` が `OneSix` 以外のものは、Prism と同じく一覧に出さない。
- インスタンスのフォルダがリンクのとき、フォルダ名が安全でないとき、`mmc-pack.json` が無い・壊れている・`net.minecraft` の版が無いとき、ローダーが対応外（LiteLoader）のとき、ローダーが 2 つ以上あるときは、行を出したうえで `unsupported` に理由（英語）を入れる。
- 表示名は `name`。無いか空なら、フォルダ名。
