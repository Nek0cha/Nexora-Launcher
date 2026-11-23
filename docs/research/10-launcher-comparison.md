# ほかのランチャーとの比較（公式サイトの比較表のための調査）

確認日：2026-11-23。公式サイトの比較表には、ここで「はい／いいえ」を出典つきで確かめた行だけを載せる（`docs/specs/2026-11-23-website-design.md` の 6 節）。

## 確かめた結果

| 行 | Nexora | Prism Launcher | Modrinth App | 公式ランチャー |
|---|---|---|---|---|
| ライセンス | Apache-2.0 | GPL-3.0 [1] | GPL-3.0 only [2] | 非公開（Minecraft の利用規約で配布）[3] |
| ソースコードの公開 | はい | はい [1] | はい [2] | いいえ [3] |
| 作り | Rust ＋ Tauri | C++ ＋ Qt [1] | Rust ＋ Tauri [2] | 不明（非公開） |
| インスタンス間の設定の共有 | はい（共通設定） | 不明（本体の機能は見つからず、外部のスクリプトと MOD がある [4]） | はい（設定の同期 [5]） | 不明 |
| ほかのランチャーからの取り込み | はい（4 種） | 不明 | はい（MultiMC、GDLauncher、ATLauncher、CurseForge、Prism [6]） | 不明 |
| バージョンを上げる前の MOD の対応確認 | はい | 不明 | 不明 | 不明 |
| `.mrpack` の読み書き | はい | 不明 | はい（Modrinth の形式） | 不明 |

Nexora の依存のライセンス：`ui/src/assets/third-party-licenses.json` の 532 件のうち GPL・LGPL は 0 件、MPL-2.0 が 5 件（`cssparser`、`cssparser-macros`、`dtoa-short`、`option-ext`、`selectors`）、残りは MIT、Apache-2.0、BSD、ISC、Zlib、Unicode-3.0、OFL-1.1（フォント）など。MPL-2.0 はファイル単位の条件で、Nexora 全体のライセンスには及ばない。

## 採用する行

- ライセンス
- ソースコードの公開
- 作り（公式ランチャーは「非公開」と書く）

「不明」が残る行は載せない。設定の共有と取り込みは、Modrinth App にもあるため、Nexora だけの違いにもならない。

結論：比較表で Nexora だけが違うのはライセンスの行である（ソースを公開しているランチャーの中で、GPL ではない）。表はこの 1 点を正直に見せるために使い、ほかの強み（共通設定、バージョンアップ前の確認など）は比較ではなく機能の一覧で伝える。

## 出典

1. https://github.com/PrismLauncher/PrismLauncher/blob/develop/LICENSE （GitHub API の判定 GPL-3.0）、リポジトリの主な言語は C++、UI は Qt。
2. https://github.com/modrinth/code/blob/main/apps/app/COPYING.md 「licensed under the GNU General Public License, Version 3 only」、`apps/app` は Tauri の構成（`tauri.conf.json`、`Cargo.toml`）。
3. https://www.minecraft.net/en-us/eula
4. https://codeberg.org/vojtiseek/mcsync （Prism 向けの options.txt 同期スクリプト）、https://modrinth.com/mod/keybind-sync
5. https://modrinth.com/news/article/sync-settings/
6. https://modrinth.com/news/article/modrinth-app-beta
