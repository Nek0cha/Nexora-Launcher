# フェーズ 1：土台の設計

- 作成日：2026-10-15
- 状態：レビュー待ち
- 前提：`docs/specs/2026-10-15-nexora-architecture-design.md`（spec 0）
- 根拠：`docs/research/07-phase1-foundation.md`（調査 7）

## 1. 目的と範囲

以降のフェーズが乗る土台を作る。フェーズ 1 の終了時点で、次のことができる状態にする。

1. アプリが Windows で起動し、サイドバーと各画面の枠が表示される。
2. 設定画面で、テーマ（システム、ダーク、ライト）、言語（日本語、英語）、同時ダウンロード数を変更でき、再起動後も保持される。
3. ホーム画面のプレイボタンを押すと、デモタスクが進捗を流し、ボタンが進捗の表示に変わる。途中でキャンセルできる。
4. タスクトレイに実行中のタスクが表示され、そこからもキャンセルできる。
5. Ctrl+K（macOS では ⌘K）でコマンドパレットが開き、画面の移動、テーマの切り替え、デモタスクの開始ができる。
6. GitHub Actions で、Windows、Linux、macOS のビルドとテストが成功する。

### 1.1 範囲外

- ゲームのインストールと起動（フェーズ 2）
- 認証（フェーズ 3）
- インスタンスのデータ（フェーズ 4）
- 自己更新、署名、配布物の作成（フェーズ 10）
- ディープリンク（フェーズ 3 で必要になった時点で入れる）

## 2. 識別情報

| 項目 | 値 |
|---|---|
| アプリの識別子 | `net.ny4n.nexora` |
| 開発ビルドの識別子 | `net.ny4n.nexora.dev`。`src-tauri/tauri.dev.conf.json` に識別子とウィンドウタイトル（Nexora Dev）だけを書き、ルートの `dev` スクリプト（`tauri dev --config src-tauri/tauri.dev.conf.json`）で重ねる。これにより、開発版と製品版は single-instance の識別も分かれる |
| リポジトリ | `Nek0cha/Nexora-Launcher` |
| ウィンドウタイトル | Nexora |
| データの置き場所 | spec 0 の 3.4 節のとおり。開発ビルドは `Nexora-dev` |

識別子は、配布を始めたら変更しない。

## 3. リポジトリの構成

```
Nexora-Launcher/
├─ package.json               # Tauri CLI とルートのスクリプト
├─ pnpm-workspace.yaml        # パッケージ：ui
├─ Cargo.toml                 # ワークスペース（members: crates/nexora-core, src-tauri）
├─ Cargo.lock
├─ .cargo/config.toml         # TS_RS_EXPORT_DIR、TS_RS_LARGE_INT
├─ rust-toolchain.toml        # 1.96.0、rustfmt、clippy
├─ crates/nexora-core/
│  ├─ Cargo.toml
│  ├─ migrations/0001_init.sql
│  └─ src/ (lib.rs, paths.rs, db.rs, error.rs, settings.rs, task/, demo.rs)
├─ src-tauri/
│  ├─ Cargo.toml、build.rs、tauri.conf.json
│  ├─ capabilities/main.json
│  └─ src/ (main.rs, lib.rs, state.rs, events.rs, logging.rs, commands/)
├─ ui/
│  ├─ package.json、pnpm-lock.yaml、vite.config.ts、tsconfig.json、index.html
│  └─ src/ (app/, routes/, components/, features/, api/, bindings/, i18n/, styles/)
├─ .github/workflows/ci.yml
└─ docs/
```

- ルートに `package.json` と `pnpm-workspace.yaml`（パッケージは `ui`）を置く。Tauri CLI（`@tauri-apps/cli`）はルートの devDependencies に入れ、リポジトリのルートで `pnpm tauri dev` と `pnpm tauri build` を実行する。CLI はルート以下から `src-tauri/` を見つける。
- `tauri.conf.json` の `build` には、`frontendDist: "../ui/dist"`、`devUrl: "http://localhost:5173"` を設定する。`beforeDevCommand` と `beforeBuildCommand` は、`{ "script": "pnpm dev", "cwd": <ui ディレクトリ> }` のように作業ディレクトリを明示した形で書く。`cwd` の基準となるディレクトリは、実装の最初のタスクで Windows で確かめて確定する。
- Vite は `port: 5173`、`strictPort: true` とする。
- 開発の起動方法は README に書く。

## 4. Rust：nexora-core

### 4.1 paths

- `AppPaths` 構造体が、データのルートと各サブフォルダ（`instances/`、`shared/`、`cache/`、`staging/`、`backups/`、`logs/`）のパスを持つ。
- `AppPaths::resolve(profile)` は、`dirs::data_dir()`（Linux では `$XDG_DATA_HOME` を優先）に `Nexora` または `Nexora-dev`（Linux は小文字）を付け足して求める。
- `profile` は Rust のビルド設定で決める。debug ビルド（`pnpm tauri dev`）は dev、release ビルド（`pnpm tauri build`）は release とする。`cargo test` は `resolve` を使わず、常に `from_root` で一時フォルダを使う。
- テストでは `AppPaths::from_root(temp_dir)` を使う。
- 起動時に、必要なフォルダを作成する。

### 4.2 db

- 接続は 1 本だけとし、`Arc<std::sync::Mutex<rusqlite::Connection>>` で持つ。
- 呼び出し側（Tauri）は `tokio::task::spawn_blocking` の中でロックを取り、処理を終えてからロックを解放する。`.await` をまたいでロックを持たない。
- 起動時に `PRAGMA journal_mode=WAL`、`PRAGMA foreign_keys=ON`、busy timeout 5 秒を設定する。
- マイグレーションは `rusqlite_migration` で、`include_str!` で埋め込んだ SQL を順に適用する。適用済みのマイグレーションは変更しない。
- `0001_init.sql` で作るテーブルは `app_settings`（key TEXT PRIMARY KEY、value TEXT NOT NULL、updated_at INTEGER NOT NULL）だけとする。`operations` を含む他のテーブルは、使うフェーズで追加する。

### 4.3 error

- `CoreError` は enum とし、各バリアントがエラーコード（`&'static str`、例：`db.migration_failed`）と引数を返すメソッドを持つ。
- エラーコードは `<モジュール>.<内容>` の形とし、一度使ったコードの意味は変えない。
- 詳細（`detail`）には、秘密情報を含めない。

### 4.4 settings

| キー | 型 | 既定値 |
|---|---|---|
| `theme` | `system` / `dark` / `light` | `system` |
| `language` | `ja` / `en` / 未設定 | 未設定（初回起動時に OS の言語から決める） |
| `max_concurrent_downloads` | 1〜16 の整数 | 8 |

- 値は JSON の文字列として `app_settings` に保存する。
- 読み出し時に不正な値があれば、既定値を使い、警告をログに出す。

### 4.5 task

- `TaskManager` は、タスクの登録、進捗の更新、キャンセル、スナップショットの取得を行う。
- `TaskSnapshot`：`id`（UUID v7 の文字列）、`kind`、`state`、`progress`（`current`、`total`、`message_key`）、`revision`（単調増加）、`result` または `error`、`created_at`、`updated_at`。
- `state`：`queued`、`running`、`cancelling`、`succeeded`、`failed`、`cancelled`。
- 状態が変わるたびに `revision` を増やし、`TaskNotifier` トレイトの `notify(id, revision, change)` を呼ぶ。`TaskNotifier` の実装は Tauri 側が持つ。
- 進捗の通知は、1 つのタスクにつき毎秒 10 回までに間引く。終了の状態（succeeded、failed、cancelled）は、間引かずにすぐ通知する。
- キャンセルは `tokio_util::sync::CancellationToken` で協調的に行う。キャンセルを要求すると `cancelling` になり、処理が止まった時点で `cancelled` になる。
- 終了したタスクは、新しい順に 50 件まで保持し、それより古いものは破棄する。

### 4.6 demo

- `start_demo_task` は、0.1 秒ごとに進捗を 1% ずつ進め、100% で成功するタスクを作る。キャンセルに応じる。
- 失敗の表示を確かめるため、引数 `fail_at: Option<u8>` を受け取り、指定された % でエラーにできるようにする。
- フェーズ 2 で本物の起動処理に置き換えたら削除する。

## 5. Rust：src-tauri

### 5.1 起動の順序

1. `tauri-plugin-single-instance` を最初に登録する。2 つ目の起動を検出したら、既存の `main` ウィンドウを前面に出す。
2. 標準出力へのログを初期化する。
3. `AppPaths` を求め、フォルダを作成する。
4. ファイルへのログを開始する（5.4 節）。
5. DB を開いてマイグレーションを適用する。
6. `TaskManager` を作る。

起動の状態は `Starting`、`Ready`、`Failed(IpcError)` の 3 つとし、Tauri の state に登録する。

- 3〜6 が成功したら `Ready` にし、DB と `TaskManager` を持つ `Services` を登録する。
- 途中で失敗したら `Failed` にする。`Failed` の間も `app_info` と `get_startup_error` は使える。それ以外の command は `app.not_ready` のエラーを返す。
- UI は最初に `get_startup_error` を呼ぶ。エラーがあれば起動エラーの画面を表示し、通常のデータの取得は始めない。
- マイグレーションに失敗した場合、DB は失敗前の状態のまま残る。`rusqlite_migration` がマイグレーションをトランザクションの中で適用するかを実装時に確かめ、適用しない場合は自前でトランザクションを張る。

### 5.2 commands

| command | 引数 | 戻り値 |
|---|---|---|
| `app_info` | なし | バージョン、ビルドの種類（release / dev）、OS、データの置き場所 |
| `get_startup_error` | なし | 起動時のエラー、またはなし |
| `get_settings` | なし | `Settings` |
| `set_setting` | `SettingChange`（キーと値の tagged enum） | 更新後の `Settings` |
| `list_tasks` | なし | `TaskSnapshot` の配列 |
| `get_task` | `task_id` | `TaskSnapshot` |
| `cancel_task` | `task_id` | なし |
| `start_demo_task` | `fail_at`（任意） | `task_id` |

- すべての command は `Result<T, IpcError>` を返す。`IpcError` は `{ code, params, detail }` で、`params` の値は文字列、数値、真偽値のいずれかとする。
- command の中では重い処理をしない。DB の操作は `spawn_blocking` で、長い処理はタスクで行う。

### 5.3 イベント

- `task://changed`：`{ taskId, revision, change }`。`change` は `progress`、`state`、`removed` のいずれか。
- UI はイベントを受け取ったら `get_task` で最新の状態を取得する。手元より古い `revision` の通知は捨てる。

### 5.4 ログ

- `tracing-subscriber` と `tracing-appender` で、`logs/nexora.YYYY-MM-DD.log` に日ごとに書き込む。7 日より古いファイルは起動時に削除する。
- 開発ビルドでは、標準出力にも出す。
- 既定のレベルは info とし、環境変数 `NEXORA_LOG` で変更できる。
- 非同期の書き込み（`tracing_appender::non_blocking`）の `WorkerGuard` は、アプリが終了するまで保持する。
- ログに出してよいのは、エラーコード、タスクの ID と種類、パス、件数、所要時間などとする。トークン、認証の URL、リクエストやレスポンスの本文は出さない。秘密の値は `secrecy::SecretString` で持ち、`Debug` で中身が出ないようにする。

### 5.5 capabilities

- `main` ウィンドウにだけ、`core:default`、イベント、`opener` の URL を開く権限を与える。
- ウィンドウのテーマは Rust 側で変更する（7.3 節）ため、JavaScript にウィンドウ操作の追加の権限は与えない。
- `opener` で開ける URL は https に限る。

### 5.6 ウィンドウ

- OS 標準のタイトルバーを使う（`decorations: true`）。
- Windows では `transparent: true` と `windowEffects: { effects: ["mica"] }` を設定する。macOS では `sidebar` の素材を使う。Linux では効果を使わない。OS ごとの設定は、`tauri.windows.conf.json`、`tauri.macos.conf.json` に分けて書く。
- `app_info` は `material_available`（素材が実際に表示される見込みがあるか）を返す。Windows では、ビルド番号が 22000 以上で、かつレジストリの `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\EnableTransparency` が 1 のときに真とする。macOS では真、Linux では偽とする。
- UI は、`material_available` が真で、かつ `forced-colors: active` でないときだけ、サイドバーを透明にする。それ以外は不透明な surface-1 で塗る。既定は不透明とし、透明は条件を満たしたときの追加の表現とする。
- 初期サイズは 1100×720、最小サイズは 880×600 とする。
- Windows の「透明効果」がオフのとき、ハイコントラストのとき、DPI が 100%・150%・200% のときの表示を、実機で確認する。

## 6. 型の生成

- `.cargo/config.toml` の `[env]` に、`TS_RS_EXPORT_DIR = { value = "ui/src/bindings", relative = true }` と `TS_RS_LARGE_INT = "number"` を設定する。
- IPC で使う型（`Settings`、`SettingChange`、`TaskSnapshot`、`IpcError`、`AppInfo` など）に `#[derive(TS)]` と `#[ts(export)]` を付ける。
- 生成した型はコミットする。CI で、生成し直した結果がコミット済みのものと一致することを確かめる。
- 64 ビット整数は TypeScript では `number` として扱う。JavaScript の安全な整数（2^53 − 1）を超えうる値は、文字列で送る。

## 7. UI

### 7.1 ライブラリ

| 用途 | パッケージ |
|---|---|
| 基本 | react 19、react-dom 19、typescript、vite、@vitejs/plugin-react |
| スタイル | tailwindcss 4、@tailwindcss/vite、tailwindcss-react-aria-components 2 |
| 部品 | react-aria-components |
| ルーティング | react-router |
| データの取得 | @tanstack/react-query |
| 多言語 | i18next、react-i18next |
| アイコン | unplugin-icons と @iconify/json（ビルド時に埋め込む） |
| フォント | @fontsource-variable/hanken-grotesk、@fontsource-variable/murecho、@fontsource-variable/spline-sans-mono |
| Tauri | @tauri-apps/api、@tauri-apps/plugin-opener、@tauri-apps/cli |
| テスト | vitest |

正確なバージョンは、実装の開始時に npm で確かめ、lockfile で固定する。アニメーション用のライブラリは入れず、CSS の `linear()` でスプリングに近い動きを作る。

### 7.2 ディレクトリ

spec 0 の 3.2 節と、次の分担に従う。

| ディレクトリ | 内容 |
|---|---|
| `app/` | `App.tsx`、ルーターの定義、プロバイダー（QueryClient、i18n、テーマ） |
| `routes/` | home、instances、browse、accounts、settings |
| `components/` | 汎用の部品（Button、PlayButton、Sidebar、TaskTray、CommandPalette、EmptyState、Select、NumberField など） |
| `features/` | 機能ごとのロジック（`tasks/`、`settings/`） |
| `api/` | command の呼び出しとイベントの購読。`IpcError` の形を実行時に検証する |
| `bindings/` | ts-rs が生成する型（手で編集しない） |
| `i18n/` | `ja/`、`en/` に `common.json`、`errors.json`、`settings.json` |
| `styles/` | `tokens.css`、`fonts.css`、`base.css` |

### 7.3 テーマ

- 設定値が `system` の場合は `matchMedia('(prefers-color-scheme: dark)')` で決め、OS の変更に追従する。
- 決まったテーマを `<html data-theme="dark|light">` に設定する。最初の描画の前に設定し、ちらつきを防ぐ。
- ウィンドウのテーマ（タイトルバーの色）は Rust 側で変更する。`set_setting` でテーマを変えたときと起動時に、`dark` と `light` は `Window::set_theme(Some(..))`、`system` は `set_theme(None)` で OS に追従させる。

### 7.4 多言語

- 翻訳キーは TypeScript の型で補完と検査ができるようにする。
- 日本語と英語の JSON のキーが一致することを、Vitest のテストで確かめる。
- 言語が未設定の場合は、`navigator.language` が `ja` で始まれば日本語、それ以外は英語とし、設定に保存する。
- `<html lang>` を言語に合わせて切り替える。
- エラーは `errors.<code>` のキーで表示する。キーがない場合は、汎用のエラー文とエラーコードを表示する。

### 7.5 データの取得

- 設定とタスクの一覧は TanStack Query で取得する。
- タスクは、`list_tasks` の結果を 1 つのキャッシュ（タスクの ID をキーとする表）として持ち、これを UI の正とする。個別のタスクのクエリは作らない。
- イベントの購読を始めてから、`list_tasks` で最初の一覧を取得する。
- `task://changed` を受け取ったときの処理：
  - `progress` と `state`：`get_task` で取得し、手元の `revision` より新しい場合だけ表に反映する。表にない ID なら追加する。
  - `removed`：取得せずに表から削除する。
- `get_task` や `list_tasks` の応答も、手元より古い `revision` のタスクでは表を上書きしない。
- ウィンドウが再表示されたときと、イベントの購読をやり直したときは、`list_tasks` で一覧を取得し直す。

## 8. デザイン

versatile-design（design-core と web）と apple-design の手順に従う。

### 8.1 コンセプト

| 項目 | 内容 |
|---|---|
| 誰に | 友達と遊ぶ Mod 構成をいくつも抱えている Java 版のプレイヤー。起動前の数十秒で、迷わずゲームに入りたい人 |
| どんな印象を | ひんやりと澄んでいる、手応えがある、少しだけ遊び心がある |
| 何で記憶に残すか | ホーム画面のプレイボタンそのものが進捗の表示になること |
| 種類 | すべての画面がアプリ系 |
| テーマ | ダーク基本＋ライト対応 |
| 動きの強さ | 静か |
| 書体の組 | S3 |

### 8.2 文字

| 用途 | 書体 |
|---|---|
| 見出し、本文 | Hanken Grotesk（欧文）、Murecho（和文） |
| 数字（バージョン、サイズ、時間、進捗） | Spline Sans Mono |

- フォントはすべて OFL-1.1 で、`@fontsource-variable` のパッケージを同梱する。CDN からは読み込まない。
- `font-family` は欧文、和文、総称の順に書く。
- 本文は 14px、最小は 12px とする。
- 字間と行間は、組 S3 の値（和文本文 0.05em／1.8、欧文本文 0／1.55、和文見出し 0.06em／1.5、欧文見出し -0.01em／1.2）から始め、スクリーンショットで調整する。

### 8.3 色

- 色は `oklch()` で定義し、役割名のトークンだけを部品から参照する。
- トークンは次の形で Tailwind につなぐ。テーマの切り替えは変数の差し替えだけで行い、部品のクラスに `dark:` は原則として使わない。

```css
@import "tailwindcss";
@plugin "tailwindcss-react-aria-components";
@custom-variant dark (&:where([data-theme=dark], [data-theme=dark] *));

:root, [data-theme="light"] { --surface-0: oklch(0.985 0.004 220); /* ほかの役割も同様 */ color-scheme: light; }
[data-theme="dark"] { --surface-0: oklch(0.16 0.008 220); /* ほかの役割も同様 */ color-scheme: dark; }

@theme inline {
  --color-surface-0: var(--surface-0);
  /* ほかの役割も同様。これで bg-surface-0 などのクラスが生成される */
}
```
- グレーはアクセントの色相（200 度前後）にわずかに寄せる（彩度 0.006〜0.010）。
- アクセントは 1 色とし、使う場所は選択中と肯定的な状態（ダウンロード中、完了、ON）に限る。

| 役割 | ダーク | ライト |
|---|---|---|
| surface-0 | oklch(0.16 0.008 220) | oklch(0.985 0.004 220) |
| surface-1 | oklch(0.19 0.008 220) | oklch(0.965 0.005 220) |
| surface-2 | oklch(0.23 0.009 220) | oklch(0.94 0.006 220) |
| surface-3 | oklch(0.27 0.010 220) | oklch(0.91 0.007 220) |
| line | oklch(0.32 0.010 220) | oklch(0.87 0.008 220) |
| line-strong | oklch(0.42 0.010 220) | oklch(0.78 0.008 220) |
| line-control、text-faint | oklch(0.55 0.010 220) | oklch(0.60 0.010 220) |
|  | text-faint は無効な部品の文字にだけ使う。背景とのコントラストが 4.5:1 に満たないため、通常の文字には使わない | |
| text-muted | oklch(0.70 0.010 220) | oklch(0.48 0.010 220) |
| text | oklch(0.87 0.006 220) | oklch(0.28 0.008 220) |
| text-strong | oklch(0.96 0.004 220) | oklch(0.18 0.008 220) |
| accent | oklch(0.80 0.11 200) | oklch(0.52 0.09 210) |
| accent-ink（アクセントの面の上の文字） | oklch(0.18 0.02 200) | oklch(0.985 0.004 220) |
| danger | oklch(0.72 0.15 25) | oklch(0.52 0.17 25) |
| warning | oklch(0.80 0.13 80) | oklch(0.55 0.13 75) |

- 上の値は初期値であり、コントラスト（本文 4.5:1 以上、操作できる部品の輪郭 3:1 以上）を確かめて調整する。2026-10-15 の計算では、text-muted は surface-0〜3 のすべてで 4.5:1 以上（最小 4.99）、line-control は 3:1 以上（最小 3.02）、accent と accent-ink はダークで 10.45、ライトで 5.09 である。ライトの accent は、当初の値（彩度 0.13）が sRGB の色域外だったため、色域内の値に改めた。
- 値を変えたときは、sRGB の色域内であることと、上のコントラストを再計算で確かめる。
- 主要な操作は、色を足さずに明暗の反転で示す（ダークでは text-strong の面に surface-0 の文字）。反転した面は 1 つの領域に 1 つまでとする。
- 面の区切りは明度差で行う。影は浮いているもの（メニュー、ポップオーバー、ダイアログ、トースト）にだけ付け、ダークでは 1px の線も付ける。

### 8.4 余白、角丸、動き

- 余白は 4px を単位とする。
- 行の高さは、標準の一覧で 40px、2 段の一覧で 52px とする。
- 角丸、影、イージングの具体的な値は、versatile-design の `tokens-tailwind.md` と apple-design の値から決め、`tokens.css` に定義する。
- 動きは、押したときの反応と画面の切り替えにだけ使う。スプリングに近い動きは CSS の `linear()` で作る。
- `prefers-reduced-motion: reduce` のときは、拡大縮小と移動の動きを止め、不透明度の変化だけを残す。

### 8.5 画面

| 画面 | フェーズ 1 の内容 |
|---|---|
| ホーム | プレイボタン（デモタスクで動く）。インスタンス名の位置には仮の文言を置き、仮であることが分かる形にする |
| インスタンス、探す、アカウント | 空の状態。何ができるようになるかを 1 文で示す |
| 設定 | テーマ、言語、同時ダウンロード数 |

- サイドバーは、上にナビゲーション、下にタスクトレイを置く。サイドバーの面は 5.6 節の条件を満たすときだけ透明にし、それ以外は surface-1 で塗る。本文の面は surface-0 で塗る。
- `forced-colors: active` のときは、文字、輪郭、フォーカスの表示に OS のシステムカラーを使い、透明や影の表現をやめる。
- ナビゲーションの各項目にはアイコンとラベルを付ける。
- 文字は、既定で選択できないようにする（`user-select: none`）。デスクトップアプリとして、見出し、ラベル、ボタン、定型文をドラッグで選択できる状態にしない。選択できるようにするのは、利用者がコピーする意味のある内容だけとする（エラーの説明、コード、詳細、パス、バージョンなど）。入力欄は、この規則にかかわらず選択できるままにする。

### 8.6 プレイボタン

| 状態 | 表示 | 操作 |
|---|---|---|
| 準備完了 | 反転した面に「プレイ」 | 押すとタスクを開始する |
| 準備中 | アクセントの色が左から満たしていく。進捗の % を等幅の数字で表示する | ホバー中とフォーカス中は「キャンセル」と表示し、押すとキャンセルする |
| キャンセル中 | 進捗を止め、「キャンセルしています」と表示する | 操作できない |
| 完了 | 「プレイ中」と経過時間を表示し、面を一段落ち着いた色にする | フェーズ 1 では、数秒後に準備完了に戻す |
| 失敗 | エラーの色は文字にだけ使い、原因を短く表示する。「詳細」で `detail` を表示する | 押すと再試行する |

- 押したときに 0.97 倍に縮み、スプリングに近い動きで戻る。
- 進捗は `transform: scaleX()` で描画する。
- Enter と Space で操作できる。
- ボタンのアクセシブルな名前は、操作を表すものにする。準備完了では「プレイ」、準備中は見た目の表示にかかわらず常に「キャンセル」とする。進捗の割合は、ボタンの名前には含めず、`aria-describedby` で結び付けた説明（例：「準備中 37%」）で伝える。
- `aria-live="polite"` の領域で、開始、25% ごとの進捗、完了、失敗、キャンセル完了を読み上げる。状態が変わっても、フォーカスはボタンに残す。
- フォーカスリングはアクセントの色で 2px とする。
- 状態の決め方（タスクのスナップショットからボタンの状態を求める関数）は、純粋な関数として切り出してテストする。

タスクとの対応：

- フェーズ 1 では、デモタスクは同時に 1 件までとする。実行中のデモタスクがあるときに `start_demo_task` を呼ぶと、新しく作らずに実行中のタスクの ID を返す。ホームとコマンドパレットは、同じ開始処理を使う。
- プレイボタンは、タスクの一覧のうち、最新のデモタスクを表示する。UI を再読み込みした後も、一覧から同じように選ぶ。
- 開始を押してから応答が返るまでは、ボタンを押せない状態にし、連打による重複を防ぐ。
- スナップショットの状態との対応：`queued` と `running` は準備中、`cancelling` はキャンセル中、`succeeded` は完了、`failed` は失敗、`cancelled` は準備完了とする。

### 8.7 タスクトレイ

- 実行中のタスクがあるときだけ、件数を等幅の数字で表示する。
- 押すとポップオーバーが開き、各タスクの名前、進捗、キャンセルのボタンを表示する。終了したタスクは、直近の 5 件を薄く表示する。

### 8.8 コマンドパレット

- Ctrl+K（macOS では ⌘K）で開く。修飾キーの表記は OS に合わせる。
- React Aria Components の Autocomplete とモーダルで作る。
- フェーズ 1 のコマンド：各画面への移動、テーマの切り替え、デモタスクの開始。

### 8.9 デザインの検証

- versatile-design の web スキルの手順で、禁止パターンの自動チェックと、スクリーンショットによる自己批評（最大 2 周）を行う。
- ダークとライトの両方で確認する。

## 9. テスト

### 9.1 Rust

| 対象 | 内容 |
|---|---|
| db | 空の DB にマイグレーションを適用できる。2 回目の起動で何も変わらない |
| settings | 書き込んだ値を読み出せる。不正な値があれば既定値になる |
| task | 進捗から成功、キャンセルの要求から `cancelling`、`cancelled` への遷移。`revision` が単調に増える。終了したタスクの保持数の上限 |
| task の間引き | 進捗の通知が毎秒 10 回を超えない。終了の通知は間引かれない |
| error | `IpcError` の JSON の形（insta のスナップショット） |
| paths | `from_root` で全サブフォルダのパスが正しく求まる |
| 起動の失敗 | 壊れた DB ファイルや、失敗するマイグレーションで `Failed` になり、DB の内容が失敗前のまま残る |
| 秘密情報 | ダミーの秘密の値（`SecretString`）を含むエラーとタスクの失敗を作り、ログ、`IpcError`、`TaskSnapshot` のどれにもその値が出ない |
| デモタスク | 実行中に再び開始しても、2 件目が作られない |

### 9.2 UI

| 対象 | 内容 |
|---|---|
| プレイボタンの状態 | スナップショットの各状態から、正しいボタンの状態が求まる |
| タスクの購読 | 古い `revision` の通知を捨てる |
| 多言語 | 日本語と英語のキーが一致する |
| エラーの表示 | 未知のエラーコードで汎用の文が出る |

### 9.3 手動の確認

- 1 節の 1〜5 を、Windows の実機で確かめる。
- 2 つ目のアプリを起動すると、既存のウィンドウが前面に出る。
- Mica の表示（5.6 節の条件）。

## 10. CI

`.github/workflows/ci.yml` に定義する。

- 対象：`windows-latest`、`ubuntu-24.04`、`macos-latest`。`fail-fast: false`。
- Rust 1.96.0（`rust-toolchain.toml`）、Node 24、pnpm 11（`package.json` の `packageManager` で固定）。
- Ubuntu では、`libwebkit2gtk-4.1-dev`、`libappindicator3-dev`、`librsvg2-dev`、`patchelf` などの依存を入れる。
- キャッシュ：`Swatinem/rust-cache@v2`（ワークスペースのルート）、pnpm のストア。

手順：

1. `pnpm install --frozen-lockfile`（ルート）
2. `cargo fmt --all --check`
3. `cargo clippy --workspace --all-targets -- -D warnings`
4. `cargo test --workspace --locked`
5. 生成した型が、コミット済みのものと一致することを確かめる（追跡されていない新しいファイルも検出する）
6. `pnpm --dir ui typecheck`、`pnpm --dir ui test --run`
7. `pnpm tauri build --no-bundle`（tauri-action を使う場合は v1）

署名、インストーラの作成、リリースは行わない。

## 11. 依存のバージョン

2026-10-15 時点で確認した直接の依存の版である。実装の開始時に改めて確認し、lockfile で固定する。

| Rust | 版 |
|---|---|
| tauri | 2.12 |
| tauri-plugin-single-instance | 2.5 |
| tauri-plugin-opener | 2.7 |
| tokio | 1.53 |
| tokio-util | 0.7 |
| serde、serde_json | 1 |
| thiserror | 2 |
| rusqlite（bundled） | 0.40 |
| rusqlite_migration | 2.6 |
| tracing | 0.1 |
| tracing-subscriber | 0.3 |
| tracing-appender | 0.2 |
| ts-rs | 12 |
| uuid（v7） | 1 |
| dirs | 7 |
| insta、tempfile（テスト） | 1、3 |

| npm | 版 |
|---|---|
| react、react-dom | 19 |
| vite | 8 |
| tailwindcss、@tailwindcss/vite | 4.3 |
| react-aria-components | 1.22 |
| react-router | 8 |
| @tanstack/react-query | 5 |
| i18next、react-i18next | 26、17 |
| unplugin-icons | 24 |
| vitest | 5 |
| @tauri-apps/api、@tauri-apps/cli | 2.12 |

## 12. 完了の条件

- 1 節の 1〜6 がすべて満たされている。
- 9 節のテストがすべて成功している。
- CI が 3 つの OS で成功している。
- README に、開発環境の準備と起動の方法が書かれている。
