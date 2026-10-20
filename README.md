# Nexora Launcher

A modern, lightweight, open-source launcher for Minecraft: Java Edition, built with Rust and Tauri 2.

## About

Nexora Launcher installs and launches official Minecraft: Java Edition versions. Windows is the first target, and macOS and Linux are planned.

The project is in early development. Building the Microsoft sign-in needs a client ID in the `NEXORA_MS_CLIENT_ID` environment variable; forks must register their own and pass Minecraft API review (aka.ms/mce-reviewappid).

- Done: installing and launching vanilla game versions, including the Java runtime
- Microsoft sign-in is implemented; the Minecraft API review was passed on 2026-10-17
- Instances can be created, duplicated, deleted and configured (memory, Java, JVM arguments), and the skin can be changed
- A running instance can be stopped from the launcher (a graceful close first, with a force-stop prompt if it does not exit); the app has its own title bar, toast notifications, a memory slider, and buttons that open an instance's folders
- Fabric and Quilt instances can be created, installed and launched
- Forge (Minecraft 1.8.9–1.12.2, 1.16.5–1.21.x and 26.x) and NeoForge (1.20.1–1.21.x and 26.x) instances can be created, installed and launched by the code. They are implemented and covered by automated tests that use a fake Java; verification on a real machine is still pending
- An instance's content tab lists its mods, resource packs and shaders, which can be enabled, disabled, removed, filtered, and added by drag and drop; files are identified on Modrinth by their hash
- Planned: legacy Forge (1.8.9 to 1.12.2), content from Modrinth and CurseForge, and importing instances from other launchers

### Accounts and privacy

- Players sign in with their own Microsoft account in the system browser. The launcher uses the authorization code flow with PKCE, and does not use a client secret.
- There is no backend server. All authentication happens on the player's device.
- When an instance's content tab is opened, the launcher sends the SHA-1 hashes of the installed mods, resource packs and shaders to api.modrinth.com to identify them. These are read-only lookups and include no account data.
- The Microsoft refresh token is stored only in the operating system's credential store (Windows Credential Manager, macOS Keychain, or Secret Service on Linux).
- Tokens are sent only to Microsoft, Xbox, and Minecraft services, and are never written to logs.
- Release builds require an account that owns Minecraft: Java Edition.

Nexora Launcher is not an official Minecraft product and is not approved by or associated with Mojang or Microsoft.

## 概要（日本語）

Minecraft: Java Edition 用のオープンソースのランチャーです。現在は開発の初期段階です（フェーズ 5：ローダーと MOD の管理。コンテンツタブまで）。

- インスタンスの一覧、作成、複製、削除ができます。
- インスタンスごとに、メモリ、Java、JVM 引数を設定できます。
- アカウント画面からスキンを変更できます（Minecraft API の審査は 2026-10-17 に通っています）。
- 起動中のインスタンスを、ランチャーから停止できます（まず閉じるよう頼み、終了しないときは強制終了の確認を出します）。
- 独自のタイトルバー、トースト通知、メモリのスライダー、インスタンスのフォルダを開くボタンがあります。
- インスタンスの詳細にコンテンツタブがあり、MOD、リソースパック、シェーダーを一覧できます。
  - 有効と無効を切り替えられます（ファイル名に `.disabled` を付け外しします）。ファイルのドラッグ＆ドロップで追加でき、削除もできます。
  - 名前、クライアント／サーバー、カテゴリ、状態で絞り込めます。MOD、リソースパック、シェーダーは、ファイルのハッシュから Modrinth 上の情報を調べて表示します。
  - コンテンツタブを開くと、入っている MOD、リソースパック、シェーダーのファイルの SHA-1 ハッシュを、それが何かを調べるために api.modrinth.com へ送ります（情報を読み取るだけで、アカウントの情報は送りません）。
  - Modrinth での検索、インストール、更新は次のフェーズ（5-4）で追加します。

## 開発環境

- Rust 1.96（`rust-toolchain.toml` で自動的に選ばれます）
- Node.js 24
- pnpm 11（`corepack enable` で `package.json` の版が使われます）
- Windows では WebView2（Windows 11 には標準で入っています）
- Linux では Tauri 2 の前提パッケージ（https://v2.tauri.app/start/prerequisites/ ）

## 起動

```sh
pnpm install
pnpm dev
```

開発ビルドは、識別子 `net.ny4n.nexora.dev` と、データフォルダ `Nexora-dev`（Windows では `%APPDATA%\Nexora-dev`）を使います。製品版のデータには触れません。

### 開発用のデータフォルダ

開発ビルドでは、環境変数 `NEXORA_DATA_DIR` でデータフォルダを指定できます（リリースビルドでは無視されます）。Windows の PowerShell では次のように指定します。

```powershell
$env:NEXORA_DATA_DIR = "C:\Users\me\日本語のフォルダ\nexora"; pnpm dev
```

macOS と Linux では次のように指定します。

```sh
NEXORA_DATA_DIR="$HOME/日本語のフォルダ/nexora" pnpm dev
```

### Microsoft ログインのクライアント ID

Microsoft のアカウントでログインするには、ビルド時に環境変数 `NEXORA_MS_CLIENT_ID` でクライアント ID を指定します。Windows の PowerShell では次のように指定します。

```powershell
$env:NEXORA_MS_CLIENT_ID = "<クライアント ID>"; pnpm dev
```

macOS と Linux では次のように指定します。

```sh
NEXORA_MS_CLIENT_ID="<クライアント ID>" pnpm dev
```

値が空の場合は未設定として扱います。指定されていないビルドでは、「Microsoft でログイン」のボタンを押すと「このビルドには Microsoft のクライアント ID が含まれていないため、ログインできません」（`auth.no_client_id`）と表示されます。このときも、開発用のオフラインアカウントでは起動できます。

フォークして配布する場合は、ご自身でクライアント ID を取得し、Minecraft API の利用について審査（aka.ms/mce-reviewappid）を受ける必要があります。

### 開発用のオフラインアカウント

開発ビルドでは、アカウント画面の「開発用のオフラインアカウント」の行で使うを選ぶと、そのアカウントで起動できます。名前はアカウント画面で変更できます（既定は `Player`）。リリースビルドでは起動できません。

## テスト

```sh
cargo test --workspace
pnpm --filter ui test --run
pnpm --filter ui typecheck
```

`cargo test` を実行すると、Rust の型から TypeScript の型（`ui/src/bindings/`）が生成されます。生成されたファイルはコミットしてください。

## ドキュメント

- 全体設計：`docs/specs/2026-10-15-nexora-architecture-design.md`
- フェーズ 1 の設計：`docs/specs/2026-10-15-phase1-foundation-design.md`
- フェーズ 4 の設計：`docs/specs/2026-10-19-phase4-instance-ui-design.md`
- フェーズ 5 の設計：`docs/specs/2026-10-20-phase5-loaders-and-content-design.md`
- 調査資料：`docs/research/`

## ライセンス

Apache License 2.0 です。詳しくは `LICENSE` を参照してください。
