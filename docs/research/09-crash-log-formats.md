# 09 クラッシュのログとレポートの書式

フェーズ 9 のクラッシュ診断が読む行の書式をまとめる。確認の状態は次の 3 つで示す。

- 確認済み：公開されたソースや、実際のログの例で確かめた。
- 未確認：書式の記憶や間接的な情報にもとづく。実機で確かめるまで、診断が見逃す可能性がある。

調査日は 2026-11-01。

## Fabric / Quilt

### 依存の不足・バージョン違い・非互換（読み込み前）

- 見出し：`Some of your mods are incompatible with the game or each other!`（確認済み。`ModResolver.java`）。
- 項目の文言は `Messages.properties`（FabricMC/fabric-loader の `src/main/resources/net/fabricmc/loader/`）にある（確認済み）。
  - `{0} {1} requires {3} of {2}, which is missing!`
  - `{0} {1} requires {3} of {2}, but only the wrong version is present: {4}!`（複数のときは `versions are`）
  - `{0} {1} is incompatible with {3} of {2}, yet a conflicting version is present: {4}!`
  - `recommends` と `conflicts` は起動を止めない警告なので、診断では使わない。
- `{0}` と `{2}` が実際にどう表示されるかは未確認。診断は `Mod '名前' (ID)` の形を想定する。この形に合わなければ手がかりは取れず、候補なしになる。原因として挙げるのは、先に出る MOD（`requires` の主語）である。

### Mixin の適用の失敗

- `Mixin apply for mod <modid> failed <modid>.mixins.json:<Mixin クラス> -> <対象クラス>`（確認済み。実際のログの例が複数ある）。
- Mixin の例外の見出し：`Mixin [<config>.json:<クラス>] from phase [DEFAULT] in config [<config>.json] FAILED during APPLY`（確認済み）。config 名の先頭から MOD ID を推定するのは確度が低いので、`Mixin apply for mod` の行だけを使う。

### 入口（entrypoint）の失敗

- `Could not execute entrypoint stage '<stage>' due to errors, provided by '<modid>'!`（未確認。記憶にもとづく）。

## Forge / NeoForge

### 依存の不足・バージョン違い

- ```
  [main/ERROR] [net.minecraftforge.fml.loading.ModSorter/LOADING]: Missing or unsupported mandatory dependencies:
  	Mod ID: 'forge', Requested by: 'puzzleslib', Expected range: '[47.1.71,)', Actual version: '47.1.3'
  ```
  （確認済み。Forge 1.20.1 の実際のログの例）。`Mod ID` が足りない側、`Requested by` が必要としている側。実際のバージョンが `[MISSING]` のとき、その MOD は入っていない（フォーラムの説明。未確認）。
- NeoForge の新しい版では書式が変わっている可能性がある（未確認）。

### 読み込みの失敗

- `ModLoadingException: <名前> (<modid>) has failed to load correctly`（未確認。記憶にもとづく）。

### クラッシュレポートの疑わしい MOD

- `Suspected Mods: <名前> (<modid>)`、または見出しのあとに字下げした行で並ぶ（未確認）。`Suspected Mods: NONE` は手がかりなし。

## スタックトレース

- `at <クラス>.<メソッド>(<ファイル>:<行>)`。Java 9 以降では `at java.base/java.lang.Thread.run(...)` のようにモジュール名やクラスローダー名が前に付く（`app//`、`knot//` など）。
- ゲーム本体とローダーとライブラリのパッケージ（`java.`、`net.minecraft.`、`org.spongepowered.` など）は除き、残ったクラスを入っている jar と結びつける。確度は低い。

## 実機で確かめること

未確認の項目は、フェーズ 9 の実機確認（計画の Task 6）で、実際のログと照らし合わせて直す。
