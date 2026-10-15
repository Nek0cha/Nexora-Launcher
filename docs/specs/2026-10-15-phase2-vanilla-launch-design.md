# フェーズ 2：Vanilla のインストールと起動の設計

- 作成日：2026-10-15
- 状態：レビュー待ち
- 前提：`docs/specs/2026-10-15-nexora-architecture-design.md`（spec 0）、`docs/specs/2026-10-15-phase1-foundation-design.md`（spec 1）
- 根拠：`docs/research/01-mojang-install-launch.md`（調査 1）

## 1. 目的と範囲

フェーズ 2 の終了時点で、次のことができる状態にする。

1. ホームからインスタンスを作成できる。作成のダイアログでは、名前と、1.8.9 以降の正式版のバージョンを選ぶ。
2. 作成すると、ゲーム本体、ライブラリ、assets、Mojang 配布の Java が自動でインストールされ、進捗がプレイボタンとタスクトレイに表示される。
3. プレイボタンを押すと、開発用のオフラインアカウントで Minecraft が起動する。
4. 実行中は「プレイ中」と経過時間が表示され、終了するとプレイ時間が記録される。異常終了した場合は、そのことが表示される。
5. ゲームの実行中にランチャーを閉じて再起動しても、実行中として扱われる。
6. インストール済みのインスタンスは、ネットワークに接続していなくても起動できる。

### 1.1 範囲外

| 項目 | 扱うフェーズ |
|---|---|
| インスタンスの一覧、編集、複製、削除、ホームでの切り替え | 4 |
| システムの Java の検出と手動指定（`java_mode = custom`） | 4 |
| ゲームのログを表示する画面（インスタンス詳細のログタブ） | 4 |
| Microsoft 認証、`accounts` テーブル | 3 |
| メモリの自動設定 | 9 |
| クラッシュ診断 | 9 |
| スナップショット、old_beta、old_alpha、1.8.9 より前のバージョン | 対象外（スナップショットは必要になった時点で判断する） |
| ローダー（Fabric など） | 5、7 |

## 2. 構成

### 2.1 モジュール

spec 0 の 3.2 節の区切りに従い、`nexora-core` に次のモジュールを追加する。

| モジュール | 役割 |
|---|---|
| `net/` | HTTP クライアント、並列ダウンロード、再試行、`.part` への書き込みと sha1 の検証、原子的なリネーム |
| `meta/mojang` | `version_manifest_v2.json`、version JSON、asset index の取得とキャッシュ |
| `install/` | resolve、plan（純粋関数）、fetch の呼び出し、インストールのタスク |
| `java/` | Mojang 配布の Java の導入と登録 |
| `launch/` | 引数の組み立て、natives の展開、プロセスの起動と監視、log4j XML の変換、起動用セッション型 |
| `instance/` | インスタンスの作成と取得、インスタンス単位のロック |

フェーズ 1 のデモタスク（`demo.rs`、`start_demo_task`、UI の `useDemoStart` と関連する文言）は削除する。

### 2.2 処理の流れ

1. **作成**：`create_instance(name, mc_version)` が、`instances` に state = `installing` で行を追加し、ゲームディレクトリを作り、インストールのタスクを開始して `task_id` を返す。
2. **インストール**：resolve → plan → fetch（sha1 まで確認）→ Java の導入の順に行う。成功したら state を `ready` にする。失敗したら `broken` にする。
3. **起動**：`launch_instance(instance_id)` が起動のタスクを開始する。インスタンス単位のロックを取り、resolve（`shared/` だけで行い、ネットワークを使わない）→ plan → fetch（存在とサイズだけ確認し、欠けたファイルだけ取得する）→ プロセスの起動の順に行う。プロセスを起動して `running_sessions` に記録した時点で、タスクは成功とする。
4. **実行中**：プロセスの監視は TaskManager の外で行う。ログの行と終了は、イベントで通知する。
5. **終了**：プレイ時間を加算し、`running_sessions` の行を削除する。

- state が `installing` または `broken` のインスタンスでプレイを押した場合は、インストールのタスクを開始する（続きから取得する）。インストールが成功しても、自動では起動しない。
- タスクが成功した時点で実行中の状態を手放すのは、ゲームが動いている間、タスクトレイにタスクが残り続けるのを避けるためである。実行中かどうかの正は `running_sessions` とする。

## 3. データモデル

### 3.1 マイグレーション

`0002_instances.sql` で次のテーブルを追加する。`accounts` と `operations` は、使うフェーズで追加する。インストールは `shared/` へのファイル単位の原子的な書き込みで完結するため、spec 0 の 3.7 節の操作の記録は使わない。

**instances**

spec 0 の 4.2 節の列をすべて作る。後のフェーズで列を追加するマイグレーションを減らすためである。フェーズ 2 での値は次のとおりとする。

| 列 | フェーズ 2 での値 |
|---|---|
| loader | `vanilla` 固定。loader_version は NULL |
| java_mode | `auto` 固定。java_path は NULL |
| memory_mode | `auto` 固定。実際の割り当ては `-Xmx2G` とする |
| jvm_args | NULL |
| source | `created` |
| sync_profile_id | NULL（外部キーは同期のテーブルを作るフェーズ 6 で付ける） |

**java_runtimes**：spec 0 の 4.2 節のとおり。フェーズ 2 で登録するのは source = `mojang` のものだけとする。

**running_sessions**：spec 0 の 4.2 節の列に、`log_path`（ゲームのログの書き出し先）を加える。account_id には、`LaunchSession` の UUID を入れる。

### 3.2 フォルダ名

- `dir_name` は、作成時の名前から決める。OS で使えない文字（`<>:"/\|?*` と制御文字）を `_` に置き換え、末尾の空白とピリオドを取り除く。Windows の予約名（`CON`、`NUL`、`COM1` など）には末尾に `_` を付ける。
- 日本語などの非 ASCII 文字は残す。
- 置き換えの結果が空になった場合は `instance` とする。
- 既存のフォルダと重複する場合は、末尾に ` (2)`、` (3)` を付ける。
- 名前の既定値はバージョン名（例：`1.21.5`）とする。

## 4. 起動用セッションと開発用のオフラインアカウント

spec 0 の 6.5 節の起動用セッション型を、次のとおり定める。

```rust
pub struct LaunchSession {
    pub uuid: Uuid,
    pub username: String,
    pub access_token: SecretString,
    pub xuid: Option<String>,
    pub user_type: UserType, // Msa | Offline
}
```

- オフラインアカウントの `LaunchSession` を作る関数は、`debug_assertions` が有効なビルドでだけコンパイルする（`paths.rs` の開発ビルドの判定と同じ）。リリースビルドでは、この関数が存在しない。
- 名前は `app_settings` の `dev.offline_username` から取る。既定値は `Player` とする。
- UUID は、文字列 `OfflinePlayer:<名前>` の MD5 から作る v3 の UUID とする（Vanilla のサーバーのオフラインモードと同じ決め方）。
- access_token は値 `0` とし、user_type は `Offline` とする。起動引数の `user_type` には `legacy` を渡す。
- アカウント画面には、開発ビルドのときだけ、名前の入力欄を表示する。
- フェーズ 3 では、Microsoft 認証の結果から同じ `LaunchSession` を作り、同じ入口で起動処理に渡す。

## 5. インストール

### 5.1 メタデータ

- `version_manifest_v2.json` は `cache/meta/` に ETag 付きで保存する。取得に失敗した場合は、キャッシュがあればキャッシュを使う。
- `list_versions()` は、type = `release` で、1.8.9 以降のものを、新しい順に返す。1.8.9 以降の判定は、manifest の `releaseTime` が 1.8.9 の `releaseTime` 以降であることで行う。
- version JSON は、manifest に記載された sha1 で検証してから、`shared/versions/<id>/<id>.json` に保存する。asset index は `shared/assets/indexes/` に保存する。起動時の resolve は、これらだけを読む。

### 5.2 resolve

- 対象は 1.8.9 以降の正式版だけであり、公式の version JSON には `inheritsFrom` がない（調査 1 の 2 節）。`inheritsFrom` がある場合は `install.unsupported_profile` を返す。

### 5.3 plan

- plan は副作用を持たない純粋関数とする。OS、アーキテクチャ、既存ファイルの情報（パスとサイズ）、確認の深さを引数として受け取る。
- 出力は、ファイルの一覧（URL、保存先、サイズ、sha1、実行権限の要否）とする。対象は、クライアントの jar、ライブラリ、natives、asset index、asset の objects、ログ設定、Java のファイルである。
- ルールは、調査 1 の Rules の節に従って評価する。空でないリストは「許可しない」から始め、一致したルールの action で順に置き換える（最後に一致したものが勝つ）。`rules` がない場合と、空のリストの場合は、無条件で含める。空のリストの扱いは公式の挙動が未確認のため、Nexora の方針としてテストで固定する。
- `os.arch` は、使用する JVM のアーキテクチャと比べる。`os.version` は正規表現として、OS のバージョン文字列の先頭からの一致で評価する。
- asset index に `virtual` または `map_to_resources` が true で含まれる場合は、`install.unsupported_profile` を返す（1.7.2 以前の形式であり、対象外のため）。

### 5.4 fetch

- spec 0 の 5.1 節のとおりとする（同時ダウンロード数は設定値、既定は 8。指数バックオフで 3 回まで再試行。`.part` に書き込み、sha1 を検証してからリネームする）。
- 確認の深さ：インストールのタスクでは、既存ファイルの sha1 まで確認する。起動のタスクでは、存在とサイズだけを確認する。
- 進捗は、完了したバイト数と全体のバイト数を `set_progress` に渡す。`TaskProgress` の current と total は u32 のため、KiB 単位に丸めて渡す。

### 5.5 Java の導入

- version JSON の `javaVersion.component` を使う。`javaVersion` がない場合は `jre-legacy` とする。1.8.9 がこれに当たるかは、実物の version JSON で確認する。
- プラットフォームのキーは、OS とアーキテクチャから決める（`windows-x64`、`windows-arm64`、`mac-os`、`mac-os-arm64`、`linux`、`linux-i386`）。
- 候補の選び方は、調査 1 の 4 節の方針に従う。完全に公開済み（availability の progress が 100）の候補を選ぶ。該当するものがない場合は、`java.no_compatible_runtime` を返す。ARM の macOS で Java 8 を必要とする場合などがこれに当たる。Rosetta を使った代替は扱わない。
- runtime manifest のファイルは、LZMA で圧縮された版ではなく、非圧縮の版を取得する。LZMA の展開のための依存を増やさないためである。
- 導入先は `shared/runtimes/<platform>/<component>/<manifest の sha1>/` とする（spec 0 の 3.4 節）。macOS と Linux では、実行権限とシンボリックリンクを manifest のとおりに再現する。
- 導入後に `java -version` を 1 回実行し、メジャーバージョンとアーキテクチャが期待どおりであることを確認してから、`java_runtimes` に登録する。一致しない場合は `java.verify_failed` を返す。

### 5.6 キャンセルとオフライン

- インストールのタスクはキャンセルできる。キャンセルした場合は、書き込み途中の `.part` を削除し、state は `installing` のまま残す。次にプレイを押すと、続きから取得する。
- インストール中にネットワークに接続できない場合は、`net.offline` を返し、state を `broken` にする。
- 起動のタスクでは、欠けたファイルがなければ、ネットワークに接続しない。

## 6. 起動

### 6.1 引数

- `arguments`（1.13 以降）と `minecraftArguments`（1.12.2 以前）の両方に対応する。
- `features` はすべて false として評価する。デモモード、解像度の指定、Quick Play は使わない。
- `arguments.default-user-jvm`（26.x で追加された部分）は使わない。`os.versionRange` の意味が未確認のためである（調査 1 の Rules の節）。代わりに、Nexora の既定の JVM 引数（`-Xmx2G` など）を付ける。`arguments.jvm` は必須の引数として使う。
- プレースホルダの値は次のとおりとする。

| プレースホルダ | 値 |
|---|---|
| `auth_player_name`、`auth_uuid`、`auth_access_token`、`user_type` | `LaunchSession` から取る |
| `auth_xuid`、`clientid` | オフラインでは空文字列。Microsoft 認証での値はフェーズ 3 で決める |
| `user_properties` | `{}` |
| `version_name`、`version_type` | version JSON の id と type |
| `game_directory` | `instances/<dir_name>/` |
| `assets_root`、`assets_index_name` | `shared/assets/` と asset index の id |
| `natives_directory` | 6.2 節の展開先 |
| `launcher_name`、`launcher_version` | `Nexora` とアプリのバージョン |
| `classpath`、`classpath_separator`、`library_directory` | ライブラリとクライアントの jar の一覧、OS の区切り文字、`shared/libraries/` |

- 未知のプレースホルダは、そのまま残して警告をログに書く。
- コマンドラインの長さが 32,767 文字を超える場合、Java 9 以降なら `@argfile` を使う。Java 8 で超える場合は `launch.command_too_long` を返す。Vanilla の正式版では超えない見込みのため、classpath 用の jar を作る代替策（調査 1 の 5 節）は実装しない。

### 6.2 natives

- 旧形式（classifier による natives）は、`cache/natives/<インスタンス ID>/` に、起動のたびに削除してから展開し直す。version JSON の `extract.exclude` に従う。
- フォルダ名にインスタンス ID を使うのは、インスタンス名の非 ASCII 文字を natives のパスに含めないためである。
- データフォルダ自体が非 ASCII 文字を含むパスにある場合の挙動は、実機で確認する（8 節）。問題が出た場合は、その時点で短い ASCII のパスへの退避を設計する。

### 6.3 プロセス

- Windows では `java.exe` を `CREATE_NO_WINDOW` で起動する（`javaw.exe` では標準出力を取得できないため）。
- 作業フォルダは `instances/<dir_name>/` とする。
- 起動したら、PID とプロセスの開始時刻を `running_sessions` に記録する。プロセスの開始時刻は `sysinfo` で取得する。PID だけでなく開始時刻も照合するのは、PID の再利用によって別のプロセスを実行中と見誤らないためである。
- 同じインスタンスが実行中の場合は、`launch.already_running` を返す。
- 起動の記録として、`instances.last_played_at` を更新する。

### 6.4 ログ

- 標準出力と標準エラーを読み、log4j の XML イベントを通常の行（時刻、レベル、スレッド、メッセージ、例外）に変換する。
- 変換には quick-xml を使う。DTD と外部エンティティの処理は無効にし、1 件のイベントのバッファには上限（1 MiB）を設ける。上限を超えた場合と、XML として解釈できない場合は、そのままの文字列を 1 行として通す。読み込みの区切りや行の区切りが、イベントの区切りと一致するとは仮定しない。
- 変換した行は、イベント `game-log`（インスタンス ID、行）で UI に通知し、`logs/game/<インスタンス ID>/<起動時刻>.log` にも書き出す。
- ログの行には、access_token の値が含まれていれば伏せ字にしてから出力する。

### 6.5 終了と再起動後の扱い

- ランチャーが監視している間にゲームが終了した場合：終了コードを記録し、`play_time_ms` に（終了時刻 − 起動時刻）を加算し、`running_sessions` の行を削除し、イベント `game-exited`（インスタンス ID、終了コード）を通知する。
- ランチャーを起動したとき、`running_sessions` の各行について、PID と開始時刻が一致するプロセスが存在するかを確認する。
  - 存在する場合は実行中として扱い、5 秒ごとに生存を確認する。標準出力は取得できないため、ログの通知は行わない。
  - 存在しない場合は終了済みとして処理する。終了コードは不明とし、異常終了の表示はしない。
- ランチャーが閉じている間に終了した場合のプレイ時間は、ゲームディレクトリの `logs/latest.log` の更新時刻を終了時刻とみなして加算する。更新時刻が起動時刻より前の場合や、ファイルがない場合は加算しない。
- ランチャーを閉じても、ゲームは終了させない（spec 0 の 3.6 節）。

### 6.6 排他

- インスタンス単位のロックは、`instance/` に、インスタンス ID ごとの `tokio::sync::Mutex` として持つ。インストールのタスクと、起動のタスクのプロセス起動までの間に取る。
- ゲームの実行中は、インストールのタスクを `launch.already_running` で拒否する。

## 7. UI

### 7.1 ホーム

- 表示するインスタンスは、`last_played_at` が最も新しいものとする。一度も遊んでいない場合は、`created_at` が最も新しいものとする。
- インスタンスが 1 つもない場合は、空の状態（`EmptyState`）と「インスタンスを作成」のボタンを表示する。
- インスタンスがある場合は、見出しの横に、2 つめ以降を作るための「インスタンスを作成」の操作を置く。表示するインスタンスを切り替える操作は、フェーズ 4 で作る。

### 7.2 作成のダイアログ

- 項目は、名前の入力欄とバージョンの選択（`Select`）の 2 つだけとする。バージョンを選ぶと、名前が空または前のバージョン名のままであれば、名前をバージョン名に合わせる。
- バージョンの一覧は `list_versions()` で取得する。オフラインでキャッシュもない場合は、エラーと再試行のボタンを表示する。
- 作成すると、ダイアログを閉じ、ホームにそのインスタンスを表示する。

### 7.3 プレイボタンの状態

| 条件 | 表示 |
|---|---|
| インストールのタスクが実行中 | 「準備中」と進捗。キャンセルできる |
| 起動のタスクが実行中 | 「起動中」と進捗。キャンセルできる |
| `running_sessions` に行がある | 「プレイ中」と経過時間。ゲームの終了まで表示する |
| 直前の終了が異常終了（終了コードが 0 以外） | 「異常終了した」と終了コードを `FailureNotice` で表示する。次の操作で消える |
| タスクが失敗 | 既存の失敗の表示（エラーコードの文言） |
| state が `installing` または `broken` | 「インストール」 |
| それ以外 | 「プレイ」 |

- タスクとインスタンスを結びつけるために、`TaskSnapshot` に `instance_id: Option<String>` を追加する。タスクの kind は `install` と `launch` とする。
- 実行中の状態は、command `list_running()` と、イベント `game-started`、`game-exited` で取得する。タスクと同じく、イベントは更新の通知として扱い、正は command の結果とする。

### 7.4 その他

- コマンドパレット：デモタスクの開始の項目を削除し、「最後のインスタンスを起動」を追加する。
- アカウント画面：開発ビルドのときだけ、オフラインアカウントの名前の入力欄を表示する（4 節）。

### 7.5 command とイベントの一覧

| 種類 | 名前 |
|---|---|
| command | `list_versions`、`create_instance`、`list_instances`、`launch_instance`、`list_running`、`get_dev_offline_username`、`set_dev_offline_username` |
| イベント | `game-started`、`game-exited`、`game-log` |

`start_demo_task` は削除する。`get_dev_offline_username` と `set_dev_offline_username` は、開発ビルドでだけ登録する。

## 8. 開発用のデータフォルダの指定

実機の確認（9.2 節）で、非 ASCII 文字を含むパスにデータフォルダを置くために、開発ビルドでだけ、環境変数 `NEXORA_DATA_DIR` でデータフォルダを指定できるようにする。リリースビルドでは、この環境変数を読まない。

## 9. エラーとテスト

### 9.1 エラーコード

spec 0 の 11.1 節の形式に従う。文言は i18n で日本語と英語を用意する。

| コード | 場面 |
|---|---|
| `net.offline` | ネットワークに接続できない |
| `net.http_status` | HTTP のエラー（引数：status） |
| `net.hash_mismatch` | 再試行してもハッシュが一致しない |
| `meta.version_not_found` | 指定したバージョンが manifest にない |
| `install.unsupported_profile` | `inheritsFrom`、または旧形式の assets |
| `java.no_compatible_runtime` | OS とアーキテクチャに合う Java がない |
| `java.verify_failed` | 導入した Java のバージョンかアーキテクチャが期待と異なる |
| `launch.already_running` | 同じインスタンスが実行中 |
| `launch.command_too_long` | Java 8 でコマンドラインの長さの上限を超えた |
| `launch.spawn_failed` | プロセスを起動できない |
| `instance.not_found` | 指定したインスタンスがない |

### 9.2 テスト

| 層 | 対象 |
|---|---|
| 単体（insta） | ルールの評価、plan、引数の組み立て。実物の version JSON（1.8.9、1.12.2、1.13、1.21.5、最新の 26.x）を `tests/fixtures/` に置いて使う |
| 単体 | log4j XML の変換（イベントが読み込みの途中で切れる場合、XML でない行、壊れた XML、上限を超えるイベント）、フォルダ名の決め方、オフラインの UUID、Java のプラットフォームのキーと候補の選び方 |
| 結合（wiremock、tempfile） | fetch の再試行、ハッシュの不一致、`.part` の後始末、キャンセル、ETag によるキャッシュ、Java の導入 |
| 結合 | プロセスの監視。ゲームの代わりに短時間で終了する子プロセスを起動し、終了の検出、プレイ時間の加算、`running_sessions` の後始末を確認する |
| UI（Vitest） | プレイボタンの状態の決め方、作成ダイアログ、ホームで表示するインスタンスの選び方 |

CI では、実際のネットワークに接続するテストを行わない。

### 9.3 実機の確認

Windows で、次の手動のチェックリストを実施する。

1. 1.8.9 の作成から起動まで（Java 8）
2. 1.12.2 の作成から起動まで（旧形式の引数と natives）
3. 最新の正式版の作成から起動まで
4. 日本語の名前のインスタンスの起動
5. `NEXORA_DATA_DIR` で日本語を含むパスにデータフォルダを置いた場合の起動
6. ゲームの実行中にランチャーを閉じて再起動すると、「プレイ中」に戻ること。ゲームを終了すると、プレイ時間が加算されること
7. ネットワークを切断した状態で、インストール済みのインスタンスを起動できること
8. インストールの途中でキャンセルし、もう一度プレイを押すと続きから取得されること
