# Nexora Launcher 全体設計（spec 0）

- 作成日：2026-10-15
- 状態：レビュー待ち
- 位置づけ：プロジェクト全体の設計の土台である。各フェーズの詳細は、この文書を前提とした個別の spec で定める。

## 1. 概要

| 項目 | 内容 |
|---|---|
| 正式名称 | Nexora Launcher |
| アプリ内表示・ウィンドウタイトル | Nexora |
| リポジトリ | Nexora-Launcher |
| 説明文 | A modern, lightweight Minecraft Java Edition launcher. |

Minecraft Java Edition 向けのデスクトップランチャーである。環境構築、Mod 管理、設定変更にかかる手間を最小にし、直感的かつ高速に使えることを目標とする。個人開発の OSS として公開する。

### 1.1 目標

- 初心者が詳細設定を意識せずに起動できること。上級者向けの詳細設定も用意する。
- 起動速度とメモリ消費を重視する。
- オフラインでも、インストール済みのインスタンスを起動できること。
- ファイル操作の失敗によってデータを壊さないこと。

### 1.2 対象外

- クラウド同期と外部サーバーの運用。設定同期は同一 PC 内のインスタンス間に限る。
- Bedrock Edition。
- サーバー（Minecraft サーバー本体やプラグイン）の管理。

## 2. 前提と決定事項

| 項目 | 決定 |
|---|---|
| ライセンス | **要決定**（Apache-2.0 を予定）。公開前に正式に決める。 |
| 実装方針 | ゲームのインストールと起動を含めて自前で実装する。外部のランチャー用ライブラリには依存しない。 |
| 他のランチャーのコード | GPL のコード（Prism Launcher、MultiMC、Modrinth App）は、仕様や挙動の確認のために読むだけとし、複製しない。portablemc はリビジョンによってライセンスが異なる。v4 以前（Python 版）は GPL-3.0 であり、読むだけとする。v5 以降（Rust 版、2026-10 時点の main は Apache-2.0 を確認済み）は参考資料として読み、取り込む場合は Apache-2.0 の条件（ライセンス文の同梱、著作権表示の保持、変更の明記）に従う。取り込む前に、対象リビジョンのライセンスを確認する。 |
| 対応 OS | Windows 11 を先に作る。Linux と macOS も設計段階から考慮し、CI で 3 つの OS のビルドとテストを最初から行う。 |
| 対応する Minecraft バージョン | **1.8.9 以降**。1.8.9 より前は対象外とする。 |
| UI の言語 | 日本語と英語。最初から i18n の仕組みを入れる。 |
| UI 部品 | React Aria Components と Tailwind CSS v4。shadcn/ui は使わない。 |
| デザイン | `versatile-design`（web スキル）と `apple-design` の 2 つのスキルに従う。詳細はフェーズ 1 の spec で定める。 |
| インポート方式 | コピーして取り込む。元のランチャーのファイルには書き込まない。 |
| インポートの優先順位 | 公式ランチャー ＞＞ Prism Launcher / MultiMC ＞ Modrinth App ＞ CurseForge App ＞＞ GDLauncher |
| CurseForge 連携 | 後回しにする。CurseForge から書面で許可を得たうえで、公式ビルドにだけ API キーを埋め込む。ソースからのビルドでは CurseForge の機能を無効にする。 |

### 2.1 技術スタック

| 層 | 採用 |
|---|---|
| デスクトップ | Tauri 2 |
| フロントエンド | React、TypeScript、Vite、Tailwind CSS v4、React Aria Components、i18next |
| バックエンド | Rust、Tokio |
| HTTP | reqwest |
| ローカルデータ | SQLite（rusqlite） |
| シリアライズ | Serde |

### 2.2 調査資料

各サブシステムの詳細な根拠は `docs/research/` にある。spec はこれらを根拠として参照する。資料の中で「UNVERIFIED」とされた項目は、実装の前に該当するフェーズで確認する。

| ファイル | 内容 |
|---|---|
| `01-mojang-install-launch.md` | Vanilla のインストールと起動 |
| `02-mod-loaders.md` | Fabric、Quilt、Forge、NeoForge |
| `03-microsoft-auth.md` | Microsoft 認証と資格情報の保管 |
| `04-content-providers.md` | Modrinth と CurseForge の API、`.mrpack` |
| `05-import-formats.md` | 他のランチャーのデータ形式 |
| `06-settings-files.md` | `options.txt`、`servers.dat`、Mod の設定ファイル |

## 3. 全体構成

### 3.1 方針

1. ビジネスロジックはすべて `nexora-core` に置く。`nexora-core` は Tauri に依存せず、UI なしで `cargo test` できる。
2. Tauri 側（`src-tauri`）は、引数を受け取り、core を呼び、結果かエラーを返すだけの薄い層とする。
3. 時間のかかる処理はタスクとして扱う（3.5 節）。
4. 認証トークンなどの秘密情報は Rust 側だけで扱い、JavaScript 側には渡さない。
5. インスタンスを変更する操作（インストール、Mod の適用、同期、復元、削除）は、インスタンス単位の排他（3.6 節）を取ってから行う。

### 3.2 リポジトリ構成

```
Nexora-Launcher/
├─ Cargo.toml              # Rust ワークスペース
├─ crates/
│  └─ nexora-core/         # ビジネスロジック（Tauri 非依存）
│     ├─ src/
│     │  ├─ paths.rs       # OS ごとのデータの置き場所
│     │  ├─ net/           # HTTP、並列ダウンロード、再試行、ハッシュ検証、流量制御
│     │  ├─ task/          # タスク管理、進捗、キャンセル
│     │  ├─ db/            # SQLite、マイグレーション
│     │  ├─ meta/          # Mojang、Fabric、Quilt、Forge、NeoForge のメタデータ
│     │  ├─ install/       # ゲームとローダーの導入
│     │  ├─ java/          # Java の検出と導入
│     │  ├─ launch/        # 起動引数の組み立て、プロセスの監視、ログ
│     │  ├─ auth/          # Microsoft 認証、トークンの保管
│     │  ├─ instance/      # インスタンスの作成、複製、削除
│     │  ├─ content/       # Mod などの管理（provider: modrinth、curseforge）
│     │  ├─ sync/          # インスタンス間の設定同期
│     │  ├─ import/        # 他のランチャーからの取り込み
│     │  ├─ backup/        # バックアップと復元
│     │  └─ diagnose/      # クラッシュ診断
│     └─ tests/fixtures/   # 実物のサンプルファイル
├─ src-tauri/              # Tauri アプリ（commands/、events.rs、state.rs）
├─ ui/                     # React アプリ
│  └─ src/ (routes/, features/, components/, api/, i18n/{ja,en}/, bindings/)
├─ docs/research/          # 調査資料（git 管理）
├─ docs/specs/             # spec（git 管理）
└─ docs/plans/             # 実装計画（git 管理外）
```

Rust のクレートは `nexora-core` の 1 つとし、モジュールで区切る。最初からクレートを細かく分けると依存の管理が複雑になるためである。モジュールが大きく育った段階で、必要に応じてクレートに切り出す。

### 3.3 Rust と TypeScript の型の共有

- Rust の型から TypeScript の型を `ts-rs` で生成し、`ui/src/bindings/` に出力する。
- command を呼ぶ関数は `ui/src/api/` に薄く手書きする。
- `tauri-specta` は v2 が RC 版（2026-10 時点で 2.0.0-rc.25）のため採用しない。安定版になった時点で再検討する。

### 3.4 ユーザーデータの置き場所

OS の基本のデータフォルダに、`Nexora` を明示的に付け足した場所を使う（Windows：`%APPDATA%\Nexora`、macOS：`~/Library/Application Support/Nexora`、Linux：`$XDG_DATA_HOME/nexora`、未設定なら `~/.local/share/nexora`）。`directories` の `ProjectDirs` は OS によって余分な階層が付くため使わない（調査 7 の 10 節）。開発ビルドでは `Nexora-dev` を使う。

```
Nexora/
├─ nexora.db          # 正となるデータ
├─ instances/<dir>/   # 各インスタンスのゲームディレクトリ
├─ shared/
│  ├─ versions/<id>/<id>.jar, <id>.json
│  ├─ libraries/<Maven のパス>
│  ├─ assets/indexes/, assets/objects/<先頭2文字>/<hash>
│  └─ runtimes/<platform>/<component>/<manifest の sha1>/   # Mojang 配布の Java（版ごとに共存させる）
├─ cache/             # 削除してよいもの：検索結果、バージョン一覧、ダウンロード途中のファイル
├─ staging/           # 操作の途中のファイル（3.7 節）
├─ backups/
└─ logs/
```

- インストール済みのインスタンスの起動に必要なメタデータ（version JSON、ローダーのプロファイル、asset index）は、`shared/` に固定して保存し、これを正とする。オフラインでの起動は `shared/` だけで行えるようにする。
- `cache/` には、削除しても再取得できるものだけを置く。

### 3.5 タスクの契約

- 長い処理を始める command は `task_id` をすぐに返す。
- 進捗、完了、失敗、キャンセル完了は、イベントで通知する。イベントは更新の通知として扱い、状態の正とはしない。
- UI は `get_task(task_id)` と `list_tasks()` で `TaskSnapshot` を取得できる。`TaskSnapshot` は、状態（queued、running、cancelling、succeeded、failed、cancelled）、進捗、結果または構造化エラー（11.1 節）を持つ。UI を再読み込みした場合や、イベントを取りこぼした場合は、これで状態を復元する。
- キャンセルの要求（cancelling）と、実際の終了（cancelled）は区別する。
- タスクの記録はメモリ上だけに持ち、終了したものは一定数を残して破棄する。

### 3.6 インスタンスの排他とゲームの実行

- インスタンスを変更する操作は、インスタンス単位のロックを取ってから行う。同じインスタンスへの変更操作は直列に処理する。
- ゲームの実行中は、インストール、Mod の適用と更新、同期の適用、上書きでの復元、削除を拒否する。
- ランチャーを閉じても、ゲームは終了させない。起動したゲームのプロセス ID と起動時刻を DB（`running_sessions`）に記録し、ランチャーを再起動したときに、そのプロセスがまだ動いているかを確認する。動いていれば実行中として扱う。
- ランチャーが閉じている間にゲームが終了した場合、終了後の処理（同期の読み取り、プレイ時間の加算）は、次にランチャーを起動したときに行う。クラッシュ診断は、その時点で残っているログを使って行う。

### 3.7 操作の記録と復旧

複数のファイルと DB をまたぐ操作（Mod の適用、同期の適用、復元、インポート）は、次の手順で行う。

1. `operations` テーブルに、操作 ID、種類、対象のインスタンス、状態（prepared）を記録する。
2. 新しいファイルを `staging/<操作 ID>/` に用意し、置き換えられるファイルを `staging/<操作 ID>/backup/` に退避する。変更の対象と退避先の一覧を、操作の記録に含める。
3. ファイルを所定の場所に移動し、DB を 1 つのトランザクションで更新する。同じトランザクションで、操作の状態を committed にする。
4. `staging/<操作 ID>/` を削除し、操作の記録を消す。

ランチャーを起動したときに、prepared のまま残っている操作があれば、退避したファイルを戻して元の状態に復旧する。committed の操作は、`staging/` の削除だけを行う。復旧が終わるまで、ファイルのスキャンによる DB の更新（4.1 節）は行わない。

## 4. データモデル

### 4.1 基本ルール

- 正となるデータは SQLite（`nexora.db`）に一本化する。インスタンスのフォルダには Nexora の管理情報を置かない。
- ディスク上のファイルは「実際に何があるか」の正とする。ファイルと DB が食い違った場合は、スキャンしてハッシュで照合し、DB をファイルに合わせる。
- ID は UUID v7 とする。時刻は UNIX ミリ秒の整数で保存する。
- スキーマの変更は、番号付きの SQL を `rusqlite_migration` で起動時に適用する。

### 4.2 テーブル

**instances**

| 列 | 内容 |
|---|---|
| id, name, icon | 識別と表示 |
| dir_name | `instances/` の下のフォルダ名。作成時の名前から決め、名前を変更してもフォルダ名は変えない。 |
| mc_version, loader, loader_version | loader は vanilla、fabric、quilt、forge、neoforge のいずれか |
| java_mode, java_path | java_mode は auto または custom |
| memory_mode, memory_mb, jvm_args | memory_mode が auto の場合は PC のスペックから決める |
| state | installing、ready、broken のいずれか |
| source | created、imported:official、imported:prism など |
| sync_profile_id | 同期プロファイルとの紐付け（任意） |
| created_at, last_played_at, play_time_ms | 記録 |

**instance_contents**

| 列 | 内容 |
|---|---|
| instance_id, kind, rel_path | kind は mod、resourcepack、shader、datapack のいずれか。rel_path はゲームディレクトリからの相対パス（例：`mods/sodium.jar`、`saves/World1/datapacks/x.zip`）で、インスタンス内で一意とする |
| enabled | ディスク上ではファイル名の末尾に `.disabled` を付けるかどうかで表す |
| sha1, sha512, size | 照合と検証に使う |
| provider, project_id, version_id | provider は modrinth、curseforge、local のいずれか |
| pinned | バージョンを固定しているか |
| installed_as | explicit（ユーザーが導入）または dependency（依存として自動導入） |

**accounts**：id（Minecraft の UUID）、username、xuid（Xbox のユーザー ID）、skin_url、is_active、added_at、profile_fetched_at、entitlement_state（verified、not_owned、unknown）、entitlement_checked_at、verified_at（Java 版の利用権を最後に確認できた時刻）、needs_login（再ログインが必要かどうか）。トークンは保存しない。プロフィールを取得できたことと、Java 版の利用権を確認できたことは区別して記録する。

**java_runtimes**：path、major_version、component、os、arch、vendor、source（mojang、system、user）、last_verified_at。

**running_sessions**：instance_id、pid、process_started_at、launched_at、account_id。3.6 節で使う。

**operations**：id、kind、instance_id、state（prepared、committed）、targets（変更の対象と退避先の一覧）、created_at。3.7 節で使う。

**backups**：instance_id、created_at、reason（manual、pre-update、pre-sync、pre-restore）、path、size。

**app_settings**：キーと値。テーマ、言語、同時ダウンロード数、既定のメモリ方針などを保存する。

**同期のテーブル**：7 節で定める。

### 4.3 DB に保存しないもの

- タスクの進捗：メモリ上だけに持つ。
- メタデータのキャッシュ：`cache/meta/` に JSON ファイルとして置く。
- ゲーム本体、ライブラリ、assets：`shared/` に置き、ハッシュで管理する。
- 認証トークン：OS の資格情報ストアに保存する（6 節）。

## 5. インストールと起動

### 5.1 処理の段階

「プレイ」を押してから起動するまでを 5 つの段階に分け、それぞれを独立してテストできる関数とする。

1. **解決（resolve）**：インスタンスの設定から完全なバージョンプロファイルを組み立てる。Vanilla の version JSON にローダーのプロファイルを重ねる（`inheritsFrom`）。メタデータは `cache/meta/` を参照し、ETag で更新を確認する。オフラインの場合はキャッシュだけで進める。
2. **計画（plan）**：必要なもの（クライアントの jar、ライブラリ、natives、assets、ログ設定、Java）の一覧を作る。
   - 各項目は、ファイル、ディレクトリ、リンクのいずれかとする。ファイルは URL、保存先、サイズ、期待するハッシュ（ない場合もある）、実行権限の要否を持つ。
   - 環境（OS、アーキテクチャ）と既存ファイルの情報は引数として受け取り、計画の作成自体は副作用を持たない純粋な処理とする。
3. **取得（fetch）**：足りないファイルだけをダウンロードする。
   - 同時ダウンロード数の既定値は 8 とし、設定で変更できる。
   - 失敗した場合は指数バックオフで 3 回まで再試行する。
   - `.part` ファイルに書き込み、sha1 を検証してから、原子的にリネームする。
   - 確認の深さは場面ごとに変える。起動時は存在とサイズだけを確認する。インストール時と「修復」の実行時は、ハッシュまで確認する。
4. **ローダーの導入（loader）**：5.3 節のとおり。
5. **起動（launch）**：引数を組み立て、natives をインスタンス専用の一時フォルダへ展開し、`tokio::process` でゲームを起動する。
   - 標準出力の log4j XML を通常の行に変換し、イベントで UI に流す。
   - 終了コードとクラッシュレポートを確認し、診断（10 節）に渡す。
   - 実行中のインスタンスを記録し、二重起動の防止と、設定同期の書き込み禁止に使う。

### 5.2 メタデータの取得元

各公式から直接取得する。Modrinth や Prism Launcher のメタデータ配信には依存しない。

| 対象 | 取得元 |
|---|---|
| Vanilla | piston-meta.mojang.com（version_manifest_v2.json） |
| Fabric | meta.fabricmc.net（v2） |
| Quilt | meta.quiltmc.org（v3） |
| Forge | maven.minecraftforge.net（maven-metadata.xml）、files.minecraftforge.net（promotions） |
| NeoForge | maven.neoforged.net（`net.neoforged:forge` と `net.neoforged:neoforge` の 2 系統） |
| Java | Mojang の Java runtime manifest |

### 5.3 ローダーの導入

| ローダー | 方式 | 初期の対応範囲 |
|---|---|---|
| Fabric、Quilt | 公式のプロファイル JSON を Vanilla に重ねる | 公式のメタデータが示すバージョン（Vanilla の対応範囲とは独立に、各ローダーが提供しているもの） |
| Forge | インストーラを取得して展開し、install_profile の processor を自前の実行エンジンで順に実行する。出力のハッシュが宣言されている場合だけ検証する。完了時に、インストーラ、入力、使用した Java の識別情報を完了記録として残す | 1.16.5 以降のうち、フェーズ 5 で検証したビルド |
| NeoForge | Forge と同じ実行エンジン | フェーズ 5 で検証したビルド（1.20.1 以降） |

- 1.8.9〜1.12.2 の Forge は旧形式（universal jar と LaunchWrapper）であり、別のアダプターが必要になる。フェーズ 5 の後半（5-2b）で扱う。
- 1.13.2〜1.16.4 の Forge は、実行エンジンが同じ形式に対応できるため、検証が済んだ時点で範囲に加える。
- processor の実行に使う Java は、インストーラの要件に合うものを選ぶ。ゲームの起動に使う Java と同じとは限らない（調査 2 の 6 節）。
- 検証していない新しい形式（例：26.x の番号の NeoForge）は、形式を確認してから対応範囲に加える。
- Forge と NeoForge の公式インストーラは `--installClient` に対応している（調査 2）。この方式は、自前の実行エンジンで問題が起きたときの代替案として記録しておく。初期実装では使わない。

### 5.4 Java

- 既定では、version JSON の `javaVersion`（component と majorVersion）、OS、アーキテクチャに合う Mojang 配布の Java を自動で導入して使う。
- 新しい版の Java を導入しても、使用中の版は削除しない。
- システムにインストールされた Java を検出し、詳細設定で選べるようにする。

### 5.5 メモリの自動設定

`sysinfo` で物理メモリを取得し、ローダーと Mod の数から割り当てを決める。初期値の目安は次のとおりとする。具体的な式はフェーズ 9 の spec で定める。

| 構成 | 割り当て |
|---|---|
| Vanilla | 2 GB |
| Mod が少ない | 4 GB |
| Mod が 100 個を超える | 6〜8 GB |

上限は物理メモリの 50%、下限は 2 GB とする。

### 5.6 Windows での注意点

- 起動時にコンソールのウィンドウを表示しない（`CREATE_NO_WINDOW`）。
- コマンドラインの長さには 32,767 文字の上限がある。Java 9 以降では `@argfile` を使う。Java 8 では使えないため、共有ストアのパスを短く保って上限に収める。
- パスに日本語などの非 ASCII 文字が含まれる場合の挙動は、フェーズ 2 で実機を使って確認する。

## 6. Microsoft 認証

### 6.1 ログインの方式

- 主な方式は、システムブラウザを使う認可コードフローと PKCE（S256）とする。Azure には `http://localhost` を登録し、要求では `http://localhost:<空いているポート>` を使う（Microsoft は localhost のリダイレクト URI に限ってポートの違いを無視する）。待ち受けはループバックだけに限定し、1 回だけ受け付ける。`127.0.0.1` を使う方式は、Nexora 自身の登録で動作を確認できた場合にだけ採用する。
- アプリ内の WebView ではログインさせない。
- 代替の方式として、デバイスコードフローを用意する。

### 6.2 トークンの連鎖

```
Microsoft（refresh token）→ Xbox Live ユーザートークン → XSTS（rp://api.minecraftservices.com/）
→ Minecraft アクセストークン → 所有権の確認（entitlements）→ プロフィール（UUID、名前、スキン）
```

### 6.3 保管

| 種類 | 保存先 |
|---|---|
| Microsoft の refresh token | OS の資格情報ストア（`keyring-core` と OS ごとのストアクレート） |
| Xbox、XSTS、Minecraft のトークン | メモリだけ。期限が切れたら refresh token から取得し直す |
| UUID、名前、スキン | DB の accounts テーブル |

- Windows の資格情報ストアには、1 件あたり 2,560 バイトの上限がある。Windows では Local persistence を明示して保存する。refresh token が 1 件に収まらない場合や、保存に失敗した場合は、そのセッションの間だけログインした状態にし、その旨を UI に表示する。トークンの切り詰めや、平文ファイルへの退避は行わない。複数の件に分けて保存する方式は、実際に必要と分かった時点で別途設計する。
- 新しい refresh token の保存に成功するまで、古い refresh token は削除しない。
- 資格情報ストアを使えない環境（例：Secret Service のない Linux）では、そのセッションの間だけログインした状態で動かす。

### 6.4 更新とエラー

- 起動の前に、期限が切れたトークンを自動で取得し直す。同じアカウントの更新は、ロックで 1 件ずつ順に行う。
- ネットワークの不通と、トークンの失効を区別する。失効（`invalid_grant`）の場合だけ、再ログインを求める。
- 次のエラーは、それぞれ別のエラーコードにする。
  - XErr（Xbox プロフィールなし、年齢制限、地域制限など）
  - HTTP 403「Invalid app registration」（クライアント ID が未承認）
  - プロフィールが存在しない（HTTP 404）
  - 所有権の確認に失敗した場合（一時的な失敗と、所有していない場合を区別する）
- OAuth は `oauth2` クレートを使わず、PKCE とトークン交換を自前で実装する。`oauth2` 5.0 は reqwest 0.12 に依存し、本体の reqwest 0.13 とバージョンが合わないため。

### 6.5 起動への受け渡し

- 認証の結果は、Rust 内部の起動用セッション型（UUID、名前、Minecraft のアクセストークン、XUID、ユーザーの種別）として起動処理に渡す。
- 開発用のオフラインアカウントと本来の認証は、同じ型と同じ入口を使う。フェーズ 2 で型を定め、フェーズ 3 で本来の認証を接続する。

### 6.6 オフラインの方針

- 製品版では、ユーザー名だけのオフラインアカウントは作成させない。
- 一度正常に認証できたアカウントに限り、ネットワークに接続できないときに「オフラインで起動」と明示したうえで起動できる。
- 開発版では、ビルドのフラグによってオフラインアカウントを使えるようにする。Minecraft API の審査を待つ間の開発に使う。

### 6.7 クライアント ID

- ビルド時の環境変数 `NEXORA_MS_CLIENT_ID` で埋め込む。
- Minecraft API の利用には、クライアント ID ごとの審査（aka.ms/mce-reviewappid）が必要である。README に、フォークする場合は自分でクライアント ID を取得して審査を受ける必要があることを記載する。

## 7. コンテンツ管理

### 7.1 Provider

- provider は enum（`Modrinth`、`Curseforge`）で表し、`match` で処理を振り分ける。provider は 2 つしか予定がないため、trait オブジェクトは使わない。
- 共通の操作：検索、プロジェクトの取得、バージョン一覧の取得、ハッシュによる特定、更新の確認、ダウンロード情報の取得。
- ダウンロード情報は、`Url`、`ManualRequired`（公式ページで手動ダウンロード）、`Unavailable`、`AuthRequired` のいずれかで返す。
- Modrinth へのリクエストでは、User-Agent を `Nexora-Launcher/<version> (<GitHub の URL>)` とする。毎分 300 回の上限は、アプリ全体で共有するリミッター（`governor`）で守る。

### 7.2 依存関係の解決

1. 前提を固定する：Minecraft のバージョン、ローダー、チャンネル（既定は release のみ）。
2. ピン留めを守る：固定されたバージョンは変更しない。
3. 依存の版を選ぶ：インストール済みで互換性のある版を優先する。ない場合は、互換性のある版のうち公開日が最新のものを選ぶ。バージョン文字列を semver として比較しない。
4. グラフを作る：依存の辺ごとに、どの Mod がなぜ必要としたかを記録する。循環は訪問済みの印を使い、1 回だけ処理する。
5. `incompatible` を確認する：インストール済みの Mod とも、双方向で照合する。衝突した場合は Mod を勝手に削除せず、理由を示して処理を止める。
6. 結果を計画として示す：追加、更新、削除の一覧を示し、ユーザーが承認してから適用する。

### 7.3 適用のトランザクション

3.7 節の操作の記録に従う。

```
staging/ にダウンロード → ハッシュを検証 → 置き換えられるファイルを退避
→ mods/ などに移動 → DB を 1 つのトランザクションで更新（操作を committed にする）
```

### 7.4 その他の操作

- 更新の確認：`POST /version_files/update` でまとめて問い合わせる。一括更新の前に、`mods/` と `config/` を自動でバックアップする。
- 有効化と無効化：ファイル名の末尾の `.disabled` を付け外しする。
- `.mrpack` のインポート：固定されたバージョンのファイルを導入し、依存の解決は行わない。`overrides` の展開時には、パストラバーサルを検査する。
- `.mrpack` のエクスポート：Modrinth で特定できたファイルは参照として書き出す。特定できなかったファイルは `overrides` に含め、事前に配布ライセンスの確認を促す警告を表示する。
- データパック：ワールドごとの `datapacks/` に導入する。

### 7.5 CurseForge

- CurseForge の API キーは、利用規約により公開できない。2026 年 7 月 16 日以降は、CDN からのファイルのダウンロードにもキーが必要である（調査 4）。
- 書面で許可を得たうえで、CI のシークレットから公式ビルドにだけキーを埋め込む。
- 配布が許可されていないファイル（`allowModDistribution` が false または null）は、公式ページへの案内による手動ダウンロードとし、選択されたファイルのサイズとハッシュを検証する。

## 8. インスタンス間の設定同期

### 8.1 基本の考え方

- ファイル全体のコピーは行わず、項目ごとに管理する。同期できる項目の一覧（フィールド登録表）をデータとして core に持つ。各項目には、`options.txt` のキー、型、カテゴリ、有効な DataVersion の範囲、コーデックの系統を定める。
- 一覧にない項目は同期しない。
- 初期実装は、共通プロファイルを正とする一方向の同期とする。
- 実行中のインスタンスには書き込まない。
- Nexora の管理情報を `options.txt` に書き込まない。ゲームは、認識しないキーを保存のときに削除するためである。

### 8.2 バージョン差への対応

- 各インスタンスの DataVersion は、クライアント jar の中の `version.json` から読み取る。対応表をハードコードしない。
- キー割り当ては、次の 3 つの系統の中だけで同期する。系統をまたぐ場合は同期せず、その旨をレポートに記載する。系統間の変換は v2 以降で扱う。
  - LWJGL2 の数値（1.8.9〜1.12.2）
  - `key.keyboard.*` 形式（1.13〜26.2）
  - SDL3 の物理スキャンコード（26.3 以降）
- 途中で構造が変わった項目（`fancyGraphics` から `graphicsMode`、1.21.11 のグラフィック設定の再編など）は、フィールド登録表の DataVersion の範囲で区別する。

### 8.3 起動前の適用

1. インスタンスが実行中であれば適用しない。
2. `options.txt` のうち、管理対象の行だけを書き換える。それ以外の行、並び順、認識しないキーはそのまま残す。
3. バックアップを取り、一時ファイルに書き込んでから、原子的にリネームする。
4. 対象のゲームの版（クライアント jar から取得した DataVersion）と、既存の `options.txt` の `version` の行が一致しない場合は、同期を適用しない。ゲームが一度起動してファイルを書き直した後に適用する。DataVersion を取得できない場合も、適用しない。
5. `options.txt` が存在しない新しいインスタンスに対して、`version:<DataVersion>` の行を含むファイルを作成して適用する方式は、フェーズ 6 でサンプルファイルを使って動作を確認できた場合にだけ採用する。確認できない場合は、初回の起動ではゲームにファイルを作成させ、2 回目の起動から適用する。

### 8.4 終了後の読み取り

- 適用した直後の状態を基準として差分を取り、ゲーム内で変更された項目を「提案」として記録する。
- UI で、変更を共通プロファイルに反映するかを確認する。自動では反映しない。
- 「このインスタンスだけ別の値にする」も選べる。これはインスタンスごとの例外として記録する。

### 8.5 カテゴリと対象外の項目

カテゴリは、キー割り当て、マウス、音量、グラフィック、チャット、アクセシビリティ、その他とする。

次の項目は、常に同期の対象外とする。

- 解像度、フルスクリーン、モニターに関する項目（`overrideWidth` など）
- `lastServer`、チュートリアルの進行状況、警告の表示済みフラグ
- テレメトリの同意
- `soundDevice`
- リソースパックの一覧（v1 では対象外）

### 8.6 Mod の設定とサーバー一覧

- Mod の設定の同期は v1 では行わない。v2 以降で、Mod ごとの専用アダプター（Mod ID、Mod のバージョン、Minecraft のバージョン、ローダー、設定ファイルの形式を確認するもの）として扱う。
- サーバー一覧（任意）は、`servers.dat`（NBT）のうち、名前、アドレス、並び順だけを同期する。

### 8.7 データモデル

| テーブル | 主な列 |
|---|---|
| sync_profiles | id、name、revision、有効にしたカテゴリ |
| sync_values | profile_id、field_id、コーデックの系統、値、updated_at |
| sync_profile_instances | instance_id、profile_id、除外する項目、インスタンスごとの例外の値 |
| sync_snapshots | instance_id、適用した revision、適用後の基準値、終了後の提案、状態 |

### 8.8 競合

- 2 つのインスタンスが同じ項目に異なる提案をした場合は、両方を表示してユーザーに選んでもらう。更新日時による自動の判定は行わない。
- ゲームの実行中にプロファイルが変更された場合、そのセッションには適用しない。次回の起動時に適用する。

## 9. インポート

### 9.1 処理の流れ

1. スキャン：インポート元ごとに、インスタンスの候補（名前、バージョン、ローダー、サイズ、ワールド数）を一覧にする。元のフォルダは読み取るだけで、書き込まない。
2. ユーザーが取り込む対象を選ぶ。
3. インスタンスを `installing` の状態で作成する。
4. ゲームディレクトリをコピーする。`logs/`、`natives/`、キャッシュは除外する。
5. Mod の出どころを復元する。インポート元のメタデータ（Prism Launcher の `.pw.toml`、CurseForge App の `installedAddons`、Modrinth App の DB）を使い、分からないものは Modrinth にハッシュで一括して問い合わせる。それでも特定できないものは `local` とする。
6. ゲームとローダーを通常のインストール処理で導入し、状態を `ready` にする。

v1 で取り込めるのは、Minecraft のバージョンと 5 種類のローダーで表現できる構成だけとする。独自の version JSON、Prism Launcher の patches や jar mods、未知のコンポーネントを含むインスタンスは「未対応の構成」として一覧に表示し、取り込まない。Vanilla や通常のローダーに置き換えて取り込むことはしない。

### 9.2 資格情報の除外

資格情報を含むファイルは除外リストで明示し、読み取りもコピーもしない（`launcher_accounts.json`、Prism Launcher の `accounts.json`、Modrinth App の DB の認証テーブル、GDLauncher の `gdl_conf.db` の認証情報など）。

### 9.3 インポート元ごとの注意点

| インポート元 | 注意点 |
|---|---|
| 公式ランチャー | プロファイル単位で取り込む。複数のプロファイルが同じ `.minecraft` を共有している場合は警告を表示する。Microsoft Store 版の置き場所にも対応する。 |
| Prism Launcher / MultiMC | `prismlauncher.cfg` の `InstanceDir` で場所が変更されている場合にも対応する。`mmc-pack.json` の components からローダーを判定する。 |
| Modrinth App | DB には旧形式（`profiles`）と新形式（`instances`）がある。両方に対応し、未知の形式の場合は「未対応のバージョン」として処理を止める。DB には認証情報も含まれるため、コピーしない。元の DB を読み取り専用で開き、許可した列だけを読み出す。 |
| CurseForge App | `minecraftinstance.json` を読み取る。ローカルのファイルをコピーするため、API キーは不要である。 |
| GDLauncher | 旧版（`config.json`）と Carbon（DB と `instance.json`）の両方に対応する。 |

### 9.4 フォルダからの取り込み

任意の `.minecraft` 相当のフォルダを指定し、バージョンとローダーを手動で選んで取り込む機能を用意する。

## 10. バックアップ、復旧、診断、自己更新

### 10.1 バックアップ

| 種類 | 対象 | 保持数 |
|---|---|---|
| 手動 | インスタンス全体、またはワールド単位（zip） | 無制限 |
| 更新前（自動） | `mods/` と `config/` | 最新 10 件 |
| 同期前（自動） | 書き換えるファイルのみ | 最新 10 件 |

- ゲームの実行中は、ワールドのバックアップを取らない。
- 復元は「新しいインスタンスとして復元」か「上書き」を選べる。上書きの前には、自動でバックアップを取る。

### 10.2 起動時の自己修復

- `installing` の状態のまま残っているインスタンスを `broken` にし、再試行の操作を表示する。
- 残っている `.part` ファイルを削除する。
- 中断した操作を、3.7 節の手順で復旧する。
- `running_sessions` に記録されたゲームが終了していれば、終了後の処理を行う（3.6 節）。

### 10.3 クラッシュ診断

- 判定ルールはデータとして持つ（ID、パターン、取り出す値、メッセージのキー、提案する対処）。
- 入力：終了コード、`latest.log`、`crash-reports/*.txt`、`hs_err_pid*.log`、ランチャー側のエラー。
- 出力：原因の候補（確信度付き）と、ワンクリックで実行できる対処。

初期のルールは次のとおりとする。

| 原因 | 検出の手がかり | 対処 |
|---|---|---|
| Java のバージョンの不一致 | `UnsupportedClassVersionError`、`class file version` | Java を切り替える |
| メモリ不足 | `OutOfMemoryError` | 割り当てを増やす |
| 依存 Mod の不足 | Fabric の `requires`、Forge の必須依存の不足のメッセージ | 不足している Mod を導入する |
| Mod の重複や衝突 | Mixin の失敗に含まれる Mod 名 | 該当する Mod を無効にする |
| GPU ドライバのクラッシュ | `EXCEPTION_ACCESS_VIOLATION` と `nvoglv`、`atio6axx` | ドライバの更新を案内する |
| ファイルの破損 | ハッシュの不一致、ZIP のエラー | インストールを修復する |

### 10.4 ランチャー本体の自己更新

- `tauri-plugin-updater` を使い、GitHub Releases から署名付きの更新を受け取る。
- 起動時に更新を確認し、ユーザーが同意した場合に適用する。
- 署名用の秘密鍵は CI のシークレットに保存する。

## 11. 横断的な設計

### 11.1 エラー処理

- core では、モジュールごとに `thiserror` の enum を定義する。各エラーは、変わらないエラーコード（例：`auth.xsts.no_xbox_profile`）と引数を持つ。
- Tauri の境界では、`{ code, params, detail }` の形の JSON で返す。
- UI では、`errors.<code>` を i18n で日本語と英語に変換する。`detail` は「詳細を表示」の操作をしたときだけ表示する。
- Rust 側は、文言を組み立てない。

### 11.2 ログ

- `tracing` で `logs/` に書き込み、日付ごとにファイルを分ける（`tracing-appender`）。
- トークンなどの秘密情報は伏せ字にする。秘密情報を持つ型には `secrecy` を使う。

### 11.3 テスト

| 層 | 対象 | 道具 |
|---|---|---|
| 単体 | ルールの評価、引数の組み立て、計画の作成、依存の解決、`options.txt` の解析 | 実物のサンプルファイル、`insta`（スナップショット） |
| 結合 | HTTP を伴う処理、ファイル操作 | `wiremock`、`tempfile` |
| 実機 | 代表的な構成での起動 | 手動のチェックリスト |
| UI | 状態管理のロジック | Vitest |

実機での確認の対象とする構成は、1.8.9 の Vanilla、1.20.1 の Fabric、1.16.5 以降の Forge、最新版の NeoForge とする。

### 11.4 CI

GitHub Actions で、Windows、Linux、macOS のビルドとテストを行う。

### 11.5 主なクレート

バージョンは 2026-10-15 時点の crates.io の安定版である。

| 用途 | クレート |
|---|---|
| アプリ | tauri 2.12、tauri-plugin-updater 2.14、tauri-plugin-opener 2.7、tauri-plugin-single-instance 2.5 |
| 非同期と HTTP | tokio 1.53、tokio-util 0.7、reqwest 0.13、futures 0.3 |
| DB | rusqlite 0.40（bundled）、rusqlite_migration 2.6 |
| シリアライズ | serde 1、serde_json 1、quick-xml 0.42、fastnbt 2.6 |
| 圧縮とハッシュ | zip 9、sha1 0.11、sha2 0.11 |
| 認証 | oauth2 5.0（採否はフェーズ 3 で決める）、keyring-core 1.0 と OS ごとのストア、secrecy 0.10 |
| OS の情報 | dirs 7（基本のデータフォルダの取得）、sysinfo 0.39 |
| 流量制御 | governor 0.10 |
| エラーとログ | thiserror 2、tracing 0.1、tracing-subscriber 0.3、tracing-appender |
| 型の生成 | ts-rs 12 |
| テスト | wiremock 0.6、tempfile 3、insta 1 |

### 11.6 ライセンス表示

配布するアプリには、依存するクレートと npm パッケージのライセンス表示を同梱する。`cargo-about` でライセンスの一覧を生成し、アプリの「About」画面のライセンスの欄に表示する。

## 12. UI の構成

見た目の詳細は、フェーズ 1 の spec で 2 つのデザインスキルを使って定める。ここでは画面の構成だけを定める。

| 画面 | 内容 |
|---|---|
| ホーム | 最後に遊んだインスタンスと、大きなプレイボタン |
| インスタンス一覧 | インスタンスの一覧と作成 |
| インスタンス詳細 | タブ：コンテンツ、設定、ワールド、ログ、バックアップ |
| 探す | Modrinth の検索。検索から導入までを画面内で完結させる |
| アカウント | アカウントの追加、切り替え、削除。スキンの表示と変更 |
| 設定 | アプリ全体の設定 |

- Ctrl+K でコマンドパレットを開き、インスタンスの起動、画面の移動、検索を行える。
- ダウンロードやインストールの進捗は、タスクトレイにまとめて表示する。
- 長い処理の間も UI を止めない。アニメーションは軽く、控えめにする。

## 13. 開発の段階

各フェーズで spec、実装計画、実装の順に進める。

| フェーズ | 内容 |
|---|---|
| 0 | 全体設計（この文書） |
| 1 | 土台：Tauri、React、Tailwind CSS、React Aria Components、デザイントークン、i18n、core の骨組み、DB、タスク管理、ログ、CI |
| 2 | Vanilla のインストールと起動、Java の導入（開発用のオフラインアカウントを使う） |
| 3 | Microsoft 認証とアカウント管理 |
| 4 | インスタンス管理の UI（作成、編集、複製、削除、ホーム）、スキンの変更 |
| 5 | Fabric、Quilt、Forge、NeoForge（旧形式の Forge を含む）、Modrinth 連携（検索、依存の解決、更新、更新前のバックアップ） |
| 6 | インスタンス間の設定同期 |
| 7 | `.mrpack` のインポートとエクスポート |
| 8 | 他のランチャーからのインポート（公式、Prism Launcher、Modrinth App、CurseForge App、GDLauncher の順） |
| 9 | スマート機能：クラッシュ診断、メモリの自動設定、環境レシピ |
| 10 | バックアップの UI、自己更新、ライセンス表示、CurseForge のキー、OSS としての公開 |

- Microsoft 認証をフェーズ 3 にしているのは、Minecraft API の審査を待つ間も、フェーズ 2 をオフラインで進められるためである。
- スキンの変更は、Minecraft のアクセストークンを使うため、フェーズ 3 の認証の後に置く。フェーズ 3 では現在のスキンの表示までを行い、変更はフェーズ 4 で UI の作業とあわせて行う。
- 環境レシピは、`.mrpack` に Nexora 独自の設定を加えた形式とする想定であるため、フェーズ 7 より後に置く。形式はフェーズ 9 の spec で定める。

## 14. 未決定の事項

| 事項 | 決める時期 |
|---|---|
| ライセンス | 決定済み：Apache-2.0（2026-10-16） |
| oauth2 クレートを使うか、自前で実装するか | 決定済み：自前で実装（フェーズ 3、2026-10-16） |
| 1.8.9〜1.12.2 の Forge に対応するか | 決定済み：フェーズ 5 の 5-2b で対応（2026-10-20） |
| Minecraft API の審査の結果と、オフライン方針の最終確認 | フェーズ 3 |
| CurseForge の API キーの許可 | フェーズ 10 |
| 環境レシピの形式 | フェーズ 9 |
| 調査資料で UNVERIFIED とされた各項目 | 該当するフェーズ |
